//! Tool availability, call guarantees and replay boundaries.
use crate::events_support::{apply, close, created, envelope, metadata, start, terminal};
use morphiecore::protocol::openai::Dialect;
use morphiecore::{adapter::ClientAdapter, protocol::openai::GenerationCodec};
use morphiecore::{
    protocol::{
        conversion::generation::RepresentationError,
        fidelity::FidelityRecords,
        openai::{
            DecodedRequest, GenerationApi, TargetContract as Contract, chat, events,
            lowering::lower_request, responses,
        },
    },
    semantic::{task::generation::*, value::Text},
};
use serde_json::{Value, json};

fn text(s: &str) -> Text {
    Text::new(s, "fixture", 1024).unwrap()
}
// Official Responses ToolParam/AdditionalTools unions and ordered-additions guide:
// docs/refs/2026-10-27-client-ir-boundaries.md#tool-contract-qualification-2026-11-04
fn definition_batch() -> Value {
    json!({"type":"additional_tools","id":"batch-a","role":"developer","tools":[
        {"type":"function","name":"read","strict":false,"parameters":{"type":"object"}}
    ]})
}
#[test]
fn native_batches_preserve_old_arguments_without_choosing_a_schema_winner() {
    let batch = |key: &str| {
        json!({"type":"additional_tools","role":"developer","tools":[
        {"type":"function","name":"read","strict":false,"parameters":{
            "type":"object","properties":{key:{"type":"integer"}},"required":[key]}}]})
    };
    let source = json!({"input":[batch("old"),
        {"type":"function_call","id":"historical-call","call_id":"call","name":"read",
            "arguments":"{ \"old\":9007199254740993 }"},
        {"type":"function_call_output","call_id":"call","output":"old-result"},
        batch("new")],"tool_choice":{"type":"function","name":"read"}});
    let decoded = responses::decode_generation(&source).unwrap();
    let encoded = request_wire(&decoded, GenerationApi::Responses);
    assert_eq!(
        encoded["input"][1]["arguments"],
        "{ \"old\":9007199254740993 }"
    );
    // Native history needs no winner; collapse must not invent one.
    let Err(error) = lower_request(
        &decoded.semantic,
        &decoded.fidelity,
        GenerationApi::Chat,
        Contract::full(),
    ) else {
        panic!("collapse selected an undocumented schema winner");
    };
    assert!(matches!(
        error.cause,
        morphiecore::protocol::conversion::ConversionCause::Representation(
            RepresentationError::Admission(Feature::ToolChanges)
        )
    ));
}
#[test]
fn ordered_declarations_cannot_bypass_inventory_bounds() {
    let mut batch = definition_batch();
    batch["tools"] = (0..=MAX_TOOLS)
        .map(|i| json!({"type":"function","name":format!("f{i}")}))
        .collect();
    assert!(responses::decode_generation(&json!({"input":[batch]})).is_err());
    // A namespace container consumes capacity alongside its leaves.
    let leaves: Vec<_> = (0..MAX_TOOLS - 1)
        .map(|i| json!({"type":"function","name":format!("f{i}"),"strict":false}))
        .collect();
    let mut scoped = json!({"input":[{"type":"additional_tools","role":"developer",
        "tools":[{"type":"namespace","name":"ops","description":"Tools","tools":leaves}]}]});
    responses::decode_generation(&scoped).unwrap();
    scoped["input"][0]["tools"]
        .as_array_mut()
        .unwrap()
        .push(json!({"type":"function","name":"extra","strict":false}));
    assert!(matches!(
        responses::decode_generation(&scoped),
        Err(morphiecore::protocol::CodecError::Semantic(
            GenerationError::Limit
        ))
    ));
    // An empty native batch must not satisfy an explicit invocation requirement.
    let empty = json!({"input":[{"type":"additional_tools","role":"developer","tools":[]}],
        "tool_choice":"required"});
    assert!(responses::decode_generation(&empty).is_err());
}
#[test]
fn returned_batch_roles_do_not_gain_replay_authority() {
    let input = responses::decode_generation(&json!({"input":"go"})).unwrap();
    let codec = GenerationCodec::new(GenerationApi::Responses, Dialect::Standard, None);
    for role in ["developer", "system"] {
        let mut batch = definition_batch();
        batch["role"] = json!(role);
        let output = responses::decode_response(&envelope("completed", json!([batch]))).unwrap();
        let continuation = codec.decode_client_history(
            json!({"model":"m","input":"next"}).to_string().as_bytes(),
            Some((&input, &output)),
        );
        assert_eq!(continuation.is_ok(), role == "developer");
        let manual = responses::decode_generation(&json!({"input":[batch]}));
        assert_eq!(manual.is_ok(), role == "developer");
    }
}
#[test]
fn incomplete_terminal_cannot_close_an_unfinished_declaration_batch() {
    let mut batch = definition_batch();
    batch["tools"] = json!([]);
    let mut response = envelope("incomplete", json!([batch]));
    response["incomplete_details"] = json!({"reason":"max_output_tokens"});
    let frames = crate::wire::response_events(response);
    let mut decoder = events::EventDecoder::new(GenerationApi::Responses);
    for frame in &frames {
        decoder.push(frame).unwrap();
    }
    decoder.finish().unwrap();
    let mut missing_done = events::EventDecoder::new(GenerationApi::Responses);
    for frame in frames.iter().take(2) {
        missing_done.push(frame).unwrap();
    }
    assert!(missing_done.push(frames.last().unwrap()).is_err());
    assert!(missing_done.materialize().is_err());
    let prefix = [
        StreamEvent::Started,
        start(
            1,
            ItemKind::ToolBatch(ToolBatch {
                tools: vec![],
                role: ToolBatchRole::Developer,
            }),
        ),
    ];
    let state = apply(&prefix).unwrap();
    assert!(reduce(state, close(1, ItemLifecycle::Incomplete)).is_err());
}
fn request_wire(d: &DecodedRequest, profile: GenerationApi) -> Value {
    let t = lower_request(&d.semantic, &d.fidelity, profile, Contract::full()).unwrap();
    match profile {
        GenerationApi::Chat => chat::encode_generation(&t).unwrap(),
        GenerationApi::Responses => responses::encode_generation(&t).unwrap(),
    }
}
#[test]
/// Named-choice cardinality is directional: an OpenAI forced choice is exactly
/// one call, a Messages named choice without the parallel flag is one-or-more,
/// and each carrier must not silently strengthen or weaken the source meaning.
/// https://developers.openai.com/api/docs/guides/function-calling#tool-choice
/// https://platform.claude.com/docs/en/agents-and-tools/tool-use/parallel-tool-use
fn named_choice_cardinality_keeps_directional_meaning() {
    use morphiecore::protocol::{
        GenerationRequestEnvelope,
        anthropic::messages::{Dialect as Md, Profile, TargetContract},
        native::ReplayTarget,
    };
    let forced = chat::decode_generation(&json!({"max_completion_tokens":64,
        "messages":[{"role":"user","content":"go"}],
        "tools":[{"type":"function","function":{"name":"read","strict":false}}],
        "tool_choice":{"type":"function","function":{"name":"read"}}}))
    .unwrap();
    let forced = GenerationRequestEnvelope::from_generation(forced, "m").unwrap();
    let messages = TargetContract::new(Profile::AdaptiveTextTools, Md::Standard)
        .encode_request(&forced, &ReplayTarget::new("synthetic", "m").unwrap())
        .unwrap();
    assert_eq!(
        messages.value["tool_choice"],
        json!({"type":"tool","name":"read","disable_parallel_tool_use":true})
    );
    // The reverse direction is one-or-more, not the exactly-one forced spelling.
    let named = ClientAdapter::Messages
        .decode_client_request(
            json!({"model":"m","max_tokens":64,"messages":[{"role":"user","content":"go"}],
            "tools":[{"name":"read","input_schema":{"type":"object"}}],
            "tool_choice":{"type":"tool","name":"read"}})
            .to_string()
            .as_bytes(),
        )
        .unwrap();
    let chat = GenerationCodec::new(GenerationApi::Chat, Dialect::Standard, None)
        .encode_request(&named, "m", &Contract::full())
        .unwrap();
    assert_eq!(
        chat["tool_choice"],
        json!({"type":"allowed_tools","allowed_tools":{
        "mode":"required","tools":[{"type":"function","function":{"name":"read"}}]}})
    );
    assert!(chat.get("parallel_tool_calls").is_none());
}

#[test]
/// Replacement history (V1 -> call -> V2) collapses to the final schema while
/// the historical call keeps its original raw arguments; a forced choice over a
/// removed tool is rejected at intake instead of surviving the projection.
fn replacement_history_collapses_and_removed_tools_never_become_callable() {
    let v1 = json!({"type":"object","properties":{"old":{"type":"integer"}},
        "required":["old"],"additionalProperties":false});
    let v2 = json!({"type":"object","properties":{"new":{"type":"string"}},
        "required":["new"],"additionalProperties":false});
    let body = json!({"model":"m","max_tokens":64,
        "tools":[{"name":"f","input_schema":v1}],
        "messages":[
            {"role":"user","content":"go"},
            {"role":"assistant","content":[
                {"type":"tool_use","id":"c","name":"f","input":{"old":1}}]},
            {"role":"user","content":[
                {"type":"tool_result","tool_use_id":"c","content":"ok"}]},
            {"role":"assistant","content":"done"},
            {"role":"user","content":"again"},
            {"role":"system","content":[
                {"type":"tool_addition","tool":{"type":"tool_definition",
                    "definition":{"name":"f","input_schema":v2}}}]}]});
    let request = ClientAdapter::Messages
        .decode_client_request(body.to_string().as_bytes())
        .unwrap();
    let wire = GenerationCodec::new(GenerationApi::Chat, Dialect::Standard, None)
        .encode_request(&request, "m", &Contract::full())
        .unwrap();
    assert_eq!(
        wire["tools"],
        json!([{"type":"function","function":{
            "name":"f","parameters":v2}}])
    );
    let messages = wire["messages"].as_array().unwrap();
    assert_eq!(
        messages[1]["tool_calls"][0]["function"],
        json!({"name":"f","arguments":"{\"old\":1}"})
    );
    assert_eq!(
        messages[2],
        json!({"role":"tool","tool_call_id":"c","content":"ok"})
    );
    // A deferred tool never surfaced stays withheld; its reference surfaces it.
    let deferred = json!({"model":"m","max_tokens":64,
        "tools":[
            {"name":"read","input_schema":{"type":"object"}},
            {"name":"g","input_schema":{"type":"object"},"defer_loading":true}],
        "messages":[
            {"role":"user","content":"go"},
            {"role":"system","content":[
                {"type":"tool_removal","tool":{"type":"tool_reference","name":"read"}},
                {"type":"tool_addition","tool":{"type":"tool_reference","name":"g"}}]}]});
    let request = ClientAdapter::Messages
        .decode_client_request(deferred.to_string().as_bytes())
        .unwrap();
    let wire = GenerationCodec::new(GenerationApi::Chat, Dialect::Standard, None)
        .encode_request(&request, "m", &Contract::full())
        .unwrap();
    let names: Vec<_> = wire["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["function"]["name"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(names, vec!["g"]);
    // Removing every declaration leaves no tools carrier at all.
    let emptied = json!({"model":"m","max_tokens":64,
        "tools":[{"name":"read","input_schema":{"type":"object"}}],
        "messages":[
            {"role":"user","content":"go"},
            {"role":"system","content":[
                {"type":"tool_removal","tool":{"type":"tool_reference","name":"read"}}]}]});
    let request = ClientAdapter::Messages
        .decode_client_request(emptied.to_string().as_bytes())
        .unwrap();
    let wire = GenerationCodec::new(GenerationApi::Chat, Dialect::Standard, None)
        .encode_request(&request, "m", &Contract::full())
        .unwrap();
    assert!(wire.get("tools").is_none());
    // A forced choice over a removed declaration never reaches a target.
    let conflicting = json!({"model":"m","max_tokens":64,
        "tools":[{"name":"read","input_schema":{"type":"object"}}],
        "tool_choice":{"type":"tool","name":"read"},
        "messages":[
            {"role":"user","content":"go"},
            {"role":"system","content":[
                {"type":"tool_removal","tool":{"type":"tool_reference","name":"read"}}]}]});
    assert!(
        ClientAdapter::Messages
            .decode_client_request(conflicting.to_string().as_bytes())
            .is_err()
    );
}

#[test]
/// Preserving declaration timing must not create an illegal system turn.
/// https://platform.claude.com/docs/en/build-with-claude/mid-conversation-system-messages
fn messages_projection_respects_system_turn_placement() {
    use morphiecore::protocol::anthropic::messages::{Dialect as MessagesDialect, Profile};
    use morphiecore::protocol::{anthropic::messages::TargetContract, native::ReplayTarget};
    let schema = json!({"type":"object","properties":{"path":{"type":"string"}},
        "required":["path"],"additionalProperties":false});
    let mut contract = TargetContract::new(Profile::AdaptiveTextTools, MessagesDialect::Standard);
    contract.dispositions.set_native(Feature::ToolChanges, true);
    let target = ReplayTarget::new("synthetic", "m").unwrap();
    // A batch following a user turn converts to an ordered native addition.
    let source = GenerationCodec::new(GenerationApi::Responses, Dialect::Standard, None)
        .decode_client_request(
            json!({"model":"m","max_output_tokens":64,"input":[
                {"role":"user","content":"go"},
                {"type":"additional_tools","role":"developer","tools":[
                    {"type":"function","name":"read","strict":false,"parameters":schema}]},
                {"id":"item_1","type":"message","role":"assistant","status":"completed",
                    "content":[{"type":"output_text","text":"done","annotations":[]}]},
                {"role":"user","content":"again"}]})
            .to_string()
            .as_bytes(),
        )
        .unwrap();
    let projected = contract.encode_request(&source, &target).unwrap();
    let wire = &projected.value;
    assert!(
        wire.get("tools").is_none(),
        "the ordered addition is not hoisted to root"
    );
    let messages = wire["messages"].as_array().unwrap();
    assert_eq!(
        messages[1],
        json!({"role":"system","content":[
            {"type":"tool_addition","tool":{"type":"tool_definition",
                "definition":{"name":"read","input_schema":schema,"strict":false}}}]}),
        "one ordered native addition"
    );
    // An addition between two user turns has no legal system placement; the
    // effective active set projects at root instead.
    let source = GenerationCodec::new(GenerationApi::Responses, Dialect::Standard, None)
        .decode_client_request(
            json!({"model":"m","max_output_tokens":64,"input":[
                {"role":"user","content":"go"},
                {"type":"additional_tools","role":"developer","tools":[
                    {"type":"function","name":"read","strict":false,"parameters":schema}]},
                {"role":"user","content":"again"}]})
            .to_string()
            .as_bytes(),
        )
        .unwrap();
    let projected = contract.encode_request(&source, &target).unwrap();
    let wire = &projected.value;
    assert_eq!(
        wire["tools"],
        json!([{"name":"read","input_schema":schema,"strict":false}])
    );
    // Consecutive user turns merge; no system content survives the fallback.
    assert_eq!(
        wire["messages"],
        json!([{"role":"user","content":[
            {"type":"text","text":"go"},{"type":"text","text":"again"}]}]),
        "no system content survives an illegal converted position"
    );
}

#[test]
/// Allowed-set inventory filtering on a Messages target: the restriction and
/// required invocation survive while filtered definitions lose visibility.
fn allowed_subsets_filter_inventory_on_messages_targets() {
    use morphiecore::protocol::anthropic::messages::{Dialect as MessagesDialect, Profile};
    use morphiecore::protocol::{anthropic::messages::TargetContract, native::ReplayTarget};
    let schema = json!({"type":"object","properties":{"path":{"type":"string"}},
        "required":["path"],"additionalProperties":false});
    let source = GenerationCodec::new(GenerationApi::Chat, Dialect::Standard, None)
        .decode_client_request(
            json!({"model":"m","max_completion_tokens":64,
                "messages":[{"role":"user","content":"go"}],
                "tools":[
                    {"type":"function","function":{"name":"read","parameters":schema}},
                    {"type":"function","function":{"name":"write","parameters":schema}}],
                "tool_choice":{"type":"allowed_tools","allowed_tools":{"mode":"required",
                    "tools":[{"type":"function","function":{"name":"read"}}]}}})
            .to_string()
            .as_bytes(),
        )
        .unwrap();
    let contract = TargetContract::new(Profile::AdaptiveTextTools, MessagesDialect::Standard);
    let target = ReplayTarget::new("m", "m").unwrap();
    let projected = contract.encode_request(&source, &target).unwrap();
    let wire = &projected.value;
    let tools = wire["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0]["name"], "read");
    assert_eq!(wire["tool_choice"], json!({"type":"any"}));
    // Optional invocation keeps auto.
    let mut optional = source.clone();
    let mut settings = optional.task.semantic.settings().clone();
    settings.tool_choice = Some(ToolChoice::Allowed {
        required: false,
        tools: vec![ToolReference {
            kind: ToolKind::Function,
            name: text("read"),
            namespace: None,
        }],
    });
    optional.task.semantic = optional.task.semantic.with_settings(settings).unwrap();
    let projected = contract.encode_request(&optional, &target).unwrap();
    assert_eq!(projected.value["tool_choice"], json!({"type":"auto"}));
}

#[test]
fn lifted_batches_use_active_set_when_the_root_carrier_is_full() {
    use morphiecore::protocol::{
        anthropic::messages::{Dialect as Md, Profile, TargetContract},
        native::ReplayTarget,
    };
    let tools: Vec<_> = (0..MAX_TOOLS)
        .map(|i| {
            json!({
        "type":"function","name":format!("deferred_{i}"),"strict":false,
        "defer_loading":true,"parameters":{"type":"object"}})
        })
        .collect();
    let source = GenerationCodec::new(GenerationApi::Responses, Dialect::Standard, None)
        .decode_client_request(
            json!({"model":"m","max_output_tokens":64,"tools":tools,"input":[
            {"type":"additional_tools","role":"developer","tools":[
                {"type":"function","name":"f","strict":false,"parameters":{"type":"object"}}]},
            {"role":"user","content":"go"}]})
            .to_string()
            .as_bytes(),
        )
        .unwrap();
    let mut contract = TargetContract::new(Profile::AdaptiveTextTools, Md::Standard);
    contract.dispositions.set_native(Feature::ToolChanges, true);
    let projected = contract
        .encode_request(&source, &ReplayTarget::new("m", "m").unwrap())
        .unwrap();
    // The viable projection exposes f, never the 128 unsurfaced root definitions.
    assert_eq!(
        projected.value["tools"],
        json!([{"name":"f","input_schema":{"type":"object"},"strict":false}])
    );
}
#[test]
fn programmatic_results_require_matching_creator() {
    let source = json!({"input":[
        {"type":"program","id":"p","call_id":"prog","code":"return lookup();","fingerprint":"fp"},
        {"type":"function_call","call_id":"c","name":"lookup","arguments":"{}","namespace":"pkg",
            "async":true,"caller":{"type":"program","caller_id":"prog"}},
        {"type":"function_call_output","call_id":"c","output":"ok",
            "caller":{"type":"program","caller_id":"prog"}},
        {"type":"program_output","id":"po","call_id":"prog","result":"ok","status":"completed"}
    ],"tools":[{"type":"function","name":"lookup","async":true,"defer_loading":true,
        "allowed_callers":["programmatic"]}]});
    responses::decode_generation(&source).unwrap();
    let mut orphan = source.clone();
    orphan["input"].as_array_mut().unwrap().remove(0);
    assert!(matches!(
        responses::decode_generation(&orphan),
        Err(morphiecore::protocol::CodecError::Semantic(
            GenerationError::InvalidDependency
        ))
    ));
    let mut mismatch = source;
    mismatch["input"][2]["caller"]["caller_id"] = json!("different-program");
    assert!(matches!(
        responses::decode_generation(&mismatch),
        Err(morphiecore::protocol::CodecError::Semantic(
            GenerationError::InvalidToolResult
        ))
    ));
}
#[test]
fn program_items_have_no_chat_projection() {
    let source = responses::decode_generation(&json!({"input":[
        {"role":"user","content":"run"},
        {"type":"program","id":"pg_1","call_id":"call_p","code":"run()","fingerprint":"fp"},
        {"type":"program_output","id":"pg_2","call_id":"call_p","result":"1","status":"completed"}
    ]}))
    .unwrap();
    assert_eq!(
        lower_request(
            &source.semantic,
            &source.fidelity,
            GenerationApi::Chat,
            Contract::full()
        )
        .err(),
        Some(RepresentationError::Tools.into())
    );
}
#[test]
fn program_output_waits_for_status_without_reordering_following_items() {
    for status in [ItemLifecycle::Completed, ItemLifecycle::Incomplete] {
        let fidelity = FidelityRecords::default();
        let mut encoder = events::EventEncoder::new(GenerationApi::Responses, metadata()).unwrap();
        let mut wire = encoder.encode(&StreamEvent::Started, &fidelity).unwrap();
        let mut held = vec![
            start(
                1,
                ItemKind::ProgramOutput {
                    call_id: text("c"),
                    result: "value".into(),
                },
            ),
            start(2, ItemKind::Message { phase: None }),
        ];
        held.extend(crate::events_support::part(2, 2, PartKind::Text, "after"));
        held.push(close(2, ItemLifecycle::Completed));
        for event in held {
            assert!(encoder.encode(&event, &fidelity).unwrap().is_empty());
        }
        wire.extend(encoder.encode(&close(1, status), &fidelity).unwrap());
        wire.extend(
            encoder
                .encode(
                    &terminal(if status == ItemLifecycle::Completed {
                        StreamTerminal::Completed
                    } else {
                        StreamTerminal::Incomplete
                    }),
                    &fidelity,
                )
                .unwrap(),
        );
        encoder.finish().unwrap();
        let added: Vec<_> = wire
            .iter()
            .filter(|e| e["type"] == "response.output_item.added")
            .collect();
        assert_eq!(added[0]["item"]["type"], "program_output");
        assert_eq!(added[1]["item"]["type"], "message");
        let mut decoder = events::EventDecoder::new(GenerationApi::Responses);
        for frame in &wire {
            decoder.push(frame).unwrap();
        }
        assert_eq!(
            decoder.materialize().unwrap().semantic.items()[0]
                .1
                .lifecycle(),
            Some(status)
        );
    }
}
#[test]
fn program_event_stream_decodes_and_changed_or_unfinished_snapshots_fail() {
    let mut d = events::EventDecoder::new(GenerationApi::Responses);
    d.push(&created()).unwrap();
    d.push(&json!({"type":"response.output_item.added","output_index":0,"item":{"id":"pg_1","type":"program","call_id":"call_p","code":"c()","fingerprint":"fp-1"}})).unwrap();
    d.push(&json!({"type":"response.output_item.done","output_index":0,"item":{"id":"pg_1","type":"program","call_id":"call_p","code":"c()","fingerprint":"fp-1"}})).unwrap();
    d.push(&json!({"type":"response.output_item.added","output_index":1,"item":{"id":"pg_2","type":"program_output","call_id":"call_p","result":"1","status":"completed"}})).unwrap();
    d.push(&json!({"type":"response.output_item.done","output_index":1,"item":{"id":"pg_2","type":"program_output","call_id":"call_p","result":"1","status":"completed"}})).unwrap();
    let out = json!([
        {"id":"pg_1","type":"program","call_id":"call_p","code":"c()","fingerprint":"fp-1"},
        {"id":"pg_2","type":"program_output","call_id":"call_p","result":"1","status":"completed"}
    ]);
    d.push(&json!({"type":"response.completed","response":envelope("completed",out)}))
        .unwrap();
    let r = d.materialize().unwrap().semantic;
    assert_eq!(r.outcome(), Outcome::Completed);
    let mut d = events::EventDecoder::new(GenerationApi::Responses);
    d.push(&created()).unwrap();
    d.push(&json!({"type":"response.output_item.added","output_index":0,"item":{"id":"pg_1","type":"program","call_id":"call_p","code":"c()","fingerprint":"fp-1"}})).unwrap();
    assert!(d
        .push(&json!({"type":"response.output_item.done","output_index":0,"item":{"id":"pg_1","type":"program","call_id":"call_p","code":"c()","fingerprint":"fp-OTHER"}}))
        .is_err());
    let mut d = events::EventDecoder::new(GenerationApi::Responses);
    d.push(&created()).unwrap();
    d.push(&json!({"type":"response.output_item.added","output_index":0,"item":{"id":"pg_2","type":"program_output","call_id":"call_p","result":"1","status":"completed"}})).unwrap();
    assert!(d
        .push(&json!({"type":"response.output_item.done","output_index":0,"item":{"id":"pg_2","type":"program_output","call_id":"call_p","result":"1"}}))
        .is_err());
    let mut d = events::EventDecoder::new(GenerationApi::Responses);
    d.push(&created()).unwrap();
    assert!(d
        .push(&json!({"type":"response.output_item.added","output_index":0,"item":{"id":"pg_2","type":"program_output","call_id":"call_p","result":"1"}}))
        .is_err());
}
