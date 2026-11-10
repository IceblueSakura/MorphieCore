use super::*;
use crate::{
    execution::attempt::DeliveryReason,
    protocol::failure::{RecognitionBasis, SourceFailureEvidence},
};
use opentelemetry_sdk::logs::SdkLogRecord;

fn attribute(record: &SdkLogRecord, name: &str) -> Option<AnyValue> {
    record
        .attributes_iter()
        .find(|(key, _)| key.as_str() == name)
        .map(|(_, value)| value.clone())
}
#[test]
fn recovered_attempt_is_warn_without_a_success_archive_or_duplicate_failure() {
    let (sink, exporter) = Sink::memory();
    let mut trace = Trace::new(Some(&sink));
    trace.begin_candidate(1);
    trace.head(503);
    trace.executor("synthetic");
    trace.named_account(Some("primary"));
    trace.end_candidate(Some("gateway.upstream"), true);
    trace.begin_candidate(2);
    trace.head(200);
    trace.end_candidate(None, false);
    trace.outcome(Outcome::Complete);
    drop(trace);
    let logs = exporter.get_emitted_logs().unwrap();
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0].record.severity_number(), Some(Severity::Warn));
    assert_eq!(
        attribute(&logs[0].record, "retry.planned"),
        Some(true.into())
    );
    assert_eq!(attribute(&logs[0].record, "attempt"), Some(1i64.into()));
    assert_eq!(
        attribute(&logs[0].record, "account"),
        Some("primary".into())
    );
}
#[test]
fn failed_task_inside_http_success_preserves_source_through_consumer_cancellation() {
    let (sink, exporter) = Sink::memory();
    let mut trace = Trace::new(Some(&sink));
    trace.begin_candidate(1);
    trace.head(200);
    trace.report.source_failure(SourceFailureEvidence::envelope(
        429,
        RecognitionBasis::EnvelopeType,
        None,
        Some("rate_limit_error"),
        Some("synthetic-private-message"),
    ));
    trace.report.failure(
        None,
        &AttemptError::Delivery(DeliveryReason::BodyConsumerClosed),
    );
    trace.stage(Stage::Delivery);
    trace.end_candidate(None, false);
    trace.outcome(Outcome::Complete);
    drop(trace);
    let logs = exporter.get_emitted_logs().unwrap();
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0].record.severity_number(), Some(Severity::Warn));
    assert_eq!(
        attribute(&logs[0].record, "error.type"),
        Some("source.rate_limit".into())
    );
    assert_eq!(attribute(&logs[0].record, "stage"), Some("intake".into()));
    assert_eq!(
        attribute(&logs[0].record, "http.response.status_code"),
        Some(200i64.into())
    );
    assert!(!format!("{:?}", logs[0].record.body()).contains("synthetic-private-message"));
}
#[test]
fn consumer_close_alone_is_not_a_system_failure() {
    let (sink, exporter) = Sink::memory();
    let mut trace = Trace::new(Some(&sink));
    trace.begin_candidate(1);
    trace.report.failure(
        None,
        &AttemptError::Delivery(DeliveryReason::BodyConsumerClosed),
    );
    trace.outcome(Outcome::Error);
    drop(trace);
    assert!(exporter.get_emitted_logs().unwrap().is_empty());
}
#[tokio::test]
async fn official_otlp_exporter_delivers_failure_logs_to_a_loopback_receiver() {
    use axum::{Json, Router, routing::post};
    use std::time::Duration;
    let (sent, mut received) = tokio::sync::mpsc::channel(1);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}/v1/logs", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new().route(
                "/v1/logs",
                post(move |Json(body): Json<serde_json::Value>| {
                    let sent = sent.clone();
                    async move {
                        sent.try_send(body).unwrap();
                        Json(serde_json::json!({}))
                    }
                }),
            ),
        )
        .await
        .unwrap();
    });
    let sink = Sink::open(&DiagnosticsConfig {
        endpoint,
        private_bodies: false,
        body_bytes: 1024,
    })
    .unwrap();
    let mut trace = Trace::new(Some(&sink));
    trace.begin_candidate(1);
    trace.head(503);
    trace.end_candidate(Some("gateway.upstream"), false);
    drop(trace);
    sink.flush().await;
    let body = tokio::time::timeout(Duration::from_secs(3), received.recv())
        .await
        .unwrap()
        .unwrap();
    let log = &body["resourceLogs"][0]["scopeLogs"][0]["logRecords"][0];
    assert_eq!(log["severityText"], "ERROR");
    assert_eq!(log["eventName"], "gateway.failure");
    assert!(log.get("traceId").is_none_or(|id| id == ""));
    server.abort();
}

#[tokio::test(flavor = "current_thread")]
async fn last_logger_owner_does_not_wait_for_a_stalled_export_on_drop() {
    use axum::{Router, routing::post};
    use std::{
        sync::Arc,
        time::{Duration, Instant},
    };
    let started = Arc::new(tokio::sync::Notify::new());
    let notification = started.clone();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}/v1/logs", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new().route(
                "/v1/logs",
                post(move || {
                    let notification = notification.clone();
                    async move {
                        notification.notify_one();
                        std::future::pending::<axum::http::StatusCode>().await
                    }
                }),
            ),
        )
        .await
        .unwrap();
    });
    let sink = Sink::open(&DiagnosticsConfig {
        endpoint,
        private_bodies: false,
        body_bytes: 1024,
    })
    .unwrap();
    for _ in 0..4 {
        sink.emit(
            "gateway.failure",
            Severity::Error,
            vec![],
            AnyValue::from("synthetic"),
        );
    }
    tokio::time::timeout(Duration::from_secs(3), started.notified())
        .await
        .unwrap();
    let before = Instant::now();
    drop(sink);
    assert!(
        before.elapsed() < Duration::from_secs(1),
        "last owner must not wait for the two-second HTTP timeout"
    );
    server.abort();
}
