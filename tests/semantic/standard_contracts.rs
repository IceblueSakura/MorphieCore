//! Source-derived requirements; ignored probes remain unmet, not expected-failure passes.
//! Sources and scope: docs/refs/2026-10-23-stable-protocol-contracts.md.
use morphiecore::protocol::{
    anthropic::messages::client::decode_client_request,
    openai::{Adapter, Dialect, Profile, TargetCaps},
};
use serde_json::{Value, json};

fn adapter(profile: Profile) -> Adapter {
    Adapter::new(profile, Dialect::Standard, None)
}
fn messages() -> Value {
    json!({"model":"public", "max_tokens":32, "messages":[{"role":"user","content":"Hello"}]})
}

#[test]
fn prediction_survives_same_protocol_projection() {
    let adapter = adapter(Profile::Chat);
    let wire = json!({"model":"public", "messages":[{"role":"user","content":"Edit"}],
        "prediction":{"type":"content","content":"hello"}});
    let request = adapter
        .decode_client_request(wire.to_string().as_bytes())
        .unwrap();
    assert_eq!(
        request
            .task
            .semantic
            .settings()
            .prediction
            .as_ref()
            .unwrap()[0]
            .as_str(),
        "hello"
    );
    assert!(request.task.fidelity.normalizations().is_empty());
    let original = request.clone();
    let output = adapter
        .encode_request(&request, "public", &TargetCaps::full())
        .unwrap();
    assert_eq!(output["prediction"], wire["prediction"]);
    let mut restricted = TargetCaps::full();
    restricted.prediction = false;
    let (omitted, rules) = adapter
        .encode_request_with_rules(&request, "public", &restricted)
        .unwrap();
    assert!(omitted.get("prediction").is_none());
    assert!(rules.contains(&"generation.omit-prediction.v1"));
    let (omitted, rules) = self::adapter(Profile::Responses)
        .encode_request_with_rules(&request, "public", &TargetCaps::full())
        .unwrap();
    assert!(omitted.get("prediction").is_none());
    assert!(rules.contains(&"generation.omit-prediction.v1"));
    assert_eq!(request, original);
    let mut settings = request.task.semantic.settings().clone();
    settings.prediction = Some(vec![
        morphiecore::semantic::value::Text::allowing_empty("A", "test", 8).unwrap(),
        morphiecore::semantic::value::Text::allowing_empty("B", "test", 8).unwrap(),
    ]);
    let mut edited = request.clone();
    edited.task.semantic = edited
        .task
        .semantic
        .with_settings(settings.clone())
        .unwrap();
    let (wire, rules) = adapter
        .encode_request_with_rules(&edited, "public", &TargetCaps::full())
        .unwrap();
    assert_eq!(
        wire["prediction"],
        json!({"type":"content","content":[
        {"type":"text","text":"A"},{"type":"text","text":"B"}]})
    );
    assert!(!rules.contains(&"generation.omit-prediction.v1"));
    settings.prediction.as_mut().unwrap().resize(
        morphiecore::semantic::task::generation::MAX_ITEMS + 1,
        morphiecore::semantic::value::Text::allowing_empty("", "test", 8).unwrap(),
    );
    assert!(
        request
            .task
            .semantic
            .clone()
            .with_settings(settings)
            .is_err()
    );
}

#[test]
#[ignore = "known standards gap: named Chat participants lack a semantic owner"]
fn named_chat_participant_survives_same_protocol_projection() {
    let adapter = adapter(Profile::Chat);
    let wire = json!({"model":"public", "messages":[{"role":"assistant","name":"planner","content":"Plan A"}]});
    let request = adapter
        .decode_client_request(wire.to_string().as_bytes())
        .expect("official participant name");
    let output = adapter
        .encode_request(&request, "public", &TargetCaps::full())
        .unwrap();
    assert_eq!(output["messages"][0]["name"], "planner");
}

#[test]
fn responses_omitted_strict_can_reach_native_best_effort_fallback() {
    let adapter = adapter(Profile::Responses);
    let mut wire = json!({"model":"public","input":"Choose", "tools":[{"type":"function","name":"choose",
        "strict":false,"parameters":{"type":"object","properties":{"x":{"oneOf":[{"type":"string"},{"type":"integer"}]}}}}]});
    let explicit_false = adapter
        .decode_client_request(wire.to_string().as_bytes())
        .unwrap();
    adapter
        .encode_request(&explicit_false, "public", &TargetCaps::full())
        .expect("ordinary non-strict schema");
    wire["tools"][0]["strict"] = json!(true);
    let explicit_true = adapter
        .decode_client_request(wire.to_string().as_bytes())
        .unwrap();
    assert!(
        adapter
            .encode_request(&explicit_true, "public", &TargetCaps::full())
            .is_err(),
        "explicit guarantee must not weaken"
    );
    wire["tools"][0].as_object_mut().unwrap().remove("strict");
    let omitted = adapter
        .decode_client_request(wire.to_string().as_bytes())
        .unwrap();
    let output = adapter
        .encode_request(&omitted, "public", &TargetCaps::full())
        .expect("official normalization-with-fallback policy");
    assert_eq!(
        output["tools"][0]["parameters"],
        wire["tools"][0]["parameters"]
    );
    assert!(output["tools"][0].get("strict").is_none());
}

#[test]
#[ignore = "known standards gap: public Messages API 2026-10-21 removes sampling fields"]
fn messages_rejects_removed_sampling_even_at_defaults() {
    let baseline = messages();
    decode_client_request(baseline.to_string().as_bytes()).unwrap();
    for (field, value) in [
        ("temperature", json!(1)),
        ("top_p", json!(1)),
        ("top_k", json!(1)),
    ] {
        let mut wire = baseline.clone();
        wire[field] = value;
        assert!(
            decode_client_request(wire.to_string().as_bytes()).is_err(),
            "removed public field {field}"
        );
    }
}

#[test]
#[ignore = "known standards gap: direct-only allowed_callers is rejected as hosted execution"]
fn messages_accepts_direct_only_function_tools() {
    let mut wire = messages();
    wire["tools"] = json!([{"name":"read","input_schema":{"type":"object","properties":{}}}]);
    decode_client_request(wire.to_string().as_bytes()).unwrap();
    wire["tools"][0]["allowed_callers"] = json!(["direct"]);
    decode_client_request(wire.to_string().as_bytes())
        .expect("direct is not hosted tool execution");
}
