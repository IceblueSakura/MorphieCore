//! Independent semantic mapping and append-only native continuation expectations.
use morphiecore::{
    protocol::{
        DecodedRequest,
        anthropic::{Profile, ReplayTarget},
    },
    semantic::{task::generation::*, value::Text},
};
use serde_json::{Value, json};

fn target() -> ReplayTarget {
    ReplayTarget::new("synthetic-provider", "synthetic-model").unwrap()
}
fn request() -> &'static [u8] {
    br#"{"model":"synthetic-model","max_tokens":512,"system":"Use lookup.","tools":[{"name":"lookup","strict":false,"input_schema":{"type":"object","properties":{"key":{"type":"string"}}}}],"messages":[{"role":"user","content":"Look up A."}]}"#
}
fn response() -> &'static [u8] {
    br#"{"id":"msg_synthetic","type":"message","role":"assistant","model":"synthetic-model","content":[{"type":"thinking","thinking":"","signature":"synthetic-signature"},{"type":"tool_use","id":"c","name":"lookup","input":{"key":"A"}}],"stop_reason":"tool_use","stop_sequence":null,"usage":{"input_tokens":11,"output_tokens":7}}"#
}
fn feedback(
    source: &DecodedRequest,
    observed: &morphiecore::protocol::DecodedResponse,
    execution: ToolExecution,
) -> GenerationRequest {
    let id = ItemId::scoped(LocalScope::new(3), 1);
    let mut request = ClientManaged::new(source.semantic.settings().clone())
        .unwrap()
        .select_request(&source.semantic)
        .unwrap()
        .select_response(&observed.semantic)
        .unwrap()
        .append_items(vec![(
            id,
            Item::ToolResult(ToolResult {
                call_id: Text::new("c", "call", 64).unwrap(),
                output: "actual feedback".into(),
                is_error: if matches!(&execution, ToolExecution::Succeeded) {
                    None
                } else {
                    Some(true)
                },
                execution: Some(execution),
                status: None,
                context: Default::default(),
            }),
        )])
        .unwrap()
        .build(&[])
        .unwrap();
    let mut groups = request.message_envelopes().to_vec();
    groups.push(
        MessageEnvelope::new(
            GroupId::new(LocalScope::new(3), 1),
            MessageEnvelopeRole::User,
            vec![id],
        )
        .unwrap(),
    );
    request = request.with_message_envelopes(groups).unwrap();
    request
}
#[test]
fn native_intake_keeps_independent_error_execution_progress_and_signature_owners() {
    let p = Profile::AdaptiveTextTools;
    let source = p
        .decode_generation_request(request(), LocalScope::new(1), &target())
        .unwrap();
    let observed = p
        .decode_generation_response(response(), LocalScope::new(2), &source, &target())
        .unwrap();
    assert_eq!(observed.metadata.created, None);
    assert_eq!(
        observed.semantic.progress(),
        InteractionProgress::AwaitingToolResults
    );
    assert_eq!(observed.semantic.outcome(), Outcome::Completed);
    assert!(
        matches!(&observed.semantic.items()[0].1, Item::Reasoning(r) if r.parts.is_empty() && r.replay.as_ref().unwrap().format() == ReplayFormat::AnthropicMessagesThinking)
    );
    assert!(
        matches!(&observed.semantic.items()[1].1, Item::ToolCall(c) if c.arguments.as_structured().unwrap().value() == &json!({"key":"A"}))
    );
    assert_eq!(observed.semantic.usage().unwrap().total_tokens, None);
    assert_eq!(
        observed.semantic.usage().unwrap().input_relation,
        InputTokenRelation::ExcludesCacheReadAndWrite
    );
    assert_eq!(
        p.encode_generation_response(&observed, &source, &target())
            .unwrap(),
        serde_json::from_slice::<Value>(response()).unwrap()
    );
}
#[test]
fn saved_observation_and_appended_error_encode_the_independent_native_history() {
    let p = Profile::AdaptiveTextTools;
    let source = p
        .decode_generation_request(request(), LocalScope::new(1), &target())
        .unwrap();
    let observed = p
        .decode_generation_response(response(), LocalScope::new(2), &source, &target())
        .unwrap();
    for (execution, marker) in [
        (ToolExecution::NotExecuted, Some(true)),
        (ToolExecution::Failed { code: None }, Some(true)),
        (ToolExecution::Succeeded, None),
    ] {
        let mut next = feedback(&source, &observed, execution);
        let mut items = next.items().to_vec();
        let Item::ToolResult(r) = &mut items.last_mut().unwrap().1 else {
            panic!()
        };
        r.is_error = marker;
        next = next.with_items(items).unwrap();
        let encoded = p
            .encode_generation_request(&next, &observed.fidelity, &target())
            .unwrap();
        let mut result =
            json!({"type":"tool_result","tool_use_id":"c","content":"actual feedback"});
        if let Some(marker) = marker {
            result["is_error"] = json!(marker);
        }
        assert_eq!(
            encoded.value["messages"],
            json!([
                {"role":"user","content":[{"type":"text","text":"Look up A."}]},
                {"role":"assistant","content":[{"type":"thinking","thinking":"","signature":"synthetic-signature"},{"type":"tool_use","id":"c","name":"lookup","input":{"key":"A"}}]},
                {"role":"user","content":[result]}
            ])
        );
        assert_eq!(
            encoded.omitted_execution,
            vec![ItemId::scoped(LocalScope::new(3), 1)]
        );
        let intake = p
            .decode_generation_request(
                &serde_json::to_vec(&encoded.value).unwrap(),
                LocalScope::new(4),
                &target(),
            )
            .unwrap();
        let Item::ToolResult(result) = &intake.semantic.items().last().unwrap().1 else {
            panic!()
        };
        assert_eq!(result.is_error, marker);
        assert_eq!(result.execution, None);
    }
}
#[test]
fn edited_prefix_deleted_signature_and_different_target_cannot_reuse_native_binding() {
    let p = Profile::AdaptiveTextTools;
    let source = p
        .decode_generation_request(request(), LocalScope::new(1), &target())
        .unwrap();
    let observed = p
        .decode_generation_response(response(), LocalScope::new(2), &source, &target())
        .unwrap();
    let next = feedback(&source, &observed, ToolExecution::NotExecuted);
    let mut settings = next.settings().clone();
    settings.instructions = morphiecore::semantic::value::Presence::Value(
        Text::new("edited", "instructions", 256).unwrap(),
    );
    assert!(
        p.encode_generation_request(
            &next.clone().with_settings(settings).unwrap(),
            &observed.fidelity,
            &target()
        )
        .is_err()
    );
    for changed in [
        ReplayTarget::new("other-provider", "synthetic-model").unwrap(),
        ReplayTarget::new("synthetic-provider", "other-model").unwrap(),
    ] {
        assert!(
            p.encode_generation_request(&next, &observed.fidelity, &changed)
                .is_err()
        );
    }
    let mut items = next.items().to_vec();
    let Item::Reasoning(r) = &mut items[1].1 else {
        panic!()
    };
    r.replay = None;
    assert!(
        p.encode_generation_request(
            &next.with_items(items).unwrap(),
            &observed.fidelity,
            &target()
        )
        .is_err()
    );
}
#[test]
fn raw_object_normalization_is_exact_and_does_not_bless_partial_arguments() {
    let p = Profile::AdaptiveTextTools;
    let raw = br#"{"model":"synthetic-model","max_tokens":32,"messages":[{"role":"user","content":"go"},{"role":"assistant","content":[{"type":"tool_use","id":"c","name":"lookup","input":{"z":9007199254740993.125,"a":1}}]},{"role":"user","content":[{"type":"tool_result","tool_use_id":"c","content":""}]}]}"#;
    let source = p
        .decode_generation_request(raw, LocalScope::new(1), &target())
        .unwrap();
    for malformed in ["", "[]", "{\"z\":0,\"z\":1}", "{\"z\":"] {
        let mut items = source.semantic.items().to_vec();
        let Item::ToolCall(c) = &mut items[1].1 else {
            panic!()
        };
        c.arguments = ToolArguments::Raw(malformed.into());
        let candidate = GenerationRequest::from_settings(items, source.semantic.settings().clone())
            .unwrap()
            .with_message_envelopes(source.semantic.message_envelopes().to_vec())
            .unwrap();
        assert!(
            p.encode_generation_request(&candidate, &source.fidelity, &target())
                .is_err()
        );
    }
    let mut items = source.semantic.items().to_vec();
    let Item::ToolCall(c) = &mut items[1].1 else {
        panic!()
    };
    c.arguments = ToolArguments::Raw("{ \"z\":9007199254740993.125,\"a\":1 }".into());
    let next = source.semantic.clone().with_items(items);
    // Observed calls cannot be revised under the same identity; build a new explicit request.
    assert!(next.is_err());
    let mut items = source.semantic.items().to_vec();
    let Item::ToolCall(c) = &mut items[1].1 else {
        panic!()
    };
    c.arguments = ToolArguments::Raw("{ \"z\":9007199254740993.125,\"a\":1 }".into());
    let next = GenerationRequest::from_settings(items, source.semantic.settings().clone())
        .unwrap()
        .with_message_envelopes(source.semantic.message_envelopes().to_vec())
        .unwrap();
    let encoded = p
        .encode_generation_request(&next, &source.fidelity, &target())
        .unwrap();
    let input = &encoded.value["messages"][1]["content"][0]["input"];
    assert_eq!(input["z"].to_string(), "9007199254740993.125");
    assert_eq!(
        input.as_object().unwrap().keys().collect::<Vec<_>>(),
        ["z", "a"]
    );
    assert_eq!(
        encoded.normalized_arguments,
        vec![ItemId::scoped(LocalScope::new(1), 2)]
    );
    let _independent_type_check: &Value = input;
}
#[test]
fn ordered_system_blocks_and_adaptive_controls_have_independent_semantic_owners() {
    let p = Profile::AdaptiveTextTools;
    let raw = br#"{"model":"synthetic-model","max_tokens":32,"system":[{"type":"text","text":"one"},{"type":"text","text":"two"}],"thinking":{"type":"adaptive","display":"omitted"},"output_config":{"effort":"low"},"messages":[{"role":"user","content":"go"}]}"#;
    let decoded = p
        .decode_generation_request(raw, LocalScope::new(1), &target())
        .unwrap();
    assert!(decoded.semantic.instructions().is_absent());
    let Item::Instruction(i) = &decoded.semantic.items()[0].1 else {
        panic!()
    };
    assert_eq!(i.authority, InstructionAuthority::System);
    assert_eq!(
        i.parts.iter().map(|(_, t)| t.as_str()).collect::<Vec<_>>(),
        ["one", "two"]
    );
    let encoded = p
        .encode_generation_request(&decoded.semantic, &decoded.fidelity, &target())
        .unwrap();
    assert_eq!(
        encoded.value["system"],
        json!([{"type":"text","text":"one"},{"type":"text","text":"two"}])
    );
    assert_eq!(
        encoded.value["thinking"],
        json!({"type":"adaptive","display":"omitted"})
    );
    assert_eq!(encoded.value["output_config"], json!({"effort":"low"}));
}
