//! Actual blocking OTLP/HTTP export, using only a bounded loopback test receiver.
use super::{app_filter, isolated_child, resource};
use axum::{Router, body::Bytes, extract::State, http::HeaderMap, routing::post};
use opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge;
use opentelemetry_otlp::{Protocol, RetryPolicy, WithExportConfig, WithHttpConfig};
use opentelemetry_sdk::logs::{BatchConfigBuilder, BatchLogProcessor, SdkLoggerProvider};
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tracing_subscriber::{Layer as _, layer::SubscriberExt};

#[derive(Clone, Default)]
struct Receiver {
    calls: Arc<AtomicUsize>,
    status: Arc<AtomicUsize>,
    bodies: Arc<Mutex<Vec<Bytes>>>,
}
async fn receive(
    State(state): State<Receiver>,
    headers: HeaderMap,
    body: Bytes,
) -> (axum::http::StatusCode, &'static str) {
    assert_eq!(headers["content-type"], "application/x-protobuf");
    assert_eq!(headers["x-explicit"], "synthetic");
    // A source-verified SDK limitation: builder headers do not disable env merging.
    assert_eq!(headers["x-environment-only"], "synthetic");
    assert!(!headers.contains_key("authorization"));
    assert!(!body.is_empty() && body.len() <= 16 << 10);
    state.bodies.lock().unwrap().push(body);
    let index = state.calls.fetch_add(1, Ordering::SeqCst);
    let status = match state.status.load(Ordering::SeqCst) {
        0 => {
            if index == 0 {
                503
            } else {
                200
            }
        }
        1 => {
            tokio::time::sleep(Duration::from_secs(1)).await;
            200
        }
        status => status as u16,
    };
    (axum::http::StatusCode::from_u16(status).unwrap(), "")
}

async fn provider(endpoint: String, max_bytes: usize, retries: usize) -> SdkLoggerProvider {
    tokio::task::spawn_blocking(move || {
        // Explicit client avoids feature-dependent async client selection and proxies.
        let client = reqwest::blocking::Client::builder()
            .no_proxy()
            .timeout(Duration::from_millis(300))
            .build()
            .unwrap();
        let exporter = opentelemetry_otlp::LogExporter::builder()
            .with_http()
            .with_http_client(client)
            .with_endpoint(endpoint)
            .with_protocol(Protocol::HttpBinary)
            .with_timeout(Duration::from_millis(300))
            .with_headers([("x-explicit".to_owned(), "synthetic".to_owned())].into())
            .with_max_request_body_size(max_bytes)
            .with_retry_policy(
                RetryPolicy::disabled()
                    .with_max_retries(retries)
                    .with_initial_delay(Duration::from_millis(1))
                    .with_max_delay(Duration::from_millis(1))
                    .with_max_jitter(Duration::ZERO),
            )
            .build()
            .unwrap();
        SdkLoggerProvider::builder()
            .with_resource(resource())
            .with_log_processor(
                BatchLogProcessor::builder(exporter)
                    .with_batch_config(
                        BatchConfigBuilder::default()
                            .with_max_queue_size(8)
                            .with_max_export_batch_size(8)
                            .with_scheduled_delay(Duration::from_secs(60))
                            .build(),
                    )
                    .build(),
            )
            .build()
    })
    .await
    .unwrap()
}
fn emit(provider: &SdkLoggerProvider) {
    let subscriber = tracing_subscriber::registry()
        .with(OpenTelemetryTracingBridge::new(provider).with_filter(app_filter()));
    tracing::subscriber::with_default(subscriber, || {
        tracing::info!(name: "generation.request.finished", target: "morphiecore.telemetry", outcome = "succeeded");
        tracing::error!(target: "unapproved.library", "synthetic-private-exporter-input");
    });
}
async fn flush(provider: &SdkLoggerProvider) -> opentelemetry_sdk::error::OTelSdkResult {
    let provider = provider.clone();
    tokio::time::timeout(
        Duration::from_secs(3),
        tokio::task::spawn_blocking(move || provider.force_flush()),
    )
    .await
    .unwrap()
    .unwrap()
}
async fn shutdown(provider: SdkLoggerProvider) {
    tokio::task::spawn_blocking(move || provider.shutdown_with_timeout(Duration::from_secs(1)))
        .await
        .unwrap()
        .unwrap();
}

#[tokio::test]
#[ignore = "environment-cleared ecosystem_checks child"]
async fn protobuf_export_checks_endpoint_headers_retry_timeout_and_size_limits() {
    isolated_child();
    let state = Receiver::default();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}/v1/logs", listener.local_addr().unwrap());
    let app = Router::new()
        .route("/v1/logs", post(receive))
        .with_state(state.clone());
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let result = async {
        let logs = provider(endpoint.clone(), 16 << 10, 1).await;
        emit(&logs);
        flush(&logs).await.unwrap();
        assert_eq!(
            state.calls.load(Ordering::SeqCst),
            2,
            "one initial export and one retry"
        );
        {
            let bodies = state.bodies.lock().unwrap();
            assert_eq!(bodies[0], bodies[1], "retry must export the same telemetry");
            assert!(
                !bodies[0]
                    .windows(b"synthetic-private".len())
                    .any(|w| w == b"synthetic-private")
            );
        }
        shutdown(logs).await;

        state.status.store(400, Ordering::SeqCst);
        let logs = provider(endpoint.clone(), 16 << 10, 1).await;
        emit(&logs);
        assert!(flush(&logs).await.is_err());
        assert_eq!(
            state.calls.load(Ordering::SeqCst),
            3,
            "400 is not retryable"
        );
        // A later empty flush is not proof that the earlier failed batch was delivered.
        flush(&logs).await.unwrap();
        assert_eq!(state.calls.load(Ordering::SeqCst), 3);
        shutdown(logs).await;

        state.status.store(1, Ordering::SeqCst);
        let logs = provider(endpoint.clone(), 16 << 10, 0).await;
        emit(&logs);
        assert!(flush(&logs).await.is_err(), "HTTP exporter must time out");
        assert_eq!(state.calls.load(Ordering::SeqCst), 4);
        shutdown(logs).await;

        let logs = provider(endpoint, 8, 0).await;
        emit(&logs);
        assert!(flush(&logs).await.is_err());
        assert_eq!(
            state.calls.load(Ordering::SeqCst),
            4,
            "oversize protobuf rejected before HTTP"
        );
        shutdown(logs).await;
    };
    // The outer owned-child deadline also bounds failures in SDK worker shutdown.
    let finished = tokio::time::timeout(Duration::from_secs(10), result).await;
    server.abort();
    assert!(server.await.unwrap_err().is_cancelled());
    finished.unwrap();
}
