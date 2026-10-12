//! Independent Interactions request expectations; no native encoder builds the oracle.
use morphiecore::protocol::{
    interactions::encode_generation_request,
    native::ReplayTarget,
    openai::{Adapter, Dialect, Profile},
};
use morphiecore::{
    protocol::{fidelity::FidelityRecords, interactions::RequestRule, replay_envelope::issuer},
    semantic::{task::generation::*, value::Text},
};
use serde_json::json;

fn signed_history(call_anchor: &str, format: ReplayFormat) -> (GenerationRequest, FidelityRecords) {
    let source = Adapter::new(Profile::Responses, Dialect::Standard, None)
        .decode_request(
            json!({"model":"public","input":[
                {"role":"user","content":"Read."},
                {"type":"function_call","call_id":"c1","name":"read","arguments":"{}"},
                {"type":"function_call_output","call_id":"c1","output":"done"}
            ]})
            .to_string()
            .as_bytes(),
        )
        .unwrap();
    let mut fidelity = FidelityRecords::default();
    let mut signed = |n, payload: serde_json::Value, parts| {
        let id = ItemId::scoped(LocalScope::new(7), n);
        let r = ReasoningItem {
            parts,
            status: ItemLifecycle::Completed,
            replay: Some(ReplayValue::final_value(
                format,
                Text::new(payload.to_string(), "synthetic signature", MAX_TEXT_BYTES).unwrap(),
            )),
        };
        fidelity
            .record_replay(id, &r, Some(issuer("synthetic-google", "gemini-synthetic")))
            .unwrap();
        (id, Item::Reasoning(r))
    };
    let thought = signed(
        1,
        json!({"anchor":"thought","signature":"synthetic-thought"}),
        vec![(
            PartId::scoped(LocalScope::new(7), 1),
            ReasoningContent::Summary(Text::new("Plan", "summary", 64).unwrap()),
        )],
    );
    let call = signed(
        2,
        json!({"anchor":"call","call":call_anchor,"signature":"synthetic-call"}),
        vec![],
    );
    let mut items = source.task.semantic.items().to_vec();
    // Chat returns opaque carriers ahead of calls; native anchoring must not use position.
    items.splice(1..1, [thought, call]);
    let request = source
        .task
        .semantic
        .with_message_envelopes(vec![])
        .unwrap()
        .with_items(items)
        .unwrap();
    (request, fidelity)
}

#[test]
fn restores_native_signatures_by_anchor_and_drops_the_other_google_format() {
    let target = ReplayTarget::new("synthetic-google", "gemini-synthetic").unwrap();
    let (request, fidelity) = signed_history("c1", ReplayFormat::GoogleInteractionsV1Thought);
    let (wire, rules) = encode_generation_request(&request, &fidelity, &target).unwrap();
    assert_eq!(
        wire["input"],
        json!([
            {"type":"user_input","content":[{"type":"text","text":"Read."}]},
            {"type":"thought","summary":[{"type":"text","text":"Plan"}],"signature":"synthetic-thought"},
            {"type":"function_call","id":"c1","name":"read","arguments":{},"signature":"synthetic-call"},
            {"type":"function_result","call_id":"c1","name":"read","result":"done"}
        ])
    );
    assert!(rules.is_empty());
    let (request, fidelity) = signed_history("c1", ReplayFormat::GoogleGenerateContentPart);
    let (wire, rules) = encode_generation_request(&request, &fidelity, &target).unwrap();
    assert_eq!(
        wire["input"],
        json!([
            {"type":"user_input","content":[{"type":"text","text":"Read."}]},
            {"type":"model_output","content":[{"type":"text","text":"Plan"}]},
            {"type":"function_call","id":"c1","name":"read","arguments":{}},
            {"type":"function_result","call_id":"c1","name":"read","result":"done"}
        ])
    );
    assert_eq!(
        rules,
        [RequestRule::OmitForeignReplay, RequestRule::ReasoningAsText]
    );
}

#[test]
fn rejects_orphaned_or_wrong_target_signatures() {
    let (request, fidelity) =
        signed_history("missing-call", ReplayFormat::GoogleInteractionsV1Thought);
    let target = ReplayTarget::new("synthetic-google", "gemini-synthetic").unwrap();
    assert!(encode_generation_request(&request, &fidelity, &target).is_err());
    let (request, fidelity) = signed_history("c1", ReplayFormat::GoogleInteractionsV1Thought);
    let other = ReplayTarget::new("synthetic-google", "other-model").unwrap();
    assert!(encode_generation_request(&request, &fidelity, &other).is_err());
}

#[test]
fn projects_stateless_steps_tools_and_controls() {
    let request = Adapter::new(Profile::Responses, Dialect::Standard, None)
        .decode_request(json!({
            "model":"public", "instructions":"Be precise.", "max_output_tokens":128,
            "reasoning":{"effort":"medium","summary":"auto"},
            "tools":[{"type":"function","name":"read","strict":false,
                "parameters":{"type":"object","properties":{"path":{"type":"string"}}}}],
            "tool_choice":"required",
            "input":[{"role":"user","content":"Read."},
                {"type":"function_call","call_id":"c1","name":"read","arguments":"{\"path\":\"a\"}"},
                {"type":"function_call_output","call_id":"c1","output":"done"}]
        }).to_string().as_bytes()).unwrap();
    let target = ReplayTarget::new("synthetic-google", "gemini-synthetic").unwrap();
    let (wire, rules) =
        encode_generation_request(&request.task.semantic, &request.task.fidelity, &target).unwrap();
    assert_eq!(
        wire,
        json!({
            "model":"gemini-synthetic","store":false,"stream":true,
            "system_instruction":"Be precise.",
            "input":[{"type":"user_input","content":[{"type":"text","text":"Read."}]},
                {"type":"function_call","id":"c1","name":"read","arguments":{"path":"a"}},
                {"type":"function_result","call_id":"c1","name":"read","result":"done"}],
            "tools":[{"type":"function","name":"read",
                "parameters":{"type":"object","properties":{"path":{"type":"string"}}}}],
            "generation_config":{"max_output_tokens":128,"thinking_level":"medium",
                "thinking_summaries":"auto","tool_choice":"any"}
        })
    );
    assert!(rules.is_empty());
}

#[test]
fn rejects_unrepresentable_critical_tool_semantics() {
    let request = Adapter::new(Profile::Responses, Dialect::Standard, None)
        .decode_request(
            json!({
                "model":"public","input":"Read.",
                "tools":[{"type":"function","name":"read","strict":true,
                    "parameters":{"type":"object","properties":{},"additionalProperties":false}}]
            })
            .to_string()
            .as_bytes(),
        )
        .unwrap();
    assert!(
        encode_generation_request(
            &request.task.semantic,
            &request.task.fidelity,
            &ReplayTarget::new("synthetic-google", "gemini-synthetic").unwrap(),
        )
        .is_err()
    );
}
