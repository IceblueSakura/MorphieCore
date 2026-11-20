use super::*;
use crate::{
    execution::Attempt,
    protocol::{
        openai::{Dialect, GenerationApi, GenerationCodec},
        stream::SseLimits,
    },
};
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
    let (sink, exporter, spans) = Sink::memory_signals(LogLevel::Warn);
    let mut trace = Trace::new(Some(&sink));
    trace.begin_candidate(1);
    trace.sent();
    trace.head(503);
    trace.executor("synthetic");
    trace.named_account(Some("primary"));
    trace.end_candidate(Some("gateway.upstream"), true);
    trace.begin_candidate(2);
    trace.sent();
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
    let spans = spans.get_finished_spans().unwrap();
    let attempts: Vec<_> = spans
        .iter()
        .filter(|span| span.name == "gateway.attempt")
        .collect();
    let operation = spans
        .iter()
        .find(|span| span.name == "gateway.operation")
        .unwrap();
    assert_eq!(attempts.len(), 2);
    assert!(matches!(
        attempts[0].status,
        opentelemetry::trace::Status::Error { .. }
    ));
    assert_eq!(attempts[1].status, opentelemetry::trace::Status::Unset);
    assert_eq!(operation.status, opentelemetry::trace::Status::Unset);
    assert_eq!(attempts[0].parent_span_id, operation.span_context.span_id());
    assert_eq!(attempts[1].parent_span_id, operation.span_context.span_id());
    let failed_context = logs[0].record.trace_context().unwrap();
    assert_eq!(failed_context.span_id, attempts[0].span_context.span_id());
    assert_eq!(failed_context.trace_id, operation.span_context.trace_id());
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
    let (sink, exporter, spans) = Sink::memory_signals(LogLevel::Warn);
    let mut trace = Trace::new(Some(&sink));
    trace.begin_candidate(1);
    trace.report.failure(
        None,
        &AttemptError::Delivery(DeliveryReason::BodyConsumerClosed),
    );
    trace.outcome(Outcome::Error);
    drop(trace);
    assert!(exporter.get_emitted_logs().unwrap().is_empty());
    let spans = spans.get_finished_spans().unwrap();
    assert_eq!(spans.len(), 2);
    for span in spans {
        assert_eq!(span.status, opentelemetry::trace::Status::Unset);
        assert!(
            span.attributes
                .iter()
                .any(|attribute| attribute.key.as_str() == "morphiecore.outcome"
                    && attribute.value.as_str() == "cancelled")
        );
    }
}
#[test]
fn span_milestones_are_bounded_and_separate_terminal_eof_and_handoff() {
    let (sink, exporter, spans) = Sink::memory_signals(LogLevel::Info);
    let mut trace = Trace::new(Some(&sink));
    trace.generation("public", crate::protocol::ClientProtocol::Responses);
    trace.begin_candidate(1);
    trace.sent();
    trace.head(200);
    for _ in 0..1000 {
        trace.stage(Stage::Intake);
        trace.stage(Stage::Projection);
        trace.stage(Stage::Delivery);
    }
    for _ in 0..1000 {
        trace.terminal_validated();
    }
    trace.finish_validated();
    for _ in 0..1000 {
        trace.publishing();
        trace.handoff(false);
    }
    trace.handoff(true);
    trace.outcome(Outcome::Complete);
    trace.end_candidate(None, false);
    drop(trace);
    let logs = exporter.get_emitted_logs().unwrap();
    assert_eq!(logs.len(), 1);
    let spans = spans.get_finished_spans().unwrap();
    let attempt = spans
        .iter()
        .find(|span| span.name == "gateway.attempt")
        .unwrap();
    let events: Vec<_> = attempt
        .events
        .iter()
        .map(|event| event.name.as_ref())
        .collect();
    assert_eq!(
        events,
        [
            "gateway.upstream.sent",
            "gateway.upstream.headers",
            "gateway.upstream.terminal",
            "gateway.upstream.eof",
            "gateway.client.published",
            "gateway.client.first_handoff",
            "gateway.client.final_handoff",
        ]
    );
    let operation = spans
        .iter()
        .find(|span| span.name == "gateway.operation")
        .unwrap();
    assert_eq!(attempt.parent_span_id, operation.span_context.span_id());
    let summary = &logs[0].record;
    assert_eq!(summary.event_name(), Some("gateway.operation.finished"));
    assert_eq!(
        summary.trace_context().unwrap().span_id,
        operation.span_context.span_id()
    );
    assert_eq!(summary.severity_number(), Some(Severity::Info));
    assert_eq!(
        attribute(summary, "morphiecore.outcome"),
        Some("complete".into())
    );
    assert_eq!(
        attribute(summary, "morphiecore.send_attempts"),
        Some(1i64.into())
    );
    assert_eq!(
        attribute(summary, "morphiecore.client.handoff_complete"),
        Some(true.into())
    );
}

#[test]
fn info_level_emits_cancelled_summary_without_promoting_cancellation_to_failure() {
    let (sink, exporter) = Sink::memory_at(LogLevel::Info);
    let mut trace = Trace::new(Some(&sink));
    trace.generation("public", crate::protocol::ClientProtocol::Responses);
    trace.stage(Stage::Intake);
    trace.report.failure(
        None,
        &AttemptError::Delivery(DeliveryReason::BodyConsumerClosed),
    );
    drop(trace);
    let logs = exporter.get_emitted_logs().unwrap();
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0].record.severity_text(), Some("INFO"));
    assert_eq!(
        attribute(&logs[0].record, "morphiecore.outcome"),
        Some("cancelled".into())
    );
}

#[test]
fn failed_terminal_snapshot_keeps_source_context_after_cancellation() {
    let mut events = morphiecore_test_support::responses_profile::events(2);
    let mut terminal = events.pop().unwrap();
    terminal["response"]["output"] = serde_json::json!([]);
    let mut attempt = Attempt::new(
        GenerationCodec::new(GenerationApi::Responses, Dialect::Standard, None),
        1 << 20,
        SseLimits::default(),
    );
    attempt.begin(200, "text/event-stream").unwrap();
    for event in events {
        attempt
            .push(format!("data: {event}\n\n").as_bytes())
            .unwrap();
    }
    let error = attempt
        .push(format!("data: {terminal}\n\n").as_bytes())
        .unwrap_err();
    let (sink, exporter) = Sink::memory();
    let mut trace = Trace::new(Some(&sink));
    trace.begin_candidate(1);
    trace.head(200);
    trace.decode_failure(&error, &attempt);
    let context = attempt.decode_failure();
    attempt.cancel();
    assert_eq!(attempt.decode_failure(), context);
    trace.report.failure(
        None,
        &AttemptError::Delivery(DeliveryReason::BodyConsumerClosed),
    );
    drop(trace);
    let logs = exporter.get_emitted_logs().unwrap();
    let record = &logs[0].record;
    assert_eq!(
        attribute(record, "error.type"),
        Some("codec.invalid.snapshot_shape".into())
    );
    assert_eq!(attribute(record, "decode.operation"), Some("push".into()));
    assert_eq!(
        attribute(record, "source.last_event.type"),
        Some("response.completed".into())
    );
    assert_eq!(
        attribute(record, "source.last_event.sequence"),
        Some(terminal["sequence_number"].as_i64().unwrap().into())
    );
}

#[test]
fn missing_terminal_is_a_finish_failure_not_a_failure_of_the_last_valid_frame() {
    let mut events = morphiecore_test_support::responses_profile::events(2);
    events.pop();
    let mut attempt = Attempt::new(
        GenerationCodec::new(GenerationApi::Responses, Dialect::Standard, None),
        1 << 20,
        SseLimits::default(),
    );
    attempt.begin(200, "text/event-stream").unwrap();
    for event in events {
        attempt
            .push(format!("data: {event}\n\n").as_bytes())
            .unwrap();
    }
    let error = attempt.finish().unwrap_err();
    let (sink, exporter) = Sink::memory();
    let mut trace = Trace::new(Some(&sink));
    trace.decode_failure(&error, &attempt);
    drop(trace);
    let logs = exporter.get_emitted_logs().unwrap();
    assert_eq!(
        attribute(&logs[0].record, "decode.operation"),
        Some("finish".into())
    );
    assert_eq!(
        attribute(&logs[0].record, "source.last_event.type"),
        Some("response.output_item.done".into())
    );
    assert_eq!(
        attribute(&logs[0].record, "upstream.finish_validated"),
        Some(false.into())
    );
}

#[test]
fn preflight_names_the_incompatible_declaration_without_exporting_private_schema() {
    use crate::protocol::{
        CodecError, ConversionFailure,
        conversion::generation::ConversionBoundary,
        failure::{CarrierReason, ToolConstraint},
    };
    let client = GenerationCodec::new(GenerationApi::Responses, Dialect::Standard, None);
    let input = serde_json::json!({"model":"public","input":"synthetic-private-input",
        "tools":[
            {"type":"function","name":"synthetic-private-first","strict":true,"parameters":{
                "type":"object","properties":{},"additionalProperties":false}},
            {"type":"function","name":"synthetic-private-second","strict":false,"parameters":{
                "type":"object","properties":{"synthetic-private-property":{"type":"string","minLength":1}},
                "additionalProperties":false}}]});
    let request = client
        .decode_request(&serde_json::to_vec(&input).unwrap())
        .unwrap();
    let target = crate::protocol::native::ReplayTarget::new("synthetic", "model").unwrap();
    let error = crate::protocol::google::interactions::encode_generation_request(
        &request.task.semantic,
        &request.task.fidelity,
        &target,
    )
    .unwrap_err();
    assert_eq!(
        error,
        CodecError::Unsupported(CarrierReason::ToolConstraint {
            declaration: 1,
            constraint: ToolConstraint::MinLength,
        })
    );
    let generate_error = crate::protocol::google::generate_content::encode_generation_request(
        &request.task.semantic,
        &request.task.fidelity,
        &target,
        None,
    )
    .unwrap_err();
    assert_eq!(generate_error, error);
    let (sink, exporter) = Sink::memory();
    let mut trace = Trace::new(Some(&sink));
    trace.generation("public", crate::protocol::ClientProtocol::Responses);
    trace.planning_failure(&crate::execution::plan::PlanError::CandidateRejected(
        crate::execution::plan::RejectionReason::Conversion(Box::new(
            ConversionFailure::from(error).at(ConversionBoundary::RequestEncode),
        )),
    ));
    trace
        .report
        .request_failure("codec.unsupported.tool_constraint", 400);
    drop(trace);
    let logs = exporter.get_emitted_logs().unwrap();
    assert_eq!(logs.len(), 1);
    let record = &logs[0].record;
    assert_eq!(attribute(record, "model"), Some("public".into()));
    assert_eq!(attribute(record, "operation"), Some("responses".into()));
    assert_eq!(
        attribute(record, "tool.declaration_index"),
        Some(1i64.into())
    );
    assert_eq!(
        attribute(record, "tool.constraint"),
        Some("min_length".into())
    );
    assert_eq!(
        attribute(record, "gateway.error.status_code"),
        Some(400i64.into())
    );
    assert_eq!(attribute(record, "send_attempts"), Some(0i64.into()));
    assert!(attribute(record, "attempt").is_none());
    assert!(attribute(record, "http.response.status_code").is_none());
    assert!(!format!("{logs:?}").contains("synthetic-private"));
}

#[test]
fn source_failure_keeps_primary_class_and_later_decode_context() {
    let mut events = morphiecore_test_support::responses_profile::events(2);
    let terminal = events.last_mut().unwrap();
    terminal["type"] = serde_json::json!("response.failed");
    terminal["response"]["status"] = serde_json::json!("failed");
    terminal["response"]["error"] = serde_json::json!({
        "code":"rate_limit_exceeded","message":"synthetic-private-source-error"
    });
    let mut attempt = Attempt::new(
        GenerationCodec::new(GenerationApi::Responses, Dialect::Standard, None),
        1 << 20,
        SseLimits::default(),
    );
    attempt.begin(200, "text/event-stream").unwrap();
    for event in events {
        attempt
            .push(format!("data: {event}\n\n").as_bytes())
            .unwrap();
    }
    let source = attempt.reported_failure().unwrap();
    let (_, source) = crate::ApiError::interpret_source("openai", 429, source, None, false);
    let (sink, exporter) = Sink::memory();
    let mut trace = Trace::new(Some(&sink));
    trace.begin_candidate(1);
    trace.head(200);
    trace.report.source_failure(source);
    let error = attempt.push(b"data: {not-json}\n\n").unwrap_err();
    trace.decode_failure(&error, &attempt);
    attempt.cancel();
    trace.report.failure(
        None,
        &AttemptError::Delivery(DeliveryReason::BodyConsumerClosed),
    );
    drop(trace);
    let logs = exporter.get_emitted_logs().unwrap();
    assert_eq!(logs.len(), 1);
    let record = &logs[0].record;
    assert_eq!(
        attribute(record, "error.type"),
        Some("source.rate_limit".into())
    );
    assert_eq!(
        attribute(record, "handling.error.type"),
        Some("codec.invalid.json".into())
    );
    assert_eq!(attribute(record, "decode.operation"), Some("push".into()));
    assert_eq!(
        attribute(record, "source.last_event.type"),
        Some("missing".into())
    );
    assert!(!format!("{logs:?}").contains("synthetic-private"));
}

#[tokio::test]
async fn official_otlp_exporter_delivers_failure_logs_to_a_loopback_receiver() {
    use axum::{Json, Router, routing::post};
    use std::time::Duration;
    let (sent, mut received) = tokio::sync::mpsc::channel(1);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new()
                .route(
                    "/v1/logs",
                    post(move |Json(body): Json<serde_json::Value>| {
                        let sent = sent.clone();
                        async move {
                            sent.try_send(body).unwrap();
                            Json(serde_json::json!({}))
                        }
                    }),
                )
                .route("/v1/traces", post(|| async { Json(serde_json::json!({})) })),
        )
        .await
        .unwrap();
    });
    let sink = Sink::open(&DiagnosticsConfig {
        endpoint,
        level: LogLevel::Warn,
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
    assert_eq!(log["traceId"].as_str().unwrap().len(), 32);
    assert_eq!(log["spanId"].as_str().unwrap().len(), 16);
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
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
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
        level: LogLevel::Warn,
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
struct HeldLogReceiver {
    endpoint: String,
    records: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    entered: std::sync::Arc<tokio::sync::Notify>,
    release: std::sync::Arc<tokio::sync::Notify>,
    server: tokio::task::JoinHandle<()>,
}

impl Drop for HeldLogReceiver {
    fn drop(&mut self) {
        self.server.abort();
    }
}

async fn hold_first_log_export() -> HeldLogReceiver {
    use axum::{Json, Router, extract::DefaultBodyLimit, routing::post};
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    let records = Arc::new(AtomicUsize::new(0));
    let entered = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let output = records.clone();
    let started = entered.clone();
    let resume = release.clone();
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new()
                .route(
                    "/v1/logs",
                    post(move |Json(payload): Json<serde_json::Value>| {
                        let output = output.clone();
                        let started = started.clone();
                        let resume = resume.clone();
                        async move {
                            let count = payload["resourceLogs"]
                                .as_array()
                                .unwrap()
                                .iter()
                                .flat_map(|r| r["scopeLogs"].as_array().unwrap())
                                .flat_map(|s| s["logRecords"].as_array().unwrap())
                                .inspect(|record| {
                                    assert_eq!(record["eventName"], "gateway.failure")
                                })
                                .count();
                            if output.fetch_add(count, Ordering::SeqCst) == 0 {
                                started.notify_one();
                                resume.notified().await;
                            }
                            Json(serde_json::json!({}))
                        }
                    }),
                )
                .route("/v1/traces", post(|| async { Json(serde_json::json!({})) }))
                .layer(DefaultBodyLimit::max(1 << 20)),
        )
        .await
        .unwrap();
    });
    HeldLogReceiver {
        endpoint,
        records,
        entered,
        release,
        server,
    }
}

fn emit_synthetic_failures(sink: &Sink, count: usize, value: &str) {
    for _ in 0..count {
        sink.emit(
            "gateway.failure",
            Severity::Error,
            vec![],
            AnyValue::from(value.to_owned()),
        );
    }
}

#[tokio::test]
async fn queued_private_records_fit_the_otlp_request_bound_after_a_slow_export() {
    use std::{sync::atomic::Ordering, time::Duration};
    let receiver = hold_first_log_export().await;
    let sink = Sink::open(&DiagnosticsConfig {
        endpoint: receiver.endpoint.clone(),
        level: LogLevel::Warn,
        private_bodies: true,
        body_bytes: 64 << 10,
    })
    .unwrap();
    emit_synthetic_failures(&sink, 4, "synthetic");
    tokio::time::timeout(Duration::from_secs(3), receiver.entered.notified())
        .await
        .unwrap();
    emit_synthetic_failures(&sink, 24, &"x".repeat(64 << 10));
    receiver.release.notify_one();
    sink.flush().await;
    assert_eq!(
        receiver.records.load(Ordering::SeqCst),
        28,
        "healthy export must not discard an oversized accumulated batch"
    );
}

#[tokio::test]
async fn full_log_queue_drops_only_telemetry_and_reports_local_loss() {
    use std::{
        sync::{Arc, atomic::Ordering},
        time::Duration,
    };
    use tower::ServiceExt;
    let receiver = hold_first_log_export().await;
    let sink = Sink::open_with_queue(
        &DiagnosticsConfig {
            endpoint: receiver.endpoint.clone(),
            level: LogLevel::Warn,
            private_bodies: false,
            body_bytes: 1024,
        },
        8,
    )
    .unwrap();
    emit_synthetic_failures(&sink, 4, "first batch");
    tokio::time::timeout(Duration::from_secs(3), receiver.entered.notified())
        .await
        .unwrap();
    let first_batch = receiver.records.load(Ordering::SeqCst);
    assert!(!sink.loss_reported());
    emit_synthetic_failures(&sink, 24, "queued batch");
    assert!(
        sink.loss_reported(),
        "the SDK must indicate queue loss before any HTTP failure"
    );

    let models = |gate: crate::Gateway| async move {
        let response = gate
            .router()
            .oneshot(
                axum::http::Request::get("/v1/models")
                    .header(
                        "authorization",
                        format!("Bearer {}", crate::tests::support::KEY),
                    )
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        axum::body::to_bytes(response.into_body(), 64 << 10)
            .await
            .unwrap()
    };
    let mut gate = crate::tests::support::gateway(Default::default());
    let expected = models(gate.clone()).await;
    Arc::get_mut(&mut gate.state).unwrap().diagnostics = Some(sink.clone());
    let actual = tokio::time::timeout(Duration::from_secs(1), models(gate))
        .await
        .unwrap();
    assert_eq!(actual, expected);

    receiver.release.notify_one();
    tokio::time::timeout(Duration::from_secs(8), sink.shutdown())
        .await
        .unwrap();
    assert_eq!(receiver.records.load(Ordering::SeqCst), first_batch + 8);
}
