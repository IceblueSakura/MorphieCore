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
fn responses_decode_uses_call_ids_not_wire_ids_or_result_positions() {
    let mut wire = expected_responses_history();
    wire["input"][2]["id"] = json!("wire_a");
    wire["input"][3]["id"] = json!("wire_b");
    let d = responses::decode_generation(&wire).unwrap();
    let (_, Item::ToolCall(call)) = &d.semantic.items()[2] else {
        panic!("call")
    };
    assert_eq!(call.call_id.as_str(), "call_a");
    assert!(d.semantic.message_owners().is_empty());
    assert_eq!(d.fidelity.response_item_id(ItemId::new(3)), Some("wire_a"));
    assert_eq!(request_wire(&d, Profile::Responses), wire);
    let chat = request_wire(&d, Profile::Chat);
    // Target assembly does not create a reported source membership.
    assert_eq!(chat["messages"][1], chat_history()["messages"][1]);
    assert_eq!(
        chat["messages"][1]["tool_calls"].as_array().unwrap().len(),
        2
    );
    assert_eq!(chat["messages"][2]["tool_call_id"], "call_b");
}
#[test]
fn strict_omission_is_source_default_not_false_and_cross_profile_fails() {
    for profile in [Profile::Chat, Profile::Responses] {
        let mut wire = if profile == Profile::Chat {
            chat_history()
        } else {
            expected_responses_history()
        };
        if profile == Profile::Chat {
            wire["messages"].as_array_mut().unwrap().truncate(1);
        } else {
            wire["input"].as_array_mut().unwrap().truncate(1);
        }
        let def = if profile == Profile::Chat {
            &mut wire["tools"][0]["function"]
        } else {
            &mut wire["tools"][0]
        };
        def.as_object_mut().unwrap().remove("strict");
        let d = if profile == Profile::Chat {
            chat::decode_generation(&wire)
        } else {
            responses::decode_generation(&wire)
        }
        .unwrap();
        let ToolDefinition::Function(t) = &d.semantic.tools()[0] else {
            panic!("function");
        };
        assert_eq!(
            t.strict,
            FunctionStrictness::Omitted(if profile == Profile::Chat {
                StrictDefault::NonStrict
            } else {
                StrictDefault::NormalizeSchema
            })
        );
        assert_eq!(request_wire(&d, profile), wire);
        let other = if profile == Profile::Chat {
            Profile::Responses
        } else {
            Profile::Chat
        };
        assert!(matches!(
            lower_request(&d.semantic, &d.fidelity, other, Contract::full()),
            Err(RepresentationError::StrictDefault)
        ));
    }
}
#[test]
fn unmodeled_tool_semantics_invalid_presence_and_missing_associations_are_rejected() {
    let base = chat_history();
    let mut cases = Vec::new();
    let mut w = base.clone();
    w["tools"][0]["type"] = json!("custom");
    cases.push(w);
    let mut w = base.clone();
    w["tools"][0]["function"]["strict"] = json!("false");
    cases.push(w);
    let mut w = base.clone();
    w["tools"][0]["function"]["parameters"] = json!([]);
    cases.push(w);
    let mut w = base.clone();
    w["tools"][0]["function"]["unknown_policy"] = json!(true);
    cases.push(w);
    let mut w = base.clone();
    w["tool_choice"] = json!({"type":"function","function":{"name":"missing"}});
    cases.push(w);
    let mut w = base.clone();
    w["messages"][2]["tool_call_id"] = json!("missing");
    cases.push(w);
    let mut w = base;
    w["stream"] = json!(true);
    cases.push(w);
    for w in cases {
        assert!(chat::decode_generation(&w).is_err());
    }
    let mut w = expected_responses_history();
    w["input"][0]["content"][0]["type"] = json!("input_image");
    assert!(responses::decode_generation(&w).is_err());
}
fn chat_response() -> Value {
    json!({"id":"response_1","object":"chat.completion","created":10,"model":"fixture-model",
        "choices":[{"index":0,"message":{"role":"assistant","content":null,"tool_calls":[
            {"id":"call_a","type":"function","function":{"name":"weather","arguments":"{\"city\":\"Paris\"}"}},
            {"id":"call_b","type":"function","function":{"name":"weather","arguments":"{}"}}
        ]},"finish_reason":"tool_calls"}],"usage":null})
}
fn responses_response() -> Value {
    json!({"id":"response_1","object":"response","created_at":10,"model":"fixture-model","status":"completed","output":[
        {"id":"item_2","type":"function_call","call_id":"call_a","name":"weather","arguments":"{\"city\":\"Paris\"}","status":"completed"},
        {"id":"item_3","type":"function_call","call_id":"call_b","name":"weather","arguments":"{}","status":"completed"}
    ],"usage":null})
}
#[test]
fn response_failures_and_independent_message_grouping_do_not_become_success() {
    let mut w = chat_response();
    w["choices"][0]["finish_reason"] = json!("length");
    assert_eq!(
        chat::decode_response(&w).unwrap().semantic.outcome(),
        Outcome::Incomplete
    );
    let mut w = chat_response();
    w["choices"][0]["finish_reason"] = json!("stop");
    assert!(chat::decode_response(&w).is_err());
    let mut w = responses_response();
    w["status"] = json!("incomplete");
    w["output"][0]["status"] = json!("incomplete");
    w["output"][1]["status"] = json!("incomplete");
    assert_eq!(
        responses::decode_response(&w).unwrap().semantic.outcome(),
        Outcome::Incomplete
    );
    let mut w = responses_response();
    w["output"][0]["status"] = json!("in_progress");
    assert!(responses::decode_response(&w).is_err());
    let mut w = responses_response();
    w["output"].as_array_mut().unwrap().insert(0, json!({"type":"message","id":"msg","role":"assistant","status":"completed","content":[{"type":"output_text","text":"Checking.","annotations":[]}]}));
    let d = responses::decode_response(&w).unwrap();
    let target = lower_response(
        &d.semantic,
        &d.fidelity,
        &d.metadata,
        Profile::Chat,
        Contract::full(),
    )
    .unwrap();
    let encoded = chat::encode_response(&target).unwrap();
    assert_eq!(encoded["choices"][0]["message"]["content"], "Checking.");
    assert_eq!(
        encoded["choices"][0]["message"]["tool_calls"],
        chat_response()["choices"][0]["message"]["tool_calls"]
    );
    assert!(d.semantic.message_owners().is_empty());
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
fn transformed_total_request_budget_includes_tools_and_history() {
    let payload = "x".repeat(MAX_TEXT_BYTES - 100);
    let items = (0..4)
        .map(|i| {
            (
                ItemId::new(i),
                Item::Message(Message {
                    phase: None,
                    role: MessageRole::User,
                    status: ItemLifecycle::Completed,
                    parts: vec![Part {
                        replay: None,
                        id: PartId::new(i),
                        content: ContentPart::Text(crate::events_support::text(&payload).into()),
                    }],
                }),
            )
        })
        .collect();
    let request = GenerationRequest::new(items, GenerationControls::default()).unwrap();
    assert!(
        request
            .with_tool_settings(
                Some(vec![ToolDefinition::Function(FunctionTool {
                    name: text("tool"),
                    output_schema: None,
                    description: Some(payload),
                    parameters: None,
                    strict: FunctionStrictness::Explicit(false),
                    dispatch: ToolDispatch::default(),
                })]),
                None,
                None
            )
            .is_err()
    );
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
fn explicit_null_tool_containers_are_rejected_not_silently_dropped() {
    // Both create types are non-nullable; explicit null never means "no tools".
    for profile in [Profile::Chat, Profile::Responses] {
        for key in ["tools", "tool_choice"] {
            let mut wire = match profile {
                Profile::Chat => json!({"messages":[{"role":"user","content":"x"}]}),
                Profile::Responses => json!({"input":"x"}),
            };
            wire[key] = Value::Null;
            let decoded = match profile {
                Profile::Chat => chat::decode_generation(&wire),
                Profile::Responses => responses::decode_generation(&wire),
            };
            assert!(decoded.is_err(), "{profile:?} {key}");
        }
    }
    // Only the Responses create type marks parallel_tool_calls nullable.
    let d = responses::decode_generation(&json!({"input":"x","parallel_tool_calls":null})).unwrap();
    assert_eq!(d.semantic.parallel_tool_calls(), None);
    assert!(
        request_wire(&d, Profile::Responses)
            .get("parallel_tool_calls")
            .is_none()
    );
    assert!(
        chat::decode_generation(
            &json!({"messages":[{"role":"user","content":"x"}],"parallel_tool_calls":null})
        )
        .is_err()
    );
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
fn program_items_round_trip_opaque_values_in_both_directions() {
    let d = responses::decode_generation(&program_history()).unwrap();
    assert_eq!(d.semantic.items()[1].1, program_items()[0].1);
    assert_eq!(d.semantic.items()[2].1, program_items()[1].1);
    assert_eq!(request_wire(&d, Profile::Responses), program_history());
}
#[test]
fn independently_constructed_program_items_encode_complete_wire_values() {
    let r = GenerationRequest::new(program_items(), GenerationControls::default()).unwrap();
    let fidelity = FidelityRecords::default();
    let t = lower_request(&r, &fidelity, Profile::Responses, Contract::full()).unwrap();
    let v = responses::encode_generation(&t).unwrap();
    assert_eq!(
        v["input"],
        json!([
            {"type":"program","id":"item_1","call_id":"call_p",
                "code":"const x = 1; return x;","fingerprint":"fp-Ω-1"},
            {"type":"program_output","id":"item_2","call_id":"call_p",
                "result":"1","status":"completed"}
        ])
    );
}
#[test]
fn program_call_id_integrity_rejects_swapped_dangling_and_duplicate_associations() {
    let d = responses::decode_generation(&program_history()).unwrap();
    let mut items = d.semantic.items().to_vec();
    if let Item::ProgramOutput(o) = &mut items[2].1 {
        o.call_id = text("call_x");
    }
    assert!(matches!(
        d.semantic.clone().with_items(items),
        Err(GenerationError::InvalidProgramOutput)
    ));
    let mut items = program_items();
    items.push((ItemId::new(3), program_items()[1].1.clone()));
    assert!(matches!(
        GenerationRequest::new(items, GenerationControls::default()),
        Err(GenerationError::InvalidProgramOutput)
    ));
    let mut items = program_items();
    items.insert(
        1,
        (
            ItemId::new(3),
            Item::Program(Program {
                call_id: text("call_p"),
                code: "other".into(),
                fingerprint: "fp-2".into(),
            }),
        ),
    );
    assert!(matches!(
        GenerationRequest::new(items, GenerationControls::default()),
        Err(GenerationError::DuplicateCall)
    ));
    let mut items = program_items();
    items.insert(
        1,
        (
            ItemId::new(3),
            Item::ToolCall(ToolCall {
                call_id: text("call_p"),
                name: text("lookup"),
                arguments: "{}".into(),
                status: ItemLifecycle::Completed,
                context: CallContext::default(),
            }),
        ),
    );
    assert!(matches!(
        GenerationRequest::new(items, GenerationControls::default()),
        Err(GenerationError::DuplicateCall)
    ));
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
