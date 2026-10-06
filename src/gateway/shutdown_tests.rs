//! Cancellation is tested before dispatch, with no upstream or arbitrary sleeps.
use super::{Limits, http::handle, tests::gateway};
use crate::protocol::openai::Profile;
use axum::{
    body::{Body, Bytes},
    http::Request,
};
use std::time::Duration;
#[tokio::test]
async fn shutdown_during_intake_releases_capacity_and_stays_stopped() {
    let gate = gateway(Limits::default());
    let capacity = gate.state.permits.available_permits();
    let (polled, ready) = tokio::sync::oneshot::channel();
    let stream = futures_util::stream::once(async move {
        polled.send(()).unwrap();
        std::future::pending::<Result<Bytes, std::io::Error>>().await
    });
    let request = Request::builder()
        .header("content-type", "application/json")
        .body(Body::from_stream(stream))
        .unwrap();
    let operation = tokio::spawn(handle(gate.state.clone(), Profile::Chat, request));
    tokio::time::timeout(Duration::from_secs(1), ready)
        .await
        .unwrap()
        .unwrap();
    gate.shutdown();
    gate.shutdown();
    let error = tokio::time::timeout(Duration::from_secs(1), operation)
        .await
        .unwrap()
        .unwrap()
        .unwrap_err();
    assert_eq!(error.code, "shutting_down");
    assert_eq!(gate.state.permits.available_permits(), capacity);
    let request = Request::builder().body(Body::empty()).unwrap();
    assert_eq!(
        handle(gate.state.clone(), Profile::Chat, request)
            .await
            .unwrap_err()
            .code,
        "shutting_down"
    );
}
