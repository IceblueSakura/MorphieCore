//! Offline HTTP transport checks with two literal-loopback listeners.
use super::*;
use axum::{Router, body::Body, http::StatusCode, response::Response, routing::post};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use tokio::net::TcpListener;
struct Guard(tokio::task::AbortHandle);
impl Drop for Guard {
    fn drop(&mut self) {
        self.0.abort();
    }
}
#[tokio::test]
async fn redirects_are_not_followed_server_errors_are_not_retried_and_timeouts_are_classified() {
    let trap = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let destination = format!("http://{}/leak", trap.local_addr().unwrap());
    let trap_calls = Arc::new(AtomicUsize::new(0));
    let observed = trap_calls.clone();
    let trap_task = tokio::spawn(async move {
        axum::serve(
            trap,
            Router::new().route(
                "/leak",
                post(move || {
                    let observed = observed.clone();
                    async move {
                        observed.fetch_add(1, Ordering::SeqCst);
                        StatusCode::OK
                    }
                }),
            ),
        )
        .await
        .unwrap();
    });
    let _trap = Guard(trap_task.abort_handle());
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = calls.clone();
    let app = Router::new()
        .route(
            "/redirect",
            post(move || {
                let destination = destination.clone();
                async move {
                    Response::builder()
                        .status(307)
                        .header("location", destination)
                        .body(Body::empty())
                        .unwrap()
                }
            }),
        )
        .route(
            "/error",
            post(move || {
                let observed = observed.clone();
                async move {
                    observed.fetch_add(1, Ordering::SeqCst);
                    StatusCode::SERVICE_UNAVAILABLE
                }
            }),
        )
        .route(
            "/stall",
            post(|| async { std::future::pending::<StatusCode>().await }),
        );
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let _server = Guard(server.abort_handle());
    let transport = HttpTransport::new(None).unwrap();
    let request = |path: &str| UpstreamRequest {
        origin: origin.clone(),
        method: "POST",
        path: path.into(),
        safe_headers: vec![("content-type".into(), "application/json".into())],
        auth_header: (
            "authorization".into(),
            "Bearer synthetic-upstream-credential".into(),
        ),
        body: b"{}".to_vec(),
    };
    let redirected = transport
        .send(request("/redirect"), Duration::from_secs(2))
        .await
        .unwrap();
    assert_eq!(redirected.status(), 307);
    assert_eq!(trap_calls.load(Ordering::SeqCst), 0);
    let error = transport
        .send(request("/error"), Duration::from_secs(2))
        .await
        .unwrap();
    assert_eq!(error.status(), 503);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        transport
            .send(request("/stall"), Duration::from_millis(30))
            .await
            .unwrap_err(),
        ErrorClass::Timeout
    );
}
