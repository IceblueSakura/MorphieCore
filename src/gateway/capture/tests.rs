//! Private capture security, incomplete-source status and retention bounds.
use super::{Boundary, CaptureConfig, Sink as CaptureSink};
use axum::http::HeaderMap;

#[tokio::test]
async fn credentials_are_redacted_before_bounded_capture_and_incomplete_sources_stay_incomplete() {
    let directory = crate::test_files::private_directory();
    let capture_path = directory.path().join("capture.jsonl");
    let config: CaptureConfig = serde_json::from_value(serde_json::json!({
        "path":capture_path,"client_request":{"headers":true,"body":true},
        "upstream_response":{"body":true},"max_unit_bytes":2048,
        "max_exchange_bytes":8192,"max_total_bytes":16384
    }))
    .unwrap();
    assert!(
        serde_json::from_value::<CaptureConfig>(serde_json::json!({
            "path":capture_path,"redact":false
        }))
        .is_err()
    );
    let sink = CaptureSink::open(config.clone()).unwrap();
    assert!(CaptureSink::open(config).is_err());
    let capture = sink.exchange();
    let mut headers = HeaderMap::new();
    headers.insert(
        "authorization",
        "Bearer synthetic-auth-secret".parse().unwrap(),
    );
    headers.insert("x-business", "synthetic-business-header".parse().unwrap());
    headers.insert(
        "location",
        "/continue?access_token=synthetic-location-secret"
            .parse()
            .unwrap(),
    );
    headers.insert(
        "link",
        "<https://example.invalid/?sig=synthetic-link-secret>; rel=next"
            .parse()
            .unwrap(),
    );
    headers.insert("x-private-key", "synthetic-private-key".parse().unwrap());
    headers.insert(
        "www-authenticate",
        "Negotiate synthetic-challenge-secret".parse().unwrap(),
    );
    capture.head(
        Boundary::ClientRequest,
        None,
        &headers,
        Some("%61pi_key=synthetic-query-secret&sig=synthetic-signature-secret&tag=business"),
    );
    assert!(capture.json(Boundary::ClientRequest, None,
            br#"{"input":"synthetic-business-body","private_key":"synthetic-body-key","auth":"synthetic-auth-body","redirect":"/next?api_key=synthetic-relative-secret","url":"https://example.invalid/#access_token=synthetic-fragment-token","nested":{"api\u005fkey":"synthetic-body-secret"},"arguments":"{\"password\":\"synthetic-argument-secret\",\"path\":\"ok\"}"}"#));
    assert!(capture.json(
        Boundary::ClientRequest,
        None,
        br#"{"literal":"{ \"sample\": 1 }"}"#
    ));
    let mut stream = capture
        .body(Boundary::UpstreamResponse, Some(1), "text/event-stream")
        .unwrap();
    for byte in b"data: {\"value\":{\"access_token\":\"synthetic-stream-secret\",\"text\":\"business-stream\"}}\n\n" {
            stream.feed(std::slice::from_ref(byte));
        }
    stream.finish();
    let mut unsafe_stream = capture
        .body(Boundary::UpstreamResponse, Some(2), "text/event-stream")
        .unwrap();
    unsafe_stream.feed(b"data: {\"type\":\"response.function_call_arguments.delta\",\"delta\":\"synthetic-fragment-secret\"}\n\n");
    unsafe_stream.finish();
    let mut legacy = capture
        .body(Boundary::UpstreamResponse, Some(3), "text/event-stream")
        .unwrap();
    legacy.feed(b"data: {\"choices\":[{\"delta\":{\"function_call\":{\"arguments\":\"{\\\"password\\\":\\\"\"}}}]}\n\ndata: {\"choices\":[{\"delta\":{\"function_call\":{\"arguments\":\"synthetic-legacy-secret\\\"}\"}}}]}\n\n");
    legacy.finish();
    use futures_util::StreamExt;
    let broken = axum::body::Body::from_stream(futures_util::stream::iter([
        Ok::<_, std::io::Error>(bytes::Bytes::from_static(
            b"data: {\"text\":\"read-error-business\"}\n\n",
        )),
        Err(std::io::Error::other("synthetic read failure")),
    ]));
    let observer = capture
        .body(Boundary::UpstreamResponse, Some(4), "text/event-stream")
        .unwrap();
    let mut observed = crate::gateway::capture::observe(broken, observer).into_data_stream();
    assert!(observed.next().await.unwrap().is_ok());
    assert!(observed.next().await.unwrap().is_err());
    assert!(observed.next().await.is_none());
    drop(observed);
    assert!(
        capture
            .body(Boundary::ClientResponse, None, "application/json")
            .is_none()
    );
    let mut nested_headers = HeaderMap::new();
    nested_headers.insert("location", "https://example.invalid/redirect?next=https%3A%2F%2Fexample.invalid%2F%3Fapi_key%3Dsynthetic-nested-secret".parse().unwrap());
    capture.head(Boundary::ClientRequest, None, &nested_headers, None);
    let double = serde_json::to_string(
        &serde_json::to_string(&serde_json::json!({"api_key":"synthetic-double-secret"})).unwrap(),
    )
    .unwrap();
    let named_headers = serde_json::to_string(&serde_json::json!({"headers":[{"name":"Authorization","value":"Bearer synthetic-header-secret"}]})).unwrap();
    assert!(
        capture.json(
            Boundary::ClientRequest,
            None,
            serde_json::json!({"double":double,"output":named_headers})
                .to_string()
                .as_bytes()
        )
    );
    sink.flush().await;
    let captured = std::fs::read_to_string(&capture_path).unwrap();
    assert!(
        captured
            .lines()
            .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
            .any(|value| value["value"]["literal"] == "{ \"sample\": 1 }")
    );
    for secret in [
        "synthetic-auth-secret",
        "synthetic-query-secret",
        "synthetic-body-secret",
        "synthetic-argument-secret",
        "synthetic-stream-secret",
        "synthetic-fragment-secret",
        "synthetic-location-secret",
        "synthetic-link-secret",
        "synthetic-private-key",
        "synthetic-signature-secret",
        "synthetic-body-key",
        "synthetic-legacy-secret",
        "synthetic-auth-body",
        "synthetic-relative-secret",
        "synthetic-fragment-token",
        "synthetic-challenge-secret",
        "synthetic-nested-secret",
        "synthetic-double-secret",
        "synthetic-header-secret",
    ] {
        assert!(!captured.contains(secret));
    }
    assert!(captured.contains("synthetic-business-body"));
    assert!(captured.contains("synthetic-business-header"));
    assert!(captured.contains("business-stream"));
    assert!(captured.contains("unsafe_fragment"));
    assert!(captured.contains("incomplete"));
    let errored: Vec<serde_json::Value> = captured
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .filter(|value: &serde_json::Value| value["attempt"] == 4)
        .collect();
    assert!(errored.iter().any(|value| value["status"] == "incomplete"));
    assert!(!errored.iter().any(|value| value["status"] == "source_eof"));
    for _ in 0..100 {
        capture.json(Boundary::ClientRequest, None, br#"{"input":"business"}"#);
    }
    sink.flush().await;
    assert!(sink.status().retention_dropped > 0);
    assert!(std::fs::metadata(&capture_path).unwrap().len() <= 16384);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&capture_path)
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
}
