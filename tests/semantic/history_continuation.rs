//! Final history owns call/result dependencies; no view authorizes another attempt.
use crate::events_support::text;
use morphiecore::{
    adapter::{Adapter, Dialect},
    lowering::generation::{
        GenerationRepresentationContract as Contract, RepresentationError, lower_request,
    },
    protocol::{
        fidelity::FidelityRecords,
        openai::{CodecError, Profile, responses},
    },
    semantic::task::generation::*,
};
use serde_json::json;

fn call(item: u64, id: &str) -> (ItemId, Item) {
    (
        ItemId::new(item),
        Item::ToolCall(ToolCall {
            call_id: text(id),
            name: text("lookup"),
            arguments: "not parsed".into(),
            message: None,
            status: ItemLifecycle::Completed,
            context: CallContext::default(),
        }),
    )
}
fn result(item: u64, id: &str) -> (ItemId, Item) {
    (
        ItemId::new(item),
        Item::ToolResult(ToolResult {
            execution: None,
            call_id: text(id),
            output: "".into(),
            status: None,
            context: CallContext::default(),
        }),
    )
}
fn history(items: Vec<(ItemId, Item)>) -> GenerationRequest {
    GenerationRequest::new(items, GenerationControls::default()).unwrap()
}
fn references(view: Continuation<'_>) -> Vec<(ItemId, &str)> {
    match view {
        Continuation::Unreported => vec![],
        Continuation::ToolResults(calls) => {
            calls.into_iter().map(|c| (c.item, c.call_id)).collect()
        }
    }
}

#[test]
fn parallel_calls_resolve_individually_then_a_new_response_has_its_own_requirements() {
    let request = history(vec![call(10, "a"), call(20, "b")]);
    let response = GenerationResponse::new(request.items().to_vec(), Outcome::Completed).unwrap();
    let expected = [(ItemId::new(10), "a"), (ItemId::new(20), "b")];
    assert_eq!(references(request.continuation()), expected);
    assert_eq!(references(response.continuation()), expected);

    let mut items = request.items().to_vec();
    items.push(result(900, "b"));
    let partial = request.with_items(items).unwrap();
    assert_eq!(references(partial.continuation()), [(ItemId::new(10), "a")]);
    let mut items = partial.items().to_vec();
    items.push(result(901, "a"));
    let answered = partial.with_items(items).unwrap();
    assert_eq!(answered.continuation(), Continuation::Unreported);
    // Reported results are neither a new response nor evidence of turn completion.
    assert_eq!(response.outcome(), Outcome::Completed);
    assert_eq!(references(response.continuation()), expected);

    let next = GenerationResponse::new(vec![call(30, "c")], Outcome::Completed).unwrap();
    let mut items = answered.items().to_vec();
    items.extend_from_slice(next.items());
    let subsequent = answered.with_items(items).unwrap();
    assert_eq!(
        references(subsequent.continuation()),
        [(ItemId::new(30), "c")]
    );
    assert_eq!(references(next.continuation()), [(ItemId::new(30), "c")]);
}

#[test]
fn replacement_insertion_deletion_and_reorder_recompute_final_identity_dependencies() {
    let request = history(vec![call(10, "a"), call(20, "b"), result(900, "b")]);
    let mut items = request.items().to_vec();
    let Item::ToolResult(result) = &mut items[2].1 else {
        panic!("result")
    };
    result.call_id = text("a");
    items.swap(0, 1);
    items.insert(2, call(30, "c"));
    let edited = request.clone().with_items(items).unwrap();
    assert_eq!(
        references(edited.continuation()),
        [(ItemId::new(20), "b"), (ItemId::new(30), "c")]
    );
    let deleted = edited.retain_items(|id, _| id != ItemId::new(900)).unwrap();
    assert_eq!(
        references(deleted.continuation()),
        [
            (ItemId::new(20), "b"),
            (ItemId::new(10), "a"),
            (ItemId::new(30), "c")
        ]
    );
    assert_eq!(references(request.continuation()), [(ItemId::new(10), "a")]);
    assert_eq!(
        request
            .clone()
            .retain_items(|id, _| id != ItemId::new(20))
            .unwrap_err(),
        GenerationError::InvalidToolResult
    );
    let repaired = request
        .retain_items(|id, _| !matches!(id.get(), 20 | 900))
        .unwrap();
    assert_eq!(
        references(repaired.continuation()),
        [(ItemId::new(10), "a")]
    );
}

#[test]
fn unresolved_or_reported_partial_values_do_not_become_execution_readiness() {
    let mut items = vec![call(10, "a"), call(20, "b"), result(900, "b")];
    let Item::ToolResult(result) = &mut items[2].1 else {
        panic!("result")
    };
    result.status = Some(ItemLifecycle::InProgress);
    assert_eq!(
        history(items.clone()).continuation(),
        Continuation::Unreported
    );
    let Item::ToolResult(result) = &mut items[2].1 else {
        panic!("result")
    };
    result.status = Some(ItemLifecycle::Incomplete);
    let request = history(items.clone());
    assert_eq!(references(request.continuation()), [(ItemId::new(10), "a")]);
    assert!(
        matches!(&request.items()[2].1, Item::ToolResult(r) if r.status == Some(ItemLifecycle::Incomplete))
    );
    for status in [ItemLifecycle::Incomplete, ItemLifecycle::InProgress] {
        let mut items = items.clone();
        let Item::ToolCall(call) = &mut items[0].1 else {
            panic!("call")
        };
        call.status = status;
        assert_eq!(history(items).continuation(), Continuation::Unreported);
    }
    let request = GenerationRequest::from_settings(
        vec![],
        GenerationSettings {
            instructions: morphiecore::semantic::value::Presence::Value(text("instructions only")),
            ..GenerationSettings::default()
        },
    )
    .unwrap();
    assert_eq!(request.continuation(), Continuation::Unreported);
}

#[test]
fn custom_and_program_results_match_kind_identity_and_final_order() {
    let custom = (
        ItemId::new(20),
        Item::CustomCall(CustomCall {
            call_id: text("custom"),
            name: text("lookup"),
            input: "opaque input".into(),
            context: CallContext::default(),
        }),
    );
    let program = (
        ItemId::new(30),
        Item::Program(Program {
            call_id: text("program"),
            code: "opaque code".into(),
            fingerprint: "opaque fingerprint".into(),
        }),
    );
    let request = history(vec![call(10, "function"), custom, program]);
    assert_eq!(
        references(request.continuation()),
        [
            (ItemId::new(10), "function"),
            (ItemId::new(20), "custom"),
            (ItemId::new(30), "program")
        ]
    );
    let custom_result = (
        ItemId::new(900),
        Item::CustomResult(ToolResult {
            execution: None,
            call_id: text("custom"),
            output: "done".into(),
            status: None,
            context: CallContext::default(),
        }),
    );
    let program_result = (
        ItemId::new(901),
        Item::ProgramOutput(ProgramOutput {
            call_id: text("program"),
            result: "partial artifact".into(),
            status: ItemLifecycle::Incomplete,
        }),
    );
    let mut items = request.items().to_vec();
    items.extend([program_result.clone(), custom_result]);
    assert_eq!(
        references(history(items).continuation()),
        [(ItemId::new(10), "function")]
    );
    assert_eq!(
        GenerationRequest::new(
            vec![program_result.clone(), request.items()[2].clone()],
            GenerationControls::default()
        )
        .unwrap_err(),
        GenerationError::InvalidProgramOutput
    );
    // Response program outputs are self-describing; an earlier output does not
    // resolve a later call, nor does the wrong result kind resolve a function.
    let earlier = GenerationResponse::new(
        vec![
            (
                ItemId::new(901),
                Item::ProgramOutput(ProgramOutput {
                    call_id: text("program"),
                    result: "done".into(),
                    status: ItemLifecycle::Completed,
                }),
            ),
            request.items()[2].clone(),
        ],
        Outcome::Completed,
    )
    .unwrap();
    assert_eq!(
        references(earlier.continuation()),
        [(ItemId::new(30), "program")]
    );
    let wrong = GenerationResponse::new(
        vec![
            call(10, "x"),
            (
                ItemId::new(900),
                Item::ProgramOutput(ProgramOutput {
                    call_id: text("x"),
                    result: "not a function result".into(),
                    status: ItemLifecycle::Completed,
                }),
            ),
        ],
        Outcome::Completed,
    )
    .unwrap();
    assert_eq!(references(wrong.continuation()), [(ItemId::new(10), "x")]);
    for invalid in [
        vec![
            call(10, "x"),
            (
                ItemId::new(900),
                Item::CustomResult(ToolResult {
                    execution: None,
                    call_id: text("x"),
                    output: "wrong kind".into(),
                    status: None,
                    context: CallContext::default(),
                }),
            ),
        ],
        vec![result(900, "x"), call(10, "x")],
        vec![call(10, "x"), result(900, "missing")],
        vec![call(10, "x"), result(900, "x"), result(901, "x")],
    ] {
        assert_eq!(
            GenerationRequest::new(invalid, GenerationControls::default()).unwrap_err(),
            GenerationError::InvalidToolResult
        );
    }
}

#[test]
fn pending_program_is_semantic_data_but_not_a_complete_responses_replay() {
    let mut wire = json!({"input":[
        {"type":"program","id":"program-owner","call_id":"p","code":"opaque code","fingerprint":"opaque fingerprint"}
    ]});
    assert!(matches!(
        responses::decode_generation(&wire),
        Err(CodecError::Invalid("program history"))
    ));
    let request = history(vec![(
        ItemId::new(30),
        Item::Program(Program {
            call_id: text("p"),
            code: "opaque code".into(),
            fingerprint: "opaque fingerprint".into(),
        }),
    )]);
    assert_eq!(references(request.continuation()), [(ItemId::new(30), "p")]);
    assert!(matches!(
        lower_request(
            &request,
            &FidelityRecords::default(),
            Profile::Responses,
            Contract::full()
        ),
        Err(RepresentationError::Tools)
    ));
    wire["input"].as_array_mut().unwrap().push(json!({"type":"program_output","id":"result-owner","call_id":"p","result":"partial artifact","status":"incomplete"}));
    let complete = responses::decode_generation(&wire).unwrap();
    assert_eq!(complete.semantic.continuation(), Continuation::Unreported);
    let target = lower_request(
        &complete.semantic,
        &complete.fidelity,
        Profile::Responses,
        Contract::full(),
    )
    .unwrap();
    assert_eq!(responses::encode_generation(&target).unwrap(), wire);
}

#[test]
fn native_history_replay_keeps_wire_authority_and_never_emits_a_continuation_field() {
    for profile in [Profile::Chat, Profile::Responses] {
        let (mut wire, local_call, pending_call) = if profile == Profile::Chat {
            (
                json!({"model":"synthetic","messages":[
                    {"role":"user","content":"lookup"},
                    {"role":"assistant","content":null,"tool_calls":[
                        {"id":"a","type":"function","function":{"name":"lookup","arguments":"not parsed"}},
                        {"id":"b","type":"function","function":{"name":"lookup","arguments":"{}"}}
                    ]},
                    {"role":"tool","tool_call_id":"b","content":""}
                ]}),
                ItemId::new(3),
                "a",
            )
        } else {
            (
                json!({"model":"synthetic","store":false,"input":[
                    {"type":"message","role":"user","content":[{"type":"input_text","text":"lookup"}]},
                    {"type":"function_call","id":"wire-a","call_id":"a","name":"lookup","arguments":"not parsed"},
                    {"type":"function_call","id":"wire-b","call_id":"b","name":"lookup","arguments":"{}"},
                    {"type":"function_call_output","call_id":"b","output":""}
                ]}),
                ItemId::new(2),
                "a",
            )
        };
        let adapter = Adapter::new(profile, Dialect::Standard, None);
        let decoded = adapter
            .decode_request(&serde_json::to_vec(&wire).unwrap())
            .unwrap();
        assert_eq!(
            references(decoded.task.semantic.continuation()),
            [(local_call, pending_call)]
        );
        assert_eq!(
            adapter
                .encode_request(&decoded, "synthetic", &Contract::full())
                .unwrap(),
            wire
        );
        wire[if profile == Profile::Chat {
            "messages"
        } else {
            "input"
        }]
        .as_array_mut()
        .unwrap()
        .push(if profile == Profile::Chat {
            json!({"role":"tool","tool_call_id":"a","content":"done"})
        } else {
            json!({"type":"function_call_output","call_id":"a","output":"done"})
        });
        let decoded = adapter
            .decode_request(&serde_json::to_vec(&wire).unwrap())
            .unwrap();
        assert_eq!(
            decoded.task.semantic.continuation(),
            Continuation::Unreported
        );
        assert_eq!(
            adapter
                .encode_request(&decoded, "synthetic", &Contract::full())
                .unwrap(),
            wire
        );
    }
}

#[test]
fn typed_function_history_keeps_raw_arguments_while_summary_and_results_are_edited() {
    use morphiecore::semantic::value::Presence;
    let raw = " {\"n\":9007199254740993, \"label\":\"synthetic\"} ";
    let mut pending = call(10, "call-a");
    let Item::ToolCall(value) = &mut pending.1 else {
        panic!("call")
    };
    value.arguments = raw.into();
    let settings = GenerationSettings {
        instructions: Presence::Value(text("lookup")),
        reasoning: ReasoningRequest::present(None, Some(ReasoningSummary::Auto)),
        tools: Some(vec![ToolDefinition::Function(FunctionTool {
            name: text("lookup"),
            description: None,
            parameters: Some(json!({"type":"object"})),
            strict: FunctionStrictness::Explicit(false),
            output_schema: None,
            dispatch: ToolDispatch::default(),
        })]),
        ..Default::default()
    };
    let request = GenerationRequest::from_settings(vec![pending], settings).unwrap();
    let fidelity = FidelityRecords::default();
    let wire = responses::encode_generation(
        &lower_request(&request, &fidelity, Profile::Responses, Contract::full()).unwrap(),
    )
    .unwrap();
    assert_eq!(
        wire,
        json!({
            "instructions":"lookup",
            "input":[{"type":"function_call","call_id":"call-a","name":"lookup","arguments":raw}],
            "reasoning":{"summary":"auto"},
            "tools":[{"type":"function","name":"lookup","parameters":{"type":"object"},"strict":false}]
        })
    );

    // Saving and reloading standard history is not a second semantic authority.
    let adapter = Adapter::new(Profile::Responses, Dialect::Standard, None);
    let mut saved = wire;
    saved["model"] = json!("synthetic");
    let mut replay = adapter
        .decode_request(&serde_json::to_vec(&saved).unwrap())
        .unwrap();
    assert_eq!(
        references(replay.task.semantic.continuation()),
        [(ItemId::new(1), "call-a")]
    );
    let mut items = replay.task.semantic.items().to_vec();
    items.push(result(900, "call-a"));
    let answered = replay.task.semantic.clone().with_items(items).unwrap();
    assert_eq!(answered.continuation(), Continuation::Unreported);
    for summary in [
        Presence::Null,
        Presence::Value(ReasoningSummary::Detailed),
        Presence::Absent,
    ] {
        let mut settings = answered.settings().clone();
        settings.reasoning.summary = summary.clone();
        replay.task.semantic = answered.clone().with_settings(settings).unwrap();
        let projected = adapter
            .encode_request(&replay, "synthetic", &Contract::full())
            .unwrap();
        assert_eq!(
            projected["input"],
            json!([
                {"type":"function_call","call_id":"call-a","name":"lookup","arguments":raw},
                {"type":"function_call_output","call_id":"call-a","output":""}
            ])
        );
        let expected = match summary {
            Presence::Null => json!({"summary":null}),
            Presence::Value(_) => json!({"summary":"detailed"}),
            Presence::Absent => json!({}),
        };
        assert_eq!(projected["reasoning"], expected);
        assert!(projected.get("continuation").is_none());
    }
    let removed = answered
        .clone()
        .retain_items(|id, _| id != ItemId::new(900))
        .unwrap();
    assert_eq!(
        references(removed.continuation()),
        [(ItemId::new(1), "call-a")]
    );
    assert!(answered.retain_items(|id, _| id != ItemId::new(1)).is_err());
    assert_eq!(request.reasoning().summary(), Some(ReasoningSummary::Auto));
    assert_eq!(
        references(request.continuation()),
        [(ItemId::new(10), "call-a")]
    );
}
