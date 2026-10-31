//! Independent value/proof, mutation and finality oracles for Responses encrypted reasoning.
use crate::events_support::{contract, envelope, metadata, origin};
use morphiecore::{
    protocol::{
        fidelity::FidelityRecords,
        openai::{
            GenerationApi,
            lowering::{lower_request, lower_response},
            responses,
        },
    },
    semantic::task::generation::*,
};
use serde_json::{Value, json};

fn wire(id: &str, token: &str, status: &str) -> Value {
    json!({"type":"reasoning","id":id,"status":status,"summary":[],"encrypted_content":token})
}
fn request(values: Value) -> morphiecore::protocol::openai::DecodedRequest {
    let mut decoded = responses::decode_generation(&json!({"input":values})).unwrap();
    decoded.fidelity.bind_replay_origin(&origin()).unwrap();
    decoded
}
fn encode(r: &GenerationRequest, f: &FidelityRecords) -> Result<Value, ()> {
    let lowered = lower_request(r, f, GenerationApi::Responses, contract()).map_err(|_| ())?;
    responses::encode_generation(&lowered).map_err(|_| ())
}

#[test]
fn opaque_value_is_semantic_and_debug_is_redacted() {
    let a = request(json!([wire("rs", "synthetic-secret-a", "completed")]));
    let b = request(json!([wire("rs", "synthetic-secret-b", "completed")]));
    assert_ne!(a.semantic, b.semantic);
    let debug = format!("{a:?}");
    assert!(!debug.contains("synthetic-secret-a"));
    let Item::Reasoning(r) = &a.semantic.items()[0].1 else {
        panic!("reasoning")
    };
    assert_eq!(
        r.replay.as_ref().unwrap().replay_token(),
        Some("synthetic-secret-a")
    );
    assert_eq!(
        encode(&a.semantic, &a.fidelity).unwrap()["input"][0]["encrypted_content"],
        "synthetic-secret-a"
    );
}

#[test]
fn foreign_reasoning_transfer_precedes_candidate_feature_admission() {
    use morphiecore::protocol::openai::{Dialect, GenerationCodec, TargetContract};
    let codec = GenerationCodec::new(GenerationApi::Responses, Dialect::Standard, None);
    let mut source = codec
        .decode_request(
            json!({"model":"synthetic","input":[
                wire("rs", "synthetic-foreign-token", "completed"),
                {"role":"user","content":"visible"}
            ]})
            .to_string()
            .as_bytes(),
        )
        .unwrap();
    source.task.fidelity.bind_replay_origin(&origin()).unwrap();
    let before = source.clone();
    let mut target = TargetContract::full();
    target.dispositions.set_native(Feature::Reasoning, false);
    target.replay_origin =
        Some(morphiecore::semantic::value::ReplayOrigin::new("different-issuer").unwrap());
    let (wire, rules) = codec
        .encode_request_with_rules(&source, "synthetic", &target)
        .unwrap();
    assert!(
        wire["input"]
            .as_array()
            .unwrap()
            .iter()
            .all(|item| item["type"] != "reasoning")
    );
    assert!(wire.to_string().contains("visible"));
    assert_eq!(rules, ["responses.transfer-visible-history.v1"]);
    assert_eq!(source, before);
}

#[test]
fn final_incomplete_reasoning_replays_exactly_but_partial_tokens_do_not() {
    for status in ["in_progress", "incomplete", "completed"] {
        let mut d = responses::decode_response(&envelope(
            "incomplete",
            json!([wire("rs", "synthetic-token", status)]),
        ))
        .unwrap();
        d.fidelity.bind_replay_origin(&origin()).unwrap();
        let m = metadata();
        let projected = lower_response(
            &d.semantic,
            &d.fidelity,
            &m,
            GenerationApi::Responses,
            contract(),
        )
        .unwrap();
        assert_eq!(
            responses::encode_response(&projected).unwrap()["output"][0]["encrypted_content"],
            "synthetic-token"
        );
        let history =
            GenerationRequest::new(d.semantic.items().to_vec(), GenerationControls::default())
                .unwrap();
        let encoded = encode(&history, &d.fidelity);
        assert_eq!(encoded.is_ok(), status != "in_progress");
        if let Ok(encoded) = encoded {
            assert_eq!(encoded["input"][0]["encrypted_content"], "synthetic-token");
            assert_eq!(encoded["input"][0]["id"], "rs");
            if status == "incomplete" {
                assert_eq!(encoded["input"][0]["status"], "incomplete");
            }
            let mut foreign = contract();
            foreign.replay_origin =
                Some(morphiecore::semantic::value::ReplayOrigin::new("different-issuer").unwrap());
            assert_eq!(
                lower_request(&history, &d.fidelity, GenerationApi::Responses, foreign).err(),
                Some(
                    morphiecore::protocol::openai::lowering::RepresentationError::ReplayOrigin
                        .into()
                )
            );
        }
    }
}
