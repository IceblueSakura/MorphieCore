//! Provider-scoped extension carriers: Codex header lifecycle and custom body sections.
use morphiecore::{
    lowering::generation::{GenerationRepresentationContract as Contract, lower_request},
    protocol::{
        extensions::{CodexHeaders, CustomSections, RoutingHint},
        openai::{Profile, envelope},
    },
};
use serde_json::{Map, Value, json};

fn headers(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect()
}
#[test]
fn codex_header_lifecycle_fields_round_trip_with_distinct_owners() {
    let wire = headers(&[
        ("session-id", "cache-or-session"),
        ("thread-id", "thread-1"),
        ("x-client-request-id", "thread-1"),
        ("x-codex-window-id", "window-1"),
        ("x-codex-turn-state", "opaque-token"),
        ("x-codex-routing-hint", "model=gpt-6-astra;tier=priority"),
        ("x-codex-parent-thread-id", "thread-0"),
        ("x-openai-subagent", "review"),
        ("x-codex-turn-metadata", "{\"turn_id\":\"t1\"}"),
    ]);
    let decoded = CodexHeaders::decode(&wire).unwrap();
    assert_eq!(decoded.session.value().unwrap(), "cache-or-session");
    assert_eq!(decoded.thread.value().unwrap(), "thread-1");
    assert_eq!(decoded.window.value().unwrap(), "window-1");
    assert_eq!(decoded.turn_state.value().unwrap(), "opaque-token");
    assert_eq!(
        decoded.routing.value().unwrap(),
        &RoutingHint {
            model: "gpt-6-astra".into(),
            tier: Some("priority".into())
        }
    );
    assert_eq!(decoded.parent_thread.value().unwrap(), "thread-0");
    assert_eq!(decoded.subagent.value().unwrap(), "review");
    assert_eq!(
        decoded.turn_metadata.value().unwrap(),
        "{\"turn_id\":\"t1\"}"
    );
    let encoded = decoded.encode();
    for (name, value) in &wire {
        assert!(
            encoded.iter().any(|(k, v)| k == name && v == value),
            "{name}"
        );
    }
    // Conflicting thread identities fail instead of guessing a winner.
    let mut bad = wire.clone();
    bad[2].1 = "other".into();
    assert!(CodexHeaders::decode(&bad).is_err());
    // Duplicated typed names are not silently merged.
    let mut bad = wire.clone();
    bad.push(("session-id".to_string(), "second".to_string()));
    assert!(CodexHeaders::decode(&bad).is_err());
}
#[test]
fn turn_state_is_opaque_and_never_derived() {
    let empty = CodexHeaders::default();
    assert!(
        !empty
            .encode()
            .iter()
            .any(|(name, _)| name == "x-codex-turn-state")
    );
    let decoded =
        CodexHeaders::decode(&headers(&[("x-codex-turn-state", "server-issued")])).unwrap();
    assert_eq!(
        decoded.encode(),
        headers(&[("x-codex-turn-state", "server-issued")])
    );
    let mut cleared = decoded.clone();
    cleared.turn_state = Default::default();
    assert!(
        !cleared
            .encode()
            .iter()
            .any(|(name, _)| name == "x-codex-turn-state")
    );
}
#[test]
fn routing_hint_syntax_is_typed_and_rebuilt_verbatim() {
    for (value, model, tier) in [
        ("model=m", "m", None),
        (
            "model=gpt-6-astra;tier=priority",
            "gpt-6-astra",
            Some("priority"),
        ),
    ] {
        let decoded = CodexHeaders::decode(&headers(&[("x-codex-routing-hint", value)])).unwrap();
        assert_eq!(
            decoded.routing.value(),
            Some(&RoutingHint {
                model: model.into(),
                tier: tier.map(Into::into)
            })
        );
        assert_eq!(
            decoded.encode(),
            headers(&[("x-codex-routing-hint", value)])
        );
    }
    for bad in [
        "",
        "tier=t",
        "model=",
        "model=m;extra=1",
        "model=m;tier=t;x=y",
        "model=m;tier=",
        "nonsense",
    ] {
        assert!(
            CodexHeaders::decode(&headers(&[("x-codex-routing-hint", bad)])).is_err(),
            "{bad}"
        );
    }
}
#[test]
fn credential_and_transport_headers_are_never_context() {
    for name in [
        "authorization",
        "Authorization",
        "proxy-authorization",
        "cookie",
        "set-cookie",
        "x-api-key",
        "chatgpt-account-id",
        "X-OpenAI-Fedramp",
        "x-oai-attestation",
        "accept",
        "content-type",
        "content-encoding",
        "content-length",
        "host",
        "connection",
        "transfer-encoding",
    ] {
        assert!(
            CodexHeaders::decode(&headers(&[(name, "value")])).is_err(),
            "{name}"
        );
    }
}
#[test]
fn unmodeled_safe_headers_round_trip_opaque_within_budget() {
    let wire = headers(&[
        ("originator", "codex_cli_rs"),
        ("x-codex-beta-features", "a,b"),
        ("x-request-id", "req-1"),
    ]);
    let decoded = CodexHeaders::decode(&wire).unwrap();
    let encoded = decoded.encode();
    for (name, value) in &wire {
        assert!(
            encoded.iter().any(|(k, v)| k == name && v == value),
            "{name}"
        );
    }
    let mut many: Vec<(String, String)> = (0..40)
        .map(|i| (format!("x-custom-{i}"), "v".into()))
        .collect();
    assert!(CodexHeaders::decode(&many).is_err());
    many.truncate(4);
    assert!(CodexHeaders::decode(&many).is_ok());
    assert!(CodexHeaders::decode(&headers(&[("x-custom", "a\r\nInjected: 1")])).is_err());
    assert!(CodexHeaders::decode(&headers(&[("x custom", "v")])).is_err());
    assert!(CodexHeaders::decode(&headers(&[("", "v")])).is_err());
    assert!(CodexHeaders::decode(&headers(&[("x-custom", "x\0z")])).is_err());
    assert!(
        CodexHeaders::decode(&headers(&[("x-custom", "x"), ("x-custom", "y")])).is_err(),
        "duplicates"
    );
    let long = "x".repeat(5000);
    assert!(CodexHeaders::decode(&headers(&[("x-custom", &long)])).is_err());
    assert!(CodexHeaders::decode(&headers(&[("x-codex-turn-state", &long)])).is_err());
}
#[test]
fn custom_body_sections_round_trip_opaque_and_stay_fail_closed() {
    let mut body = Map::new();
    body.insert(
        "client_metadata".into(),
        json!({"session_id":"s1","x-codex-turn-metadata":{"turn_id":"t1"}}),
    );
    let decoded = CustomSections::decode(&body).unwrap();
    let mut out = Map::new();
    decoded.write(&mut out).unwrap();
    assert_eq!(Value::Object(out), Value::Object(body.clone()));
    let mut bad = Map::new();
    bad.insert("x-mystery".into(), json!({}));
    assert!(CustomSections::decode(&bad).is_err());
    let mut bad = Map::new();
    bad.insert("client_metadata".into(), json!("not-an-object"));
    assert!(CustomSections::decode(&bad).is_err());
    let mut big = Map::new();
    big.insert("client_metadata".into(), json!({"k": "x".repeat(70000)}));
    assert!(CustomSections::decode(&big).is_err());
    let mut many = Map::new();
    let mut entries = Map::new();
    for i in 0..40 {
        entries.insert(format!("k{i}"), json!("v"));
    }
    many.insert("client_metadata".into(), Value::Object(entries));
    assert!(CustomSections::decode(&many).is_err());
}
#[test]
fn request_envelope_carries_custom_sections_without_opening_unknown_keys() {
    let mut request = json!({"model":"m","input":"hello","client_metadata":{"session_id":"s1"}});
    let d = envelope::decode_request(&request).unwrap();
    assert_eq!(
        d.context.extensions.sections["client_metadata"],
        json!({"session_id":"s1"})
    );
    let target = lower_request(
        &d.task.semantic,
        &d.task.fidelity,
        Profile::Responses,
        Contract::full(),
    )
    .unwrap();
    let out = envelope::encode_request(&target, &d.context).unwrap();
    assert_eq!(out["client_metadata"], json!({"session_id":"s1"}));
    request["x-mystery"] = json!({});
    assert!(envelope::decode_request(&request).is_err());
}
