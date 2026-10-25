//! Scoped application instrumentation. Only local exporters are constructed here.
use super::{Outcome, Record, writer::Output};
use opentelemetry::{
    logs::{LogRecord as _, Logger as _, LoggerProvider as _, Severity},
    trace::{Status, TraceContextExt, TracerProvider as _},
};
use opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge;
use opentelemetry_sdk::{
    Resource,
    logs::{SdkLogger, SdkLoggerProvider},
    trace::{Sampler, SdkTracerProvider, SpanLimits},
};
use std::{sync::Arc, time::SystemTime};
use tracing_opentelemetry::OpenTelemetrySpanExt;
use tracing_subscriber::{Layer as _, layer::SubscriberExt};
#[cfg(test)]
mod tests;

pub(super) const TARGET: &str = "morphiecore.telemetry";
const ATTEMPT_FINISHED: &str = "gateway.attempt.finished";

pub(super) struct Telemetry {
    dispatch: tracing::Dispatch,
    logger: SdkLogger,
    // Retained until all request/body owners release their Sink.
    logger_provider: SdkLoggerProvider,
    tracer_provider: SdkTracerProvider,
    output: Output,
}
impl Telemetry {
    pub(super) fn new(output: Output) -> Arc<Self> {
        // SDK builders still read discarded environment defaults internally. Never
        // let their diagnostics use an ambient subscriber or their defaults win.
        tracing::dispatcher::with_default(&tracing::Dispatch::none(), || {
            let resource = Resource::builder_empty()
                .with_service_name("morphiecore")
                .with_attribute(opentelemetry::KeyValue::new(
                    "service.version",
                    env!("CARGO_PKG_VERSION"),
                ))
                .build();
            let tracer_provider = SdkTracerProvider::builder()
                .with_resource(resource.clone())
                .with_sampler(Sampler::AlwaysOn)
                .with_span_limits(SpanLimits {
                    max_events_per_span: 0,
                    max_attributes_per_span: 16,
                    max_links_per_span: 0,
                    max_attributes_per_event: 0,
                    max_attributes_per_link: 0,
                })
                .with_simple_exporter(super::export::LocalExporter::new(output.clone()))
                .build();
            let logger_provider = SdkLoggerProvider::builder()
                .with_resource(resource)
                .with_simple_exporter(super::export::LocalExporter::new(output.clone()))
                .build();
            let traces = tracing_opentelemetry::layer()
                .with_tracer(tracer_provider.tracer("morphiecore.gateway"))
                .with_error_events_to_status(false)
                .with_error_events_to_exceptions(false)
                .with_error_fields_to_exceptions(false)
                .with_error_records_to_exceptions(false)
                .with_location(false)
                .with_threads(false)
                .with_level(false)
                .with_filter(tracing_subscriber::filter::filter_fn(|m| {
                    m.is_span() && m.target() == TARGET && matches!(m.name(), "request" | "attempt")
                }));
            let logs = OpenTelemetryTracingBridge::new(&logger_provider).with_filter(
                tracing_subscriber::filter::filter_fn(|m| {
                    m.is_event() && m.target() == TARGET && m.name() == ATTEMPT_FINISHED
                }),
            );
            let dispatch =
                tracing::Dispatch::new(tracing_subscriber::registry().with(traces).with(logs));
            Arc::new(Self {
                dispatch,
                logger: logger_provider.logger("morphiecore.gateway"),
                logger_provider,
                tracer_provider,
                output,
            })
        })
    }
    pub(super) fn request(self: &Arc<Self>, id: &str) -> Request {
        let root = tracing::dispatcher::with_default(
            &self.dispatch,
            || tracing::info_span!(target: TARGET, parent: None, "request", request_id = id),
        );
        Request {
            telemetry: self.clone(),
            root: Some(root),
            attempt: None,
            request_id: id.to_owned(),
        }
    }
}
impl Drop for Telemetry {
    fn drop(&mut self) {
        // These local simple exporters only enqueue. No HTTP, retries or blocking
        // file flush occurs in SDK shutdown; the writer drains after its senders go.
        tracing::dispatcher::with_default(&tracing::Dispatch::none(), || {
            if self
                .logger_provider
                .shutdown_with_timeout(std::time::Duration::from_millis(100))
                .is_err()
            {
                self.output.encoding_failed();
            }
            if self
                .tracer_provider
                .shutdown_with_timeout(std::time::Duration::from_millis(100))
                .is_err()
            {
                self.output.encoding_failed();
            }
        });
    }
}

pub(super) struct Request {
    telemetry: Arc<Telemetry>,
    root: Option<tracing::Span>,
    attempt: Option<(tracing::Span, usize)>,
    request_id: String,
}
impl Request {
    pub(super) fn begin(&mut self, ordinal: usize) {
        self.end(Some("interrupted"), false);
        let attribute_ordinal = i64::try_from(ordinal).unwrap_or(i64::MAX);
        self.attempt = Some((
            tracing::dispatcher::with_default(
                &self.telemetry.dispatch,
                || tracing::info_span!(target: TARGET, parent: self.root.as_ref().unwrap(), "attempt", attempt = attribute_ordinal, request_id = self.request_id.as_str()),
            ),
            ordinal,
        ));
    }
    pub(super) fn end(&mut self, code: Option<&'static str>, advanced: bool) {
        let Some((attempt, ordinal)) = self.attempt.take() else {
            return;
        };
        tracing::dispatcher::with_default(&self.telemetry.dispatch, || {
            let ordinal = i64::try_from(ordinal).unwrap_or(i64::MAX);
            attempt.set_status(match code {
                Some(code) => Status::error(code),
                None => Status::Ok,
            });
            attempt.in_scope(|| {
                tracing::info!(name: "gateway.attempt.finished", target: TARGET,
                    request_id = self.request_id.as_str(),
                    attempt = ordinal,
                    outcome = if code.is_some() {"failed"} else {"succeeded"},
                    error_type = code.unwrap_or("none"), advanced);
            });
            drop(attempt);
        });
    }
    pub(super) fn finish(mut self, record: &Record) {
        self.end(Some("interrupted"), false);
        let Some(root) = self.root.take() else {
            return;
        };
        tracing::dispatcher::with_default(&self.telemetry.dispatch, || {
            let complete = matches!(record.outcome, Outcome::Complete);
            root.set_status(if complete {
                Status::Ok
            } else {
                Status::error(record.outcome.label())
            });
            // Use the native log API for a structured body: the tracing bridge
            // formats complex Debug values as strings. No second report is emitted.
            let body = serde_json::to_value(record)
                .ok()
                .and_then(|v| super::export::body(v).ok());
            if let Some(body) = body {
                let mut log = self.telemetry.logger.create_log_record();
                log.set_event_name(if record.generation.is_some() {
                    "generation.request.finished"
                } else {
                    "gateway.request.finished"
                });
                log.set_target(TARGET);
                log.set_timestamp(SystemTime::now());
                log.set_severity_number(if complete {
                    Severity::Info
                } else {
                    Severity::Error
                });
                log.set_severity_text(if complete { "INFO" } else { "ERROR" });
                log.set_body(body);
                log.add_attribute(
                    "morphiecore.body.encoding",
                    "json_optional_nulls_omitted_wide_uint_decimal",
                );
                let context = root.context();
                let span = context.span();
                let sc = span.span_context();
                log.set_trace_context(sc.trace_id(), sc.span_id(), Some(sc.trace_flags()));
                self.telemetry.logger.emit(log);
            } else {
                self.telemetry.output.encoding_failed();
            }
            drop(root);
        });
    }
}
impl Drop for Request {
    fn drop(&mut self) {
        self.end(Some("interrupted"), false);
        if let Some(root) = self.root.take() {
            tracing::dispatcher::with_default(&self.telemetry.dispatch, || {
                root.set_status(Status::error("interrupted"));
                drop(root);
            });
        }
    }
}
