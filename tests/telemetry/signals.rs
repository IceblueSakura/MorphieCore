//! Scoped SDK providers exercise correlation without installing process-global state.
use super::{TARGET, app_filter, isolated_child, resource};
use opentelemetry::{KeyValue, metrics::MeterProvider as _, trace::TracerProvider as _};
use opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge;
use opentelemetry_sdk::{
    logs::{InMemoryLogExporter, SdkLoggerProvider},
    metrics::{
        InMemoryMetricExporter, PeriodicReader, SdkMeterProvider,
        data::{AggregatedMetrics, MetricData},
    },
    trace::{InMemorySpanExporter, Sampler, SdkTracerProvider},
};
use std::time::Duration;
use tracing::{Instrument, instrument::WithSubscriber};
use tracing_opentelemetry::OpenTelemetrySpanExt;
use tracing_subscriber::{Layer as _, layer::SubscriberExt};

struct Signals {
    dispatch: tracing::Dispatch,
    tracer: SdkTracerProvider,
    logger: SdkLoggerProvider,
    meter: SdkMeterProvider,
    spans: InMemorySpanExporter,
    logs: InMemoryLogExporter,
    metrics: InMemoryMetricExporter,
}
impl Signals {
    fn new(sampler: Sampler) -> Self {
        let spans = InMemorySpanExporter::default();
        let logs = InMemoryLogExporter::default();
        let metrics = InMemoryMetricExporter::default();
        let tracer = SdkTracerProvider::builder()
            .with_resource(resource())
            .with_sampler(sampler)
            .with_simple_exporter(spans.clone())
            .build();
        let logger = SdkLoggerProvider::builder()
            .with_resource(resource())
            .with_simple_exporter(logs.clone())
            .build();
        let meter = SdkMeterProvider::builder()
            .with_resource(resource())
            .with_reader(
                PeriodicReader::builder(metrics.clone())
                    .with_interval(Duration::from_secs(60))
                    .build(),
            )
            .build();
        // Events go to logs only, not also to span events. Error status is explicit.
        let traces = tracing_opentelemetry::layer()
            .with_tracer(tracer.tracer("morphiecore-test"))
            .with_error_events_to_status(false)
            .with_error_events_to_exceptions(false)
            .with_error_fields_to_exceptions(false)
            .with_error_records_to_exceptions(false)
            .with_filter(tracing_subscriber::filter::filter_fn(|metadata| {
                metadata.is_span() && metadata.target() == TARGET
            }));
        let subscriber = tracing_subscriber::registry()
            .with(traces)
            .with(OpenTelemetryTracingBridge::new(&logger).with_filter(app_filter()));
        Self {
            dispatch: tracing::Dispatch::new(subscriber),
            tracer,
            logger,
            meter,
            spans,
            logs,
            metrics,
        }
    }
    async fn shutdown(self) {
        tokio::task::spawn_blocking(move || {
            self.tracer.shutdown().unwrap();
            self.meter.shutdown().unwrap();
            self.logger.shutdown().unwrap();
        })
        .await
        .unwrap();
    }
}

#[tracing::instrument(skip_all, target = "morphiecore.telemetry", name = "projection")]
fn project(payload: &str) -> &'static str {
    assert!(!payload.is_empty());
    tracing::info!(
        name: "generation.adaptation", target: "morphiecore.telemetry",
        request_id = "synthetic-request", attempt = 2_u64,
        field = "generation.text.verbosity", action = "omit"
    );
    "synthetic-result"
}

#[tokio::test]
#[ignore = "environment-cleared ecosystem_checks child"]
async fn recovered_attempts_correlate_without_duplicate_events_or_payloads() {
    isolated_child();
    let signals = Signals::new(Sampler::AlwaysOn);
    let meter = signals.meter.meter("morphiecore-test");
    let attempts = meter.u64_counter("test.generation.attempts").build();
    let requests = meter.u64_counter("test.generation.requests").build();
    let duration = meter
        .f64_histogram("test.generation.duration")
        .with_unit("s")
        .build();
    let request = tracing::dispatcher::with_default(
        &signals.dispatch,
        || tracing::info_span!(target: "morphiecore.telemetry", "request", request_id = "synthetic-request"),
    );
    for ordinal in 1..=2 {
        let attempt = tracing::dispatcher::with_default(
            &signals.dispatch,
            || tracing::info_span!(target: "morphiecore.telemetry", parent: &request, "attempt", attempt = ordinal),
        );
        let counter = attempts.clone();
        let result = tokio::spawn(
            async move {
                // A real await/spawn boundary must retain the chosen request context.
                tokio::task::yield_now().await;
                if ordinal == 1 {
                    tracing::Span::current()
                        .set_status(opentelemetry::trace::Status::error("timeout"));
                    tracing::error!(
                        name: "generation.attempt.failed", target: "morphiecore.telemetry",
                        request_id = "synthetic-request", attempt = ordinal,
                        error_type = "timeout"
                    );
                    counter.add(1, &[KeyValue::new("outcome", "failed")]);
                    None
                } else {
                    let result = project("synthetic-private-prompt-and-tool-arguments");
                    tracing::warn!(target: "unapproved.library", "synthetic-private-library-error");
                    counter.add(1, &[KeyValue::new("outcome", "succeeded")]);
                    Some(result)
                }
            }
            .instrument(attempt.clone())
            .with_subscriber(signals.dispatch.clone()),
        )
        .await
        .unwrap();
        // Keep one owner handle until join: registry parent cleanup must run with
        // this scoped dispatcher, not the task's restored default dispatcher.
        tracing::dispatcher::with_default(&signals.dispatch, || drop(attempt));
        assert_eq!(result.is_some(), ordinal == 2);
    }
    tracing::dispatcher::with_default(&signals.dispatch, || {
        request.in_scope(|| {
            tracing::info!(name: "generation.request.finished", target: "morphiecore.telemetry", outcome = "succeeded");
        });
    });
    requests.add(1, &[KeyValue::new("outcome", "succeeded")]);
    duration.record(0.25, &[KeyValue::new("outcome", "succeeded")]);
    tracing::dispatcher::with_default(&signals.dispatch, || drop(request));
    signals.meter.force_flush().unwrap();
    let spans = signals.spans.get_finished_spans().unwrap();
    let root = spans
        .iter()
        .find(|span| span.name == "request")
        .unwrap_or_else(|| panic!("missing request span: {spans:#?}"));
    assert_eq!(root.status, opentelemetry::trace::Status::Unset);
    let children: Vec<_> = spans.iter().filter(|span| span.name == "attempt").collect();
    assert_eq!(children.len(), 2);
    assert!(
        children
            .iter()
            .all(|span| span.parent_span_id == root.span_context.span_id())
    );
    let attempt = |ordinal| {
        *children
            .iter()
            .find(|span| {
                span.attributes.iter().any(|kv| {
                    kv.key.as_str() == "attempt" && kv.value == opentelemetry::Value::I64(ordinal)
                })
            })
            .unwrap()
    };
    let failed = attempt(1);
    let succeeded = attempt(2);
    assert_eq!(
        failed.status,
        opentelemetry::trace::Status::error("timeout")
    );
    assert_eq!(succeeded.status, opentelemetry::trace::Status::Unset);
    let projection = spans.iter().find(|span| span.name == "projection").unwrap();
    assert_eq!(projection.parent_span_id, succeeded.span_context.span_id());
    assert_eq!(spans.len(), 4);
    assert!(spans.iter().all(|span| span.events.is_empty()));

    let logs = signals.logs.get_emitted_logs().unwrap();
    assert_eq!(logs.len(), 3);
    for log in &logs {
        let context = log.record.trace_context().unwrap();
        assert_eq!(context.trace_id, root.span_context.trace_id());
        let expected_span = match log.record.event_name().unwrap() {
            "generation.attempt.failed" => failed,
            "generation.adaptation" => projection,
            "generation.request.finished" => root,
            event => panic!("unexpected log event: {event}"),
        };
        assert_eq!(context.span_id, expected_span.span_context.span_id());
    }
    let final_log = logs
        .iter()
        .find(|log| log.record.event_name() == Some("generation.request.finished"))
        .unwrap();
    assert!(
        !final_log
            .record
            .attributes_iter()
            .any(|(key, _)| key.as_str() == "request_id"),
        "default log bridge does not copy tracing span fields"
    );
    let metrics = signals.metrics.get_finished_metrics().unwrap();
    let mut totals = std::collections::BTreeMap::new();
    let mut duration_seen = false;
    for metric in metrics
        .iter()
        .flat_map(|r| r.scope_metrics())
        .flat_map(|s| s.metrics())
    {
        match metric.data() {
            AggregatedMetrics::U64(MetricData::Sum(sum)) => {
                for point in sum.data_points() {
                    let attributes = point.attributes().collect::<Vec<_>>();
                    assert_eq!(attributes.len(), 1);
                    assert_eq!(attributes[0].key.as_str(), "outcome");
                    assert!(
                        totals
                            .insert(
                                (metric.name().to_owned(), attributes[0].value.to_string()),
                                point.value(),
                            )
                            .is_none()
                    );
                }
            }
            AggregatedMetrics::F64(MetricData::Histogram(histogram)) => {
                assert_eq!(metric.name(), "test.generation.duration");
                assert_eq!(metric.unit(), "s");
                let points = histogram.data_points().collect::<Vec<_>>();
                assert_eq!(points.len(), 1);
                let point = points[0];
                assert_eq!((point.count(), point.sum()), (1, 0.25));
                assert_eq!(
                    point.attributes().cloned().collect::<Vec<_>>(),
                    vec![KeyValue::new("outcome", "succeeded")]
                );
                duration_seen = true;
            }
            other => panic!("unexpected metric: {other:?}"),
        }
    }
    assert_eq!(
        totals,
        [
            (
                ("test.generation.attempts".to_owned(), "failed".to_owned()),
                1
            ),
            (
                (
                    "test.generation.attempts".to_owned(),
                    "succeeded".to_owned()
                ),
                1
            ),
            (
                (
                    "test.generation.requests".to_owned(),
                    "succeeded".to_owned()
                ),
                1
            ),
        ]
        .into()
    );
    assert!(duration_seen);
    let emitted = format!("{spans:?}{logs:?}{metrics:?}");
    assert!(!emitted.contains("synthetic-private"));
    signals.shutdown().await;
}

#[tokio::test]
#[ignore = "environment-cleared ecosystem_checks child"]
async fn worker_abort_closes_spans_but_does_not_invent_completion_or_require_sampling() {
    isolated_child();
    for (sampler, expected_sampled) in [(Sampler::AlwaysOn, true), (Sampler::AlwaysOff, false)] {
        let signals = Signals::new(sampler);
        let request = tracing::dispatcher::with_default(
            &signals.dispatch,
            || tracing::info_span!(target: "morphiecore.telemetry", "stream-request"),
        );
        let (started, ready) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(
            async move {
                tracing::info!(name: "generation.prefix.checked", target: "morphiecore.telemetry", action = "omit");
                started.send(()).unwrap();
                std::future::pending::<()>().await;
                tracing::info!(name: "generation.request.finished", target: "morphiecore.telemetry", outcome = "succeeded");
            }
            .instrument(request.clone())
            .with_subscriber(signals.dispatch.clone()),
        );
        tokio::time::timeout(Duration::from_secs(2), ready)
            .await
            .unwrap()
            .unwrap();
        assert!(signals.spans.get_finished_spans().unwrap().is_empty());
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        drop(request);
        let logs = signals.logs.get_emitted_logs().unwrap();
        assert_eq!(logs.len(), 1);
        assert_eq!(
            logs[0].record.event_name(),
            Some("generation.prefix.checked")
        );
        let spans = signals.spans.get_finished_spans().unwrap();
        let sampled = logs[0]
            .record
            .trace_context()
            .unwrap()
            .trace_flags
            .unwrap()
            .is_sampled();
        assert_eq!(sampled, expected_sampled);
        assert_eq!(spans.len(), usize::from(expected_sampled));
        if let Some(span) = spans.first() {
            assert_eq!(
                span.status,
                opentelemetry::trace::Status::Unset,
                "span closure is not a cancellation or success classifier"
            );
        }
        signals.shutdown().await;
    }
}
