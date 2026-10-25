use super::*;
use crate::gateway::diagnostics::{Sink, Trace};
use opentelemetry::logs::AnyValue;
use serde_json::{Value, json};
#[path = "../../../../tests/support/child_process.rs"]
mod child_process;

fn read(path: &std::path::Path) -> Vec<Value> {
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}
fn select<'a>(rows: &'a [Value], event: &str) -> Vec<&'a Value> {
    rows.iter().filter(|row| row["event"] == event).collect()
}

#[tokio::test]
async fn reports_keep_native_bodies_correlated_recovery_and_runtime_isolation() {
    let first = crate::test_files::private_directory();
    let second = crate::test_files::private_directory();
    let a = first.path().join("events.jsonl");
    let b = second.path().join("events.jsonl");
    let sink_a = Sink::open(&a).unwrap();
    let sink_b = Sink::open(&b).unwrap();
    let remote = opentelemetry::trace::SpanContext::new(
        opentelemetry::TraceId::from_bytes([17; 16]),
        opentelemetry::SpanId::from_bytes([34; 8]),
        opentelemetry::TraceFlags::SAMPLED,
        true,
        Default::default(),
    );
    let ambient = opentelemetry::Context::new().with_remote_span_context(remote);
    let mut trace = {
        let _guard = ambient.attach();
        Trace::generation(Some(&sink_a))
    };
    let id = trace.record.request_id.clone().unwrap();
    let instrumentation = trace.telemetry.as_ref().unwrap();
    tracing::dispatcher::with_default(&instrumentation.telemetry.dispatch, || {
        instrumentation.root.as_ref().unwrap().in_scope(|| {
            tracing::info!(target:"unapproved.library","synthetic-private-prompt");
            tracing::info!(name:"unregistered.event",target:TARGET,error="synthetic-private-error");
            let unregistered=tracing::info_span!(target:TARGET,"unregistered.span",payload="synthetic-private-arguments");
            drop(unregistered);
        });
    });
    trace.begin_candidate(1);
    trace.decode_error(&crate::execution::AttemptError::Codec(
        crate::protocol::CodecError::Invalid(crate::protocol::failure::ShapeReason::Json),
    ));
    trace.end_candidate(Some("upstream_error"), true);
    trace.begin_candidate(2);
    trace.end_candidate(None, false);
    // A source counter above i64 is not rounded to an approximate double.
    trace.record.reported_cached_tokens = Some(u64::MAX);
    trace.outcome(Outcome::Complete);
    tokio::spawn(async move {
        tokio::task::yield_now().await;
        drop(trace);
    })
    .await
    .unwrap();
    let mut other = Trace::generation(Some(&sink_b));
    other.outcome(Outcome::Timeout);
    drop(other);
    sink_a.flush().await;
    sink_b.flush().await;
    let rows = read(&a);
    assert!(
        !serde_json::to_string(&rows)
            .unwrap()
            .contains("synthetic-private")
    );
    let other = read(&b);
    let requests = select(&rows, "request");
    assert_eq!(requests.len(), 1);
    let request = requests[0];
    assert_eq!(request["request_id"], id);
    assert!(request["generation"].is_object());
    assert_eq!(request["reported_cached_tokens"], u64::MAX.to_string());
    assert_eq!(request["generation"]["result"], "succeeded");
    let spans = select(&rows, "span");
    assert_eq!(spans.len(), 3);
    let root = *spans.iter().find(|row| row["name"] == "request").unwrap();
    assert_eq!(root["status"], "ok");
    assert_eq!(root["parent_span_id"], "0000000000000000");
    assert_ne!(root["trace_id"], "11111111111111111111111111111111");
    assert_eq!(request["telemetry"]["trace_id"], root["trace_id"]);
    assert_eq!(request["telemetry"]["span_id"], root["span_id"]);
    for ordinal in 1..=2 {
        let attempt = *spans
            .iter()
            .find(|row| row["attributes"]["attempt"] == ordinal)
            .unwrap();
        assert_eq!(attempt["parent_span_id"], root["span_id"]);
        assert_eq!(attempt["trace_id"], root["trace_id"]);
        assert_eq!(attempt["status"], if ordinal == 1 { "error" } else { "ok" });
        let logs = select(&rows, "log");
        assert_eq!(logs.len(), 2);
        let log = *logs
            .iter()
            .find(|row| row["telemetry"]["attributes"]["attempt"] == ordinal)
            .unwrap();
        assert_eq!(log["telemetry"]["event_name"], "gateway.attempt.finished");
        assert_eq!(log["telemetry"]["trace_id"], root["trace_id"]);
        assert_eq!(log["telemetry"]["span_id"], attempt["span_id"]);
        assert_eq!(log["telemetry"]["attributes"]["request_id"], id);
    }
    assert_eq!(select(&other, "request").len(), 1);
    assert_ne!(
        select(&other, "request")[0]["telemetry"]["trace_id"],
        root["trace_id"]
    );
    assert!(!serde_json::to_string(&other).unwrap().contains(&id));
    // No subscriber/global context was installed for unrelated library events.
    assert!(tracing::Span::current().is_none());
}

#[tokio::test]
async fn body_abort_keeps_the_runtime_alive_and_closes_interrupted_spans_once() {
    use crate::execution::report::View;
    let directory = crate::test_files::private_directory();
    let path = directory.path().join("events.jsonl");
    let sink = Sink::open(&path).unwrap();
    let mut trace = Trace::generation(Some(&sink));
    trace.begin_candidate(1);
    trace.outcome(Outcome::Interrupted);
    let frames = trace.report.batch(
        View::DownstreamSse,
        vec![bytes::Bytes::from_static(b"acknowledged")],
        Default::default(),
    );
    let (_, ack) = frames.into_acknowledged_frames().next().unwrap();
    assert!(ack.unwrap().acknowledge(|| true));
    // Cancellation before producer resumption must not lose this observation
    // or require a second trace.handed_off call to synchronize a parallel counter.
    let runtime = Arc::downgrade(&trace.telemetry.as_ref().unwrap().telemetry);
    let output = trace.telemetry.as_ref().unwrap().telemetry.output.clone();
    drop(sink);
    assert!(runtime.upgrade().is_some());
    let (ready, started) = tokio::sync::oneshot::channel();
    let task = tokio::spawn(async move {
        let _trace = trace;
        let _ = ready.send(());
        std::future::pending::<()>().await;
    });
    started.await.unwrap();
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    assert!(runtime.upgrade().is_none());
    output.flush().await;
    let rows = read(&path);
    let requests = select(&rows, "request");
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0]["generation"]["result"], "interrupted");
    assert_eq!(requests[0]["handed_off_bytes"], 12);
    assert_eq!(requests[0]["candidates"][0]["handed_off_bytes"], 12);
    assert_eq!(
        requests[0]["generation"]["attempts"][0]["handed_off_bytes"],
        12
    );
    let spans = select(&rows, "span");
    assert_eq!(spans.len(), 2);
    assert!(spans.iter().all(|row| row["status"] == "error"));
    assert_eq!(select(&rows, "log").len(), 1);
}

#[test]
fn native_body_encoding_is_explicit_and_does_not_relabel_unknowns_or_wide_counts() {
    let encoded = super::super::export::body(
        json!({"unknown":null,"zero":0,"max":u64::MAX,"list":[true,1,"x"]}),
    )
    .unwrap();
    let AnyValue::Map(map) = encoded else {
        panic!("not a structured body")
    };
    assert!(!map.contains_key(&opentelemetry::Key::from("unknown")));
    assert_eq!(map[&opentelemetry::Key::from("zero")], AnyValue::Int(0));
    assert_eq!(
        map[&opentelemetry::Key::from("max")],
        AnyValue::String(u64::MAX.to_string().into())
    );
    assert!(super::super::export::body(json!([null])).is_err());
}

#[tokio::test]
async fn explicit_configuration_isolated_from_synthetic_sdk_environment() {
    let mut command = tokio::process::Command::new(std::env::current_exe().unwrap());
    command
        .args([
            "--ignored",
            "--exact",
            "gateway::diagnostics::telemetry::tests::synthetic_environment_child",
            "--nocapture",
        ])
        .env_clear()
        .env("MORPHIECORE_LOCAL_TELEMETRY_TEST", "isolated")
        .env(
            "OTEL_RESOURCE_ATTRIBUTES",
            "synthetic.private=synthetic-private-resource",
        )
        .env("OTEL_SERVICE_NAME", "synthetic-private-service")
        .env("OTEL_TRACES_SAMPLER", "synthetic-private-invalid-sampler")
        .env("OTEL_SPAN_ATTRIBUTE_COUNT_LIMIT", "0")
        .env("OTEL_SDK_DISABLED", "true")
        .env(
            "OTEL_EXPORTER_OTLP_ENDPOINT",
            "http://127.0.0.1:1/synthetic-private-destination",
        )
        .env(
            "OTEL_EXPORTER_OTLP_HEADERS",
            "x-private=synthetic-private-header",
        );
    let output = child_process::run(
        &mut command,
        b"",
        std::time::Duration::from_secs(25),
        32 << 10,
    )
    .await
    .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("synthetic-private"));
    assert!(!String::from_utf8_lossy(&output.stderr).contains("synthetic-private"));
}

#[tokio::test]
#[ignore = "owned environment-cleared child"]
async fn synthetic_environment_child() {
    assert_eq!(
        std::env::var("MORPHIECORE_LOCAL_TELEMETRY_TEST").as_deref(),
        Ok("isolated")
    );
    let directory = crate::test_files::private_directory();
    let path = directory.path().join("events.jsonl");
    let sink = Sink::open(&path).unwrap();
    let mut trace = Trace::generation(Some(&sink));
    trace.begin_candidate(1);
    trace.end_candidate(None, false);
    trace.outcome(Outcome::Complete);
    drop(trace);
    sink.flush().await;
    let rows = read(&path);
    let text = serde_json::to_string(&rows).unwrap();
    assert!(!text.contains("synthetic-private"));
    assert_eq!(select(&rows, "span").len(), 2);
    let requests = select(&rows, "request");
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0]["telemetry"]["resource"]["service.name"],
        "morphiecore"
    );
    assert_eq!(requests[0]["telemetry"]["scope"], "morphiecore.gateway");
    assert_eq!(requests[0]["telemetry"]["severity"], "INFO");
}
