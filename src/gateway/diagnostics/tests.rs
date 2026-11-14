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
#[test]
fn generation_timeline_is_bounded_and_separates_terminal_eof_and_handoff() {
    let (sink, exporter) = Sink::memory_at(LogLevel::Debug);
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
    trace.terminal_validated();
    trace.finish_validated();
    trace.publishing();
    trace.handoff(true);
    trace.outcome(Outcome::Complete);
    trace.end_candidate(None, false);
    drop(trace);
    let logs = exporter.get_emitted_logs().unwrap();
    assert_eq!(logs.len(), 9);
    let records: Vec<_> = logs.iter().map(|log| &log.record).collect();
    let request_id = attribute(records[0], "request_id").unwrap();
    for (index, record) in records.iter().enumerate() {
        assert_eq!(attribute(record, "request_id"), Some(request_id.clone()));
        assert_eq!(
            attribute(record, "event.sequence"),
            Some(((index + 1) as i64).into())
        );
    }
    let terminal = records[4];
    assert_eq!(
        attribute(terminal, "upstream.terminal_validated"),
        Some(true.into())
    );
    assert_eq!(
        attribute(terminal, "upstream.finish_validated"),
        Some(false.into())
    );
    assert_eq!(
        attribute(terminal, "client.handoff_complete"),
        Some(false.into())
    );
    let summary = records.last().unwrap();
    assert_eq!(summary.severity_number(), Some(Severity::Info));
    assert_eq!(attribute(summary, "outcome"), Some("complete".into()));
    assert_eq!(attribute(summary, "send_attempts"), Some(1i64.into()));
    assert_eq!(
        attribute(summary, "client.handoff_complete"),
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
        attribute(&logs[0].record, "outcome"),
        Some("cancelled".into())
    );
}

#[test]
fn failed_terminal_snapshot_keeps_source_context_after_cancellation() {
    let mut events = crate::test_responses_wire::events(2);
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
    let mut events = crate::test_responses_wire::events(2);
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
    let mut events = crate::test_responses_wire::events(2);
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
    let (_, source) =
        crate::gateway::ApiError::interpret_source("openai", 429, source, None, false);
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
