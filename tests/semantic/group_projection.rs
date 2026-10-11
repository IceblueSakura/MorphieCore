//! Standard Responses omits native container boundaries, never tool identity or outcome.
use crate::events_support::text;
use morphiecore::{
    lowering::generation::RepresentationError,
    protocol::{
        fidelity::FidelityRecords,
        openai::{
            Profile, TargetCaps as Contract,
            lowering::{lower_request, lower_response},
            responses,
        },
    },
    semantic::task::generation::*,
};
use serde_json::{Value, json};

fn source(content: Value, finish: &str) -> Value {
    json!({"id":"r","object":"chat.completion","created":0,"model":"synthetic","choices":[{"index":0,"message":{"role":"assistant","content":content,"tool_calls":[{"id":"c","type":"function","function":{"name":"lookup","arguments":"{}"}}]},"finish_reason":finish}],"usage":null})
}

#[test]
fn owner_only_encrypted_reasoning_survives_independent_message_envelope_projection() {
    use morphiecore::{
        protocol::openai::{Adapter, Dialect},
        semantic::value::ReplayOrigin,
    };
    let origin = ReplayOrigin::new("synthetic-owner-scope").unwrap();
    let adapter = Adapter::new(Profile::Chat, Dialect::OpenRouter, Some(origin.clone()));
    let mut wire = source(Value::Null, "tool_calls");
    wire["choices"][0]["message"]["reasoning_details"] = json!([
        {"type":"reasoning.encrypted","format":"openai-responses-v1","index":0,
         "id":"rs","data":"synthetic-final"}]);
    let decoded = adapter
        .decode_response(wire.to_string().as_bytes())
        .unwrap();
    let original = decoded.semantic.clone();
    let mut contract = Contract::full();
    contract.replay_origin = Some(origin.clone());
    let target = lower_response(
        &decoded.semantic,
        &decoded.fidelity,
        &decoded.metadata,
        Profile::Responses,
        contract.clone(),
    )
    .unwrap();
    let output = responses::encode_response(&target).unwrap();
    assert_eq!(output["output"][0]["id"], "rs");
    assert_eq!(output["output"][0]["encrypted_content"], "synthetic-final");
    assert_eq!(output["output"][1]["content"], json!([]));
    assert_eq!(output["output"][2]["call_id"], "c");
    assert_eq!(output["output"][2]["arguments"], "{}");
    assert_eq!(decoded.semantic, original);
    assert!(target.semantic().message_envelopes().is_empty());
    let history = GenerationRequest::new(
        decoded.semantic.items().to_vec(),
        GenerationControls::default(),
    )
    .unwrap()
    .with_message_envelopes(decoded.semantic.message_envelopes().to_vec())
    .unwrap();
    let projected = lower_request(
        &history,
        &decoded.fidelity,
        Profile::Responses,
        contract.clone(),
    )
    .unwrap();
    let input = responses::encode_generation(&projected).unwrap();
    assert_eq!(input["input"][0]["encrypted_content"], "synthetic-final");
    assert_eq!(input["input"][2]["call_id"], "c");
    let client = Adapter::new(
        Profile::Responses,
        Dialect::MorphieCore,
        Some(origin.clone()),
    );
    let mut saved = output["output"].as_array().unwrap().clone();
    saved.push(json!({"type":"function_call_output","call_id":"c","output":"synthetic-result"}));
    let returned = client
        .decode_request(
            json!({"model":"synthetic","input":saved})
                .to_string()
                .as_bytes(),
        )
        .unwrap();
    let upstream = adapter
        .encode_request(&returned, "synthetic", &adapter.contract(&contract))
        .unwrap();
    assert_eq!(
        upstream["messages"][0]["reasoning_details"][0]["data"],
        "synthetic-final"
    );
    assert_eq!(upstream["messages"][0]["tool_calls"][0]["id"], "c");
    assert_eq!(upstream["messages"][1]["tool_call_id"], "c");
    let mut changed = decoded.semantic.items().to_vec();
    let Item::Reasoning(reasoning) = &mut changed[0].1 else {
        panic!("reasoning owner")
    };
    reasoning.replay = Some(ReplayValue::final_value(
        ReplayFormat::ResponsesEncrypted,
        text("changed"),
    ));
    let changed = decoded.semantic.clone().with_items(changed).unwrap();
    assert_eq!(
        lower_response(
            &changed,
            &decoded.fidelity,
            &decoded.metadata,
            Profile::Responses,
            contract.clone()
        )
        .err(),
        Some(RepresentationError::ReplayOrigin)
    );
    contract.replay_origin = Some(ReplayOrigin::new("wrong-scope").unwrap());
    assert_eq!(
        lower_response(
            &decoded.semantic,
            &decoded.fidelity,
            &decoded.metadata,
            Profile::Responses,
            contract
        )
        .err(),
        Some(RepresentationError::ReplayOrigin)
    );
}

#[test]
fn empty_owner_still_participates_in_wire_identity_checks() {
    let decoded = responses::decode_response(&json!({"id":"r","object":"response","created_at":0,"model":"synthetic","status":"completed","output":[
        {"id":"owner","type":"message","role":"assistant","content":[],"status":"completed"},
        {"id":"call","type":"function_call","call_id":"c","name":"lookup","arguments":"{}","status":"completed"}
    ]})).unwrap();
    let mut collision = FidelityRecords::default();
    collision
        .record_response_item_id(ItemId::new(2), "item_1")
        .unwrap();
    assert!(matches!(
        lower_response(
            &decoded.semantic,
            &collision,
            &decoded.metadata,
            Profile::Responses,
            Contract::full()
        ),
        Err(RepresentationError::Metadata)
    ));
}
