use super::*;

fn exchange() -> Exchange {
    Sink::new(4096).exchange()
}
fn context(exchange: &Exchange) -> String {
    format!("{:?}", exchange.context())
}
#[test]
fn body_context_redacts_structured_credentials_and_nested_urls_before_export() {
    let exchange = exchange();
    exchange.begin(1);
    assert!(exchange.json(Boundary::UpstreamRequest, Some(1), br#"{"prompt":"keep business context","api_key":"synthetic-secret","arguments":"{\"access_token\":\"nested-secret\",\"answer\":42}","url":"https://user:password@example.com/file?token=query-secret&x=keep"}"#));
    let body = context(&exchange);
    for secret in [
        "synthetic-secret",
        "nested-secret",
        "query-secret",
        "user:password",
    ] {
        assert!(!body.contains(secret), "{secret}");
    }
    assert!(body.contains("keep business context"));
    assert!(body.contains("REDACTED"));
}
#[test]
fn split_json_is_sanitized_only_after_eof_and_incomplete_fragments_are_never_exported() {
    let exchange = exchange();
    let mut capture = exchange.body(Boundary::ClientRequest, None, "application/json");
    capture.feed(br#"{"access_token":"private"#);
    assert!(!context(&exchange).contains("private"));
    capture.feed(br#"-value","prompt":"keep"}"#);
    capture.finish();
    let body = context(&exchange);
    assert!(body.contains("keep"));
    assert!(!body.contains("private-value"));
    let other = self::exchange();
    let mut capture = other.body(Boundary::ClientRequest, None, "application/json");
    capture.feed(br#"{"access_token":"unclosed-secret"#);
    capture.finish();
    assert!(!context(&other).contains("unclosed-secret"));
    assert!(context(&other).contains("unsafe_body"));
}
#[test]
fn sse_retains_a_bounded_recent_tail_and_marks_unsafe_argument_fragments() {
    let exchange = exchange();
    exchange.begin(1);
    let mut capture = exchange.body(Boundary::UpstreamResponse, Some(1), "text/event-stream");
    for index in 0..10 {
        capture.feed(
            format!(
                "data: {{\"type\":\"response.output_text.delta\",\"delta\":\"event-{index}\"}}\n\n"
            )
            .as_bytes(),
        );
    }
    capture.feed(b"data: {\"type\":\"response.function_call_arguments.delta\",\"delta\":\"secret-fragment\"}\n\n");
    capture.finish();
    let body = context(&exchange);
    assert!(!body.contains("event-0"));
    assert!(body.contains("event-9"));
    assert!(!body.contains("secret-fragment"));
    let state = exchange.0.context.lock().unwrap();
    assert!(state.slots.iter().map(|s| s.retained).sum::<usize>() <= 4096);
    assert!(state.slots[2].incomplete);
    assert!(state.slots[2].truncated);
}
#[test]
fn a_new_attempt_releases_old_upstream_context_without_late_observer_pollution() {
    let exchange = exchange();
    exchange.json(Boundary::ClientRequest, None, br#"{"input":"client"}"#);
    exchange.begin(1);
    let mut old = exchange.body(Boundary::UpstreamResponse, Some(1), "application/json");
    exchange.json(
        Boundary::UpstreamRequest,
        Some(1),
        br#"{"input":"first-attempt"}"#,
    );
    exchange.begin(2);
    old.feed(br#"{"result":"late-first-response"}"#);
    old.finish();
    exchange.json(
        Boundary::UpstreamRequest,
        Some(2),
        br#"{"input":"second-attempt"}"#,
    );
    let body = context(&exchange);
    assert!(body.contains("client"));
    assert!(body.contains("second-attempt"));
    assert!(!body.contains("first-attempt"));
    assert!(!body.contains("late-first-response"));
}
