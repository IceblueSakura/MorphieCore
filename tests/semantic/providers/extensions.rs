//! Provider-scoped extension carriers: Codex header lifecycle.
use morphiecore::protocol::extensions::{CodexHeaders, RoutingHint};

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
