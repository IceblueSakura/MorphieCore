//! Neutral ingress context, independent of Provider selection and real I/O.
use super::*;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use tower::ServiceExt;

#[tokio::test]
async fn neutral_conversation_header_is_authenticated_and_checked_before_body_poll() {
    let gate = tests::gateway(Limits::default());
    for authenticated in [false, true] {
        let forbidden = futures_util::stream::poll_fn(
            |_| -> std::task::Poll<Option<Result<bytes::Bytes, std::io::Error>>> {
                panic!("invalid conversation header must not poll the body")
            },
        );
        let mut request = Request::builder()
            .method("POST")
            .uri("/v1/chat/completions")
            .header("content-type", "application/json")
            .header("x-morphiecore-conversation-id", "same")
            .header("x-morphiecore-conversation-id", "same");
        if authenticated {
            request = request.header("authorization", format!("Bearer {}", tests::KEY));
        }
        let response = gate
            .router()
            .oneshot(request.body(Body::from_stream(forbidden)).unwrap())
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            if authenticated {
                StatusCode::BAD_REQUEST
            } else {
                StatusCode::UNAUTHORIZED
            }
        );
    }
}

#[test]
fn public_chat_no_longer_accepts_provider_session_body() {
    let gate = tests::gateway(Limits::default());
    for session in [
        serde_json::Value::Null,
        serde_json::json!("synthetic-provider-session"),
    ] {
        let body = serde_json::json!({"model":"fixture-model","messages":[{"role":"user","content":"hello"}],"session_id":session});
        assert_eq!(
            admission::prepare(&gate.state, Profile::Chat, body.to_string().as_bytes())
                .err()
                .unwrap()
                .status,
            400
        );
    }
}

#[test]
fn neutral_context_has_explicit_or_request_local_scope_and_never_reads_vendor_headers() {
    use axum::http::{HeaderMap, HeaderValue};
    let mut headers = HeaderMap::new();
    headers.insert(
        "x-opencode-session",
        HeaderValue::from_static("ignored-provider-session"),
    );
    headers.insert("x-session-id", HeaderValue::from_static("ignored-alias"));
    let one = admission::conversation(&headers).unwrap();
    let two = admission::conversation(&headers).unwrap();
    assert!(!one.is_conversation_scoped());
    assert_ne!(one.id(), two.id());
    assert_eq!(one.id().len(), 32);
    headers.insert(
        "x-morphiecore-conversation-id",
        HeaderValue::from_static("conversation-one"),
    );
    let group = admission::conversation(&headers).unwrap();
    assert!(group.is_conversation_scoped());
    assert_eq!(group.id(), "conversation-one");
    assert_eq!(group, admission::conversation(&headers).unwrap());
    headers.insert(
        "x-morphiecore-conversation-id",
        HeaderValue::from_str(&"x".repeat(256)).unwrap(),
    );
    admission::conversation(&headers).unwrap();
    for invalid in ["", " ", "x,y", "中文", &"x".repeat(257)] {
        headers.insert(
            "x-morphiecore-conversation-id",
            HeaderValue::from_bytes(invalid.as_bytes()).unwrap(),
        );
        assert_eq!(admission::conversation(&headers).unwrap_err().status, 400);
    }
}
