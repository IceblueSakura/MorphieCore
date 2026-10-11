//! Independent oracles for the function-tool migration, without providers or production routing.
use crate::events_support::{apply, close, created, encode, envelope, metadata, start, terminal};
use morphiecore::{
    lowering::generation::RepresentationError,
    protocol::{
        fidelity::FidelityRecords,
        openai::{
            DecodedRequest, Profile, TargetCaps as Contract, chat, events,
            lowering::{lower_request, lower_response},
            responses,
        },
    },
    semantic::{task::generation::*, value::Text},
};
use serde_json::{Value, json};

fn text(s: &str) -> Text {
    Text::new(s, "fixture", 1024).unwrap()
}
fn chat_history() -> Value {
    json!({"messages":[
        {"role":"user","content":"weather?"},
        {"role":"assistant","content":"Checking.","tool_calls":[
            {"id":"call_a","type":"function","function":{"name":"weather","arguments":"{ \"city\": \"Paris\" }"}},
            {"id":"call_b","type":"function","function":{"name":"weather","arguments":"{\"city\":\"Rome\"}"}}
        ]},
        {"role":"tool","tool_call_id":"call_b","content":""},
        {"role":"tool","tool_call_id":"call_a","content":"sunny"}
    ],"tools":[{"type":"function","function":{"name":"weather","description":"","parameters":{"type":"object","properties":{"city":{"type":"string"}}},"strict":false}}],
    "tool_choice":{"type":"function","function":{"name":"weather"}},"parallel_tool_calls":false})
}
fn request_wire(d: &DecodedRequest, profile: Profile) -> Value {
    let t = lower_request(&d.semantic, &d.fidelity, profile, Contract::full()).unwrap();
    match profile {
        Profile::Chat => chat::encode_generation(&t).unwrap(),
        Profile::Responses => responses::encode_generation(&t).unwrap(),
    }
}
fn expected_responses_history() -> Value {
    json!({"input":[
        {"type":"message","role":"user","content":[{"type":"input_text","text":"weather?"}]},
        {"type":"message","role":"assistant","content":[{"type":"output_text","text":"Checking.","annotations":[]}]},
        {"type":"function_call","call_id":"call_a","name":"weather","arguments":"{ \"city\": \"Paris\" }"},
        {"type":"function_call","call_id":"call_b","name":"weather","arguments":"{\"city\":\"Rome\"}"},
        {"type":"function_call_output","call_id":"call_b","output":""},
        {"type":"function_call_output","call_id":"call_a","output":"sunny"}
    ],"tools":[{"type":"function","name":"weather","description":"","parameters":{"type":"object","properties":{"city":{"type":"string"}}},"strict":false}],
    "tool_choice":{"type":"function","name":"weather"},"parallel_tool_calls":false})
}
#[test]
fn chat_tool_text_arrays_preserve_parts_and_project_only_final_ir() {
    for parts in [
        json!([]),
        json!([{"type":"text","text":""}]),
        json!([{"type":"text","text":"sunny"},{"type":"text","text":""}]),
    ] {
        let mut c = chat_history();
        c["messages"][3]["content"] = parts.clone();
        let mut r = expected_responses_history();
        r["input"][5]["output"] = json!(
            parts
                .as_array()
                .unwrap()
                .iter()
                .map(|p| json!({"type":"input_text","text":p["text"]}))
                .collect::<Vec<_>>()
        );
        let mut decoded = chat::decode_generation(&c).unwrap();
        let Item::ToolResult(result) = &decoded.semantic.items()[5].1 else {
            panic!("result");
        };
        let ToolOutput::Parts(values) = &result.output else {
            panic!("parts");
        };
        assert_eq!(result.call_id.as_str(), "call_a");
        assert_eq!(values.len(), parts.as_array().unwrap().len());
        // Tool results are request history, not assistant response output.
        assert!(
            GenerationResponse::new(
                vec![decoded.semantic.items()[5].clone()],
                Outcome::Completed
            )
            .is_err()
        );
        if !values.is_empty() {
            let mut cached = r.clone();
            cached["input"][5]["output"][0]["prompt_cache_breakpoint"] = json!({"mode":"explicit"});
            let cached = responses::decode_generation(&cached).unwrap();
            assert!(
                lower_request(
                    &cached.semantic,
                    &cached.fidelity,
                    Profile::Chat,
                    Contract::full()
                )
                .is_err()
            );
        }
        assert_eq!(request_wire(&decoded, Profile::Chat), c);
        assert_eq!(request_wire(&decoded, Profile::Responses), r);
        let independent = responses::decode_generation(&r).unwrap();
        assert_eq!(request_wire(&independent, Profile::Responses), r);
        // Responses call items have no Chat carrier-message association.
        let projected = request_wire(&responses::decode_generation(&r).unwrap(), Profile::Chat);
        let result = projected["messages"].as_array().unwrap().last().unwrap();
        assert_eq!(result["tool_call_id"], "call_a");
        assert_eq!(result["content"], parts);
        let mut items = decoded.semantic.items().to_vec();
        let Item::ToolResult(result) = &mut items[5].1 else {
            unreachable!()
        };
        result.output = ToolOutput::Parts(vec![
            (PartId::new(900), ToolResultPart::Text(text("replacement"))),
            (PartId::new(901), ToolResultPart::Text(text("inserted"))),
        ]);
        decoded.semantic = decoded.semantic.with_items(items).unwrap();
        assert_eq!(
            request_wire(&decoded, Profile::Chat)["messages"][3]["content"],
            json!([{"type":"text","text":"replacement"},{"type":"text","text":"inserted"}])
        );
        let mut items = decoded.semantic.items().to_vec();
        let Item::ToolResult(result) = &mut items[5].1 else {
            unreachable!()
        };
        result.output = ToolOutput::Parts(vec![]);
        decoded.semantic = decoded.semantic.with_items(items).unwrap();
        assert_eq!(
            request_wire(&decoded, Profile::Chat)["messages"][3]["content"],
            json!([])
        );
    }
    for bad in [
        Value::Null,
        json!([{"type":"image_url","image_url":{"url":"https://example.test/a"}}]),
        json!([{"type":"text","text":1}]),
        json!([{"type":"text","text":"x","extra":true}]),
    ] {
        let mut c = chat_history();
        c["messages"][3]["content"] = bad;
        assert!(chat::decode_generation(&c).is_err());
    }
}

#[test]
fn chat_allowed_functions_use_their_own_shell_and_keep_selection_authoritative() {
    for mode in ["auto", "required"] {
        let mut c = chat_history();
        c["messages"] = json!([{"role":"user","content":"weather?"}]);
        c["tool_choice"] = json!({"type":"allowed_tools","allowed_tools":{"mode":mode,"tools":[{"type":"function","function":{"name":"weather"}}]}});
        let mut r = expected_responses_history();
        r["input"].as_array_mut().unwrap().truncate(1);
        r["tool_choice"] = json!({"type":"allowed_tools","mode":mode,"tools":[{"type":"function","name":"weather"}]});
        let mut d = chat::decode_generation(&c).unwrap();
        assert_eq!(
            d.semantic.tool_choice(),
            Some(&ToolChoice::Allowed {
                required: mode == "required",
                tools: vec![ToolReference {
                    kind: ToolKind::Function,
                    name: text("weather"),
                    namespace: None,
                }]
            })
        );
        assert_eq!(request_wire(&d, Profile::Chat), c);
        assert_eq!(request_wire(&d, Profile::Responses), r);
        assert_eq!(
            request_wire(&responses::decode_generation(&r).unwrap(), Profile::Chat)["tool_choice"],
            c["tool_choice"]
        );
        let mut s = d.semantic.settings().clone();
        s.tool_choice = Some(ToolChoice::None);
        d.semantic = d.semantic.with_settings(s).unwrap();
        assert_eq!(request_wire(&d, Profile::Chat)["tool_choice"], "none");
        let mut s = d.semantic.settings().clone();
        s.tool_choice = None;
        d.semantic = d.semantic.with_settings(s).unwrap();
        assert!(request_wire(&d, Profile::Chat).get("tool_choice").is_none());
        for refs in [
            json!([]),
            json!([{"type":"function","function":{"name":"absent"}}]),
            json!([{"type":"function","function":{"name":"weather"}},{"type":"function","function":{"name":"weather"}}]),
            json!([{"type":"custom","name":"weather"}]),
        ] {
            let mut bad = c.clone();
            bad["tool_choice"]["allowed_tools"]["tools"] = refs;
            assert!(chat::decode_generation(&bad).is_err());
        }
    }
}

#[test]
fn bounded_values_fidelity_collisions_and_codec_profile_mismatch_fail() {
    let mut wire = chat_history();
    wire["messages"][1]["tool_calls"][0]["function"]["arguments"] =
        json!("x".repeat(MAX_TEXT_BYTES + 1));
    assert!(chat::decode_generation(&wire).is_err());
    let mut f = FidelityRecords::default();
    f.record_response_item_id(ItemId::new(1), "same").unwrap();
    assert!(f.record_response_item_id(ItemId::new(2), "same").is_err());
    let d = chat::decode_generation(&chat_history()).unwrap();
    let t = lower_request(&d.semantic, &d.fidelity, Profile::Chat, Contract::full()).unwrap();
    assert!(responses::encode_generation(&t).is_err());
}

#[test]
fn semantic_construction_cannot_bypass_identifier_or_control_validation() {
    let empty = Text::allowing_empty("", "fixture", 1).unwrap();
    let call = Item::ToolCall(ToolCall {
        call_id: empty.clone(),
        name: text("f"),
        arguments: "{}".into(),
        status: ItemLifecycle::Completed,
        context: CallContext::default(),
    });
    assert!(
        GenerationRequest::new(vec![(ItemId::new(1), call)], GenerationControls::default())
            .is_err()
    );
    let d = chat::decode_generation(&chat_history()).unwrap();
    let tool = ToolDefinition::Function(FunctionTool {
        name: empty,
        output_schema: None,
        description: None,
        parameters: None,
        strict: FunctionStrictness::Explicit(false),
        dispatch: ToolDispatch::default(),
    });
    assert!(
        d.semantic
            .clone()
            .with_tools(vec![tool], ToolChoice::Auto)
            .is_err()
    );
    let mut controls = GenerationControls::default();
    controls.max_output_tokens = Some(0);
    assert!(GenerationRequest::new(d.semantic.items().to_vec(), controls).is_err());
}
#[test]
fn active_tool_dispatch_and_instruction_lifecycle_round_trip_without_chat_projection() {
    let source = json!({"input":[
        {"type":"message","id":"ins","role":"developer","status":"in_progress","content":"hold"},
        {"type":"function_call","call_id":"c","name":"lookup","arguments":"{}","namespace":"pkg","async":true,"caller":{"type":"program","caller_id":"prog"}}
    ],"tools":[{"type":"function","name":"lookup","async":true,"defer_loading":true,"allowed_callers":["programmatic"]}]});
    let decoded = responses::decode_generation(&source).unwrap();
    let Item::Instruction(instruction) = &decoded.semantic.items()[0].1 else {
        panic!("instruction")
    };
    assert_eq!(instruction.status, Some(ItemLifecycle::InProgress));
    let Item::ToolCall(call) = &decoded.semantic.items()[1].1 else {
        panic!("call")
    };
    assert_eq!(
        call.context.namespace.as_ref().map(|v| v.as_str()),
        Some("pkg")
    );
    assert!(call.context.async_call);
    assert!(matches!(
        call.context.caller,
        Some(CallOrigin::Program { .. })
    ));
    let ToolDefinition::Function(tool) = &decoded.semantic.tools()[0] else {
        panic!("tool")
    };
    assert!(tool.dispatch.async_call && tool.dispatch.defer_loading);
    assert_eq!(
        tool.dispatch.allowed_callers,
        Some(vec![CallerMode::Programmatic])
    );
    let encoded = request_wire(&decoded, Profile::Responses);
    assert_eq!(encoded["input"][0]["status"], json!("in_progress"));
    assert_eq!(encoded["input"][0]["id"], json!("ins"));
    assert_eq!(encoded["input"][1]["namespace"], json!("pkg"));
    assert_eq!(encoded["input"][1]["caller"]["type"], json!("program"));
    assert_eq!(encoded["tools"][0]["defer_loading"], json!(true));
    assert!(
        lower_request(
            &decoded.semantic,
            &decoded.fidelity,
            Profile::Chat,
            Contract::full()
        )
        .is_err()
    );
    let completed =
        responses::decode_generation(&json!({"input":[{"role":"developer","content":"done"}]}))
            .unwrap();
    let Item::Instruction(instruction) = &completed.semantic.items()[0].1 else {
        panic!("complete instruction")
    };
    assert_eq!(instruction.status, None);
    assert!(
        request_wire(&completed, Profile::Responses)["input"][0]
            .get("status")
            .is_none()
    );
}
fn program_history() -> Value {
    json!({"input":[
        {"type":"message","role":"user","content":[{"type":"input_text","text":"run"}]},
        {"type":"program","id":"pg_1","call_id":"call_p","code":"const x = 1; return x;","fingerprint":"fp-Ω-1"},
        {"type":"program_output","id":"pg_2","call_id":"call_p","result":"1","status":"completed"}
    ]})
}
fn program_items() -> Vec<(ItemId, Item)> {
    vec![
        (
            ItemId::new(1),
            Item::Program(Program {
                call_id: text("call_p"),
                code: "const x = 1; return x;".into(),
                fingerprint: "fp-Ω-1".into(),
            }),
        ),
        (
            ItemId::new(2),
            Item::ProgramOutput(ProgramOutput {
                call_id: text("call_p"),
                result: "1".into(),
                status: ItemLifecycle::Completed,
            }),
        ),
    ]
}
#[test]
fn program_output_requires_terminal_status_and_complete_opaque_fields() {
    for (index, key) in [(1, "fingerprint"), (1, "id"), (2, "status"), (2, "result")] {
        let mut wire = program_history();
        wire["input"][index].as_object_mut().unwrap().remove(key);
        assert!(responses::decode_generation(&wire).is_err());
    }
    for status in ["in_progress", "failed", "queued"] {
        let mut wire = program_history();
        wire["input"][2]["status"] = json!(status);
        assert!(responses::decode_generation(&wire).is_err());
    }
    let mut wire = program_history();
    wire["input"][1]["fingerprint"] = Value::Null;
    assert!(responses::decode_generation(&wire).is_err());
    let mut items = program_items();
    if let Item::ProgramOutput(o) = &mut items[1].1 {
        o.status = ItemLifecycle::InProgress;
    }
    assert!(matches!(
        GenerationRequest::new(items, GenerationControls::default()),
        Err(GenerationError::InvalidProgramOutput)
    ));
}
#[test]
fn program_items_have_no_chat_projection() {
    let d = responses::decode_generation(&program_history()).unwrap();
    assert!(matches!(
        lower_request(&d.semantic, &d.fidelity, Profile::Chat, Contract::full()),
        Err(RepresentationError::Tools)
    ));
    let r = GenerationResponse::new(
        vec![(ItemId::new(1), program_items()[0].1.clone())],
        Outcome::Completed,
    )
    .unwrap();
    assert!(matches!(
        lower_response(
            &r,
            &FidelityRecords::default(),
            &metadata(),
            Profile::Chat,
            Contract::full()
        ),
        Err(RepresentationError::Tools)
    ));
    let mut e = events::EventEncoder::new(Profile::Chat, metadata()).unwrap();
    assert!(
        e.encode(&start(1, program_kind()), &FidelityRecords::default())
            .is_err()
    );
}
fn program_kind() -> ItemKind {
    ItemKind::Program {
        call_id: text("call_p"),
        code: "c()".into(),
        fingerprint: "fp-1".into(),
    }
}
#[test]
fn program_items_are_atomic_event_items_with_owner_bound_snapshots() {
    let mut e = vec![StreamEvent::Started, start(1, program_kind())];
    e.push(close(1, ItemLifecycle::Completed));
    e.push(start(
        2,
        ItemKind::ProgramOutput {
            call_id: text("call_p"),
            result: "1".into(),
        },
    ));
    e.push(close(2, ItemLifecycle::Completed));
    e.push(terminal(StreamTerminal::Completed));
    let wire = encode(&e, Profile::Responses, &FidelityRecords::default());
    let added: Vec<_> = wire
        .iter()
        .filter(|v| v["type"] == "response.output_item.added")
        .collect();
    assert_eq!(
        added[0]["item"],
        json!({"id":"item_1","type":"program","call_id":"call_p","code":"c()","fingerprint":"fp-1"})
    );
    assert_eq!(
        added[1]["item"],
        json!({"id":"item_2","type":"program_output","call_id":"call_p","result":"1"})
    );
    let done: Vec<_> = wire
        .iter()
        .filter(|v| v["type"] == "response.output_item.done")
        .collect();
    assert_eq!(done[1]["item"]["status"], json!("completed"));
    assert_eq!(done[0]["item"].get("status"), None);
    assert_eq!(
        wire.last().unwrap()["response"]["output"][0]["type"],
        json!("program")
    );
    assert!(apply(&e).is_ok());
    let mut dup = e.clone();
    dup.insert(
        2,
        start(
            3,
            ItemKind::Program {
                call_id: text("call_p"),
                code: "x".into(),
                fingerprint: "fp-2".into(),
            },
        ),
    );
    assert!(apply(&dup).is_err());
}
#[test]
fn program_event_stream_decodes_and_changed_or_unfinished_snapshots_fail() {
    let mut d = events::EventDecoder::new(Profile::Responses);
    d.push(&created()).unwrap();
    d.push(&json!({"type":"response.output_item.added","output_index":0,"item":{"id":"pg_1","type":"program","call_id":"call_p","code":"c()","fingerprint":"fp-1"}})).unwrap();
    d.push(&json!({"type":"response.output_item.done","output_index":0,"item":{"id":"pg_1","type":"program","call_id":"call_p","code":"c()","fingerprint":"fp-1"}})).unwrap();
    d.push(&json!({"type":"response.output_item.added","output_index":1,"item":{"id":"pg_2","type":"program_output","call_id":"call_p","result":"1"}})).unwrap();
    d.push(&json!({"type":"response.output_item.done","output_index":1,"item":{"id":"pg_2","type":"program_output","call_id":"call_p","result":"1","status":"completed"}})).unwrap();
    let out = json!([
        {"id":"pg_1","type":"program","call_id":"call_p","code":"c()","fingerprint":"fp-1"},
        {"id":"pg_2","type":"program_output","call_id":"call_p","result":"1","status":"completed"}
    ]);
    d.push(&json!({"type":"response.completed","response":envelope("completed",out)}))
        .unwrap();
    let r = d.materialize().unwrap().semantic;
    assert_eq!(r.outcome(), Outcome::Completed);
    assert_eq!(r.continuation(), Continuation::Unreported);
    assert_eq!(r.items()[1].1, program_items()[1].1);
    let mut d = events::EventDecoder::new(Profile::Responses);
    d.push(&created()).unwrap();
    d.push(&json!({"type":"response.output_item.added","output_index":0,"item":{"id":"pg_1","type":"program","call_id":"call_p","code":"c()","fingerprint":"fp-1"}})).unwrap();
    assert!(d
        .push(&json!({"type":"response.output_item.done","output_index":0,"item":{"id":"pg_1","type":"program","call_id":"call_p","code":"c()","fingerprint":"fp-OTHER"}}))
        .is_err());
    let mut d = events::EventDecoder::new(Profile::Responses);
    d.push(&created()).unwrap();
    d.push(&json!({"type":"response.output_item.added","output_index":0,"item":{"id":"pg_2","type":"program_output","call_id":"call_p","result":"1"}})).unwrap();
    assert!(d
        .push(&json!({"type":"response.output_item.done","output_index":0,"item":{"id":"pg_2","type":"program_output","call_id":"call_p","result":"1"}}))
        .is_err());
    let mut d = events::EventDecoder::new(Profile::Responses);
    d.push(&created()).unwrap();
    assert!(d
        .push(&json!({"type":"response.output_item.added","output_index":0,"item":{"id":"pg_2","type":"program_output","call_id":"call_p","result":"1","status":"completed"}}))
        .is_err());
}
