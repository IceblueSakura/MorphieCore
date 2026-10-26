//! Response closure and pending tool results are independent; neither executes tools.
use crate::events_support::{call_item, envelope};
use morphiecore::{
    protocol::openai::{Dialect, GenerationApi, GenerationCodec, TargetContract as Contract},
    semantic::task::generation::*,
};
use serde_json::{Value, json};

fn adapter() -> GenerationCodec {
    GenerationCodec::new(GenerationApi::Responses, Dialect::Standard, None)
}
fn decode(status: &str, output: Value) -> morphiecore::protocol::openai::DecodedResponse {
    adapter()
        .decode_response(envelope(status, output).to_string().as_bytes())
        .unwrap()
}
fn calls(response: &GenerationResponse) -> Vec<(ItemId, &str)> {
    match response.continuation() {
        Continuation::Unreported => vec![],
        Continuation::ToolResults(calls) => calls
            .into_iter()
            .map(|call| (call.item, call.call_id))
            .collect(),
    }
}
fn program_pair() -> Value {
    json!([
        {"id":"p","type":"program","call_id":"pc","code":"opaque code","fingerprint":"opaque fingerprint"},
        {"id":"po","type":"program_output","call_id":"pc","result":"done","status":"completed"}
    ])
}

#[test]
fn partial_or_failed_responses_do_not_offer_completed_tool_continuation() {
    for status in ["incomplete", "failed", "cancelled"] {
        let mut wire = envelope(status, json!([call_item("a", "ca", "{", "incomplete")]));
        if status == "incomplete" {
            wire["incomplete_details"] = json!({"reason":"max_output_tokens"});
        }
        if status == "failed" {
            wire["error"] = json!({"code":"synthetic_failure","message":"failed"});
        }
        wire["usage"] = json!({"input_tokens":3,"output_tokens":2,"total_tokens":5});
        let mut decoded = adapter()
            .decode_response(wire.to_string().as_bytes())
            .unwrap();
        let usage = decoded.semantic.usage();
        let outcome = decoded.semantic.outcome();
        let details = decoded.semantic.details().clone();
        assert_eq!(decoded.semantic.continuation(), Continuation::Unreported);
        decoded.semantic = decoded.semantic.with_items(vec![]).unwrap();
        assert_eq!(decoded.semantic.outcome(), outcome);
        assert_eq!(decoded.semantic.details(), &details);
        assert_eq!(decoded.semantic.usage(), usage);
        assert_eq!(decoded.semantic.continuation(), Continuation::Unreported);
    }
    let call = decode(
        "incomplete",
        json!([call_item("a", "ca", "{", "incomplete")]),
    );
    assert!(GenerationResponse::new(call.semantic.items().to_vec(), Outcome::Completed).is_err());
}

#[test]
fn program_results_resolve_by_identity_and_edits_recompute_dependencies() {
    let mut output = program_pair().as_array().unwrap().clone();
    output.insert(
        1,
        json!({"id":"q","type":"program","call_id":"qc","code":"second","fingerprint":"fp"}),
    );
    let mut decoded = decode("completed", json!(output));
    assert_eq!(calls(&decoded.semantic), [(ItemId::new(2), "qc")]);
    let mut items = decoded.semantic.items().to_vec();
    let Item::ProgramOutput(result) = &mut items[2].1 else {
        panic!("program output")
    };
    result.call_id = crate::events_support::text("qc");
    decoded.semantic = decoded.semantic.with_items(items).unwrap();
    assert_eq!(calls(&decoded.semantic), [(ItemId::new(1), "pc")]);
    let items = decoded.semantic.items()[..2].to_vec();
    decoded.semantic = decoded.semantic.with_items(items).unwrap();
    assert_eq!(
        calls(&decoded.semantic),
        [(ItemId::new(1), "pc"), (ItemId::new(2), "qc")]
    );
}

#[test]
fn chat_retains_its_wire_finish_consistency_after_outcome_separation() {
    let chat = GenerationCodec::new(GenerationApi::Chat, Dialect::Standard, None);
    let response = |finish: &str, message: Value| json!({"id":"r","object":"chat.completion","created":0,"model":"synthetic","choices":[{"index":0,"message":message,"finish_reason":finish}]});
    let wire = response("tool_calls", json!({"role":"assistant","content":"done"}));
    assert!(chat.decode_response(wire.to_string().as_bytes()).is_err());
    // A forced tool choice reports `stop` beside its calls; the cause stays exact.
    let forced = response(
        "stop",
        json!({"role":"assistant","content":null,"tool_calls":[{"id":"c","type":"function","function":{"name":"lookup","arguments":"{}"}}]}),
    );
    let forced = chat.decode_response(forced.to_string().as_bytes()).unwrap();
    assert_eq!(
        forced.semantic.details().completion,
        Some(CompletionCause::NaturalOrStopSequence)
    );
    assert!(matches!(
        forced.semantic.continuation(),
        Continuation::ToolResults(_)
    ));
    let encoded = chat.encode_response(&forced, &Contract::full()).unwrap();
    assert_eq!(encoded["choices"][0]["finish_reason"], "stop");
    let decoded = decode(
        "completed",
        json!([call_item("a", "ca", "{}", "completed")]),
    );
    let encoded = chat.encode_response(&decoded, &Contract::full()).unwrap();
    assert_eq!(encoded["choices"][0]["finish_reason"], "tool_calls");
}
