//! Official Logs/Traces pipelines with isolated subscribers and safe exporter failures.
use opentelemetry::{
    KeyValue,
    logs::{AnyValue, LogRecord as _, Logger as _, LoggerProvider as _, Severity},
    trace::TracerProvider as _,
};
use opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge;
use opentelemetry_otlp::{Protocol, WithExportConfig, WithHttpConfig};
use opentelemetry_sdk::{
    Resource,
    error::{OTelSdkError, OTelSdkResult},
    logs::{
        BatchConfigBuilder, BatchLogProcessor, LogBatch, LogExporter, SdkLogger, SdkLoggerProvider,
    },
    trace::{
        BatchConfigBuilder as TraceBatchConfigBuilder, BatchSpanProcessor, Sampler,
        SdkTracerProvider, SpanData, SpanExporter, SpanLimits,
    },
};
use serde::{Deserialize, Serialize};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant, SystemTime},
};
use tracing::instrument::WithSubscriber;
use tracing_subscriber::{Layer, filter::filter_fn, layer::SubscriberExt};

/// Trusted OTLP/HTTP base URL; business inputs never select telemetry destinations.
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DiagnosticsConfig {
    pub endpoint: String,
    /// Log verbosity is independent of trace collection.
    #[serde(default)]
    pub level: LogLevel,
    #[serde(default)]
    pub private_bodies: bool,
    #[serde(default = "body_bytes")]
    pub body_bytes: usize,
}
#[derive(Clone, Copy, Default, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Error,
    Warn,
    #[default]
    Info,
    Debug,
}
impl LogLevel {
    fn allows(self, severity: Severity) -> bool {
        severity
            >= match self {
                Self::Error => Severity::Error,
                Self::Warn => Severity::Warn,
                Self::Info => Severity::Info,
                Self::Debug => Severity::Debug,
            }
    }
    fn allows_event(self, level: &tracing::Level) -> bool {
        self.allows(match *level {
            tracing::Level::ERROR => Severity::Error,
            tracing::Level::WARN => Severity::Warn,
            tracing::Level::INFO => Severity::Info,
            _ => Severity::Debug,
        })
    }
}
fn body_bytes() -> usize {
    16 << 10
}
impl DiagnosticsConfig {
    pub(crate) fn validate(&self) -> std::io::Result<()> {
        if !(1024..=64 << 10).contains(&self.body_bytes) {
            return Err(std::io::Error::other("invalid diagnostic body bounds"));
        }
        let url = url::Url::parse(&self.endpoint)
            .map_err(|_| std::io::Error::other("invalid diagnostics endpoint"))?;
        if !matches!(url.scheme(), "http" | "https")
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err(std::io::Error::other("invalid diagnostics endpoint"));
        }
        // Explicit endpoints/resources cannot turn ambient secrets into export headers.
        if [
            "OTEL_EXPORTER_OTLP_HEADERS",
            "OTEL_EXPORTER_OTLP_LOGS_HEADERS",
            "OTEL_EXPORTER_OTLP_TRACES_HEADERS",
        ]
        .iter()
        .any(|name| std::env::var_os(name).is_some())
        {
            return Err(std::io::Error::other(
                "ambient OTLP authentication is not supported",
            ));
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Default)]
struct Warning(Arc<AtomicBool>);
impl Warning {
    fn report(&self) {
        if !self.0.swap(true, Ordering::Relaxed) {
            eprintln!("MorphieCore telemetry incomplete; records may be lost");
        }
    }
}
impl<S: tracing::Subscriber> Layer<S> for Warning {
    fn on_event(&self, event: &tracing::Event<'_>, _: tracing_subscriber::layer::Context<'_, S>) {
        if event.metadata().target().starts_with("opentelemetry")
            && *event.metadata().level() <= tracing::Level::WARN
        {
            self.report();
        }
    }
}
#[derive(Debug)]
struct Logs {
    inner: opentelemetry_otlp::LogExporter,
    warning: Warning,
    dispatch: tracing::Dispatch,
}
impl LogExporter for Logs {
    async fn export(&self, batch: LogBatch<'_>) -> OTelSdkResult {
        self.inner
            .export(batch)
            .with_subscriber(self.dispatch.clone())
            .await
            .map_err(|_| {
                self.warning.report();
                OTelSdkError::InternalFailure("OTLP log export failed".into())
            })
    }
    fn set_resource(&mut self, resource: &Resource) {
        self.inner.set_resource(resource);
    }
    fn shutdown_with_timeout(&self, timeout: Duration) -> OTelSdkResult {
        self.inner.shutdown_with_timeout(timeout)
    }
}
#[derive(Debug)]
struct Traces {
    inner: opentelemetry_otlp::SpanExporter,
    warning: Warning,
    dispatch: tracing::Dispatch,
}
impl SpanExporter for Traces {
    async fn export(&self, batch: Vec<SpanData>) -> OTelSdkResult {
        self.inner
            .export(batch)
            .with_subscriber(self.dispatch.clone())
            .await
            .map_err(|_| {
                self.warning.report();
                OTelSdkError::InternalFailure("OTLP trace export failed".into())
            })
    }
    fn set_resource(&mut self, resource: &Resource) {
        self.inner.set_resource(resource);
    }
    fn shutdown_with_timeout(&self, timeout: Duration) -> OTelSdkResult {
        self.inner.shutdown_with_timeout(timeout)
    }
}
struct Telemetry {
    logger: SdkLogger,
    logs: SdkLoggerProvider,
    traces: SdkTracerProvider,
    dispatch: tracing::Dispatch,
    warning: Warning,
    level: LogLevel,
}
impl Drop for Telemetry {
    fn drop(&mut self) {
        // The composition root owns draining; body/request destructors never wait.
        let _ = self.traces.shutdown_with_timeout(Duration::ZERO);
        let _ = self.logs.shutdown_with_timeout(Duration::ZERO);
    }
}
#[derive(Clone)]
pub(crate) struct Sink(Arc<Telemetry>);
fn application(metadata: &tracing::Metadata<'_>) -> bool {
    metadata.target().starts_with("morphiecore.")
}
impl Sink {
    pub(crate) fn open(config: &DiagnosticsConfig) -> std::io::Result<Self> {
        config.validate()?;
        let endpoint = config.endpoint.trim_end_matches('/').to_owned();
        let level = config.level;
        std::thread::spawn(move || {
            tracing::dispatcher::with_default(&tracing::Dispatch::none(), || {
                let client = reqwest::blocking::Client::builder()
                    .no_proxy()
                    .redirect(reqwest::redirect::Policy::none())
                    .timeout(Duration::from_secs(2))
                    .build()
                    .map_err(|_| std::io::Error::other("diagnostics HTTP client"))?;
                let logs = opentelemetry_otlp::LogExporter::builder()
                    .with_http()
                    .with_endpoint(format!("{endpoint}/v1/logs"))
                    .with_protocol(Protocol::HttpJson)
                    .with_timeout(Duration::from_secs(2))
                    .with_http_client(client.clone())
                    .with_retry_policy(
                        opentelemetry_otlp::RetryPolicy::default().with_max_retries(0),
                    )
                    .with_max_request_body_size(1 << 20)
                    .build()
                    .map_err(|_| std::io::Error::other("diagnostics log exporter"))?;
                let traces = opentelemetry_otlp::SpanExporter::builder()
                    .with_http()
                    .with_endpoint(format!("{endpoint}/v1/traces"))
                    .with_protocol(Protocol::HttpJson)
                    .with_timeout(Duration::from_secs(2))
                    .with_http_client(client)
                    .with_retry_policy(
                        opentelemetry_otlp::RetryPolicy::default().with_max_retries(0),
                    )
                    .with_max_request_body_size(1 << 20)
                    .build()
                    .map_err(|_| std::io::Error::other("diagnostics trace exporter"))?;
                let warning = Warning::default();
                let errors =
                    tracing::Dispatch::new(tracing_subscriber::registry().with(warning.clone()));
                let resource = Resource::builder_empty()
                    .with_service_name("morphiecore")
                    .with_attribute(KeyValue::new("service.version", env!("CARGO_PKG_VERSION")))
                    .with_attribute(KeyValue::new(
                        "morphiecore.build.revision",
                        option_env!("MORPHIECORE_REVISION").unwrap_or("unknown"),
                    ))
                    .build();
                let logs = SdkLoggerProvider::builder()
                    .with_resource(resource.clone())
                    .with_log_processor(
                        BatchLogProcessor::builder(Logs {
                            inner: logs,
                            warning: warning.clone(),
                            dispatch: errors.clone(),
                        })
                        .with_batch_config(
                            BatchConfigBuilder::default()
                                .with_max_queue_size(2048)
                                // Capture can retain 64 KiB; keep serialized log
                                // batches below the exporter's 1 MiB wire bound.
                                .with_max_export_batch_size(4)
                                .with_scheduled_delay(Duration::from_millis(100))
                                .build(),
                        )
                        .build(),
                    )
                    .build();
                let traces = SdkTracerProvider::builder()
                    .with_resource(resource)
                    .with_sampler(Sampler::AlwaysOn)
                    .with_span_limits(SpanLimits::default())
                    .with_span_processor(
                        BatchSpanProcessor::builder(Traces {
                            inner: traces,
                            warning: warning.clone(),
                            dispatch: errors,
                        })
                        .with_batch_config(
                            TraceBatchConfigBuilder::default()
                                .with_max_queue_size(2048)
                                .with_max_export_batch_size(128)
                                .with_scheduled_delay(Duration::from_millis(100))
                                .build(),
                        )
                        .build(),
                    )
                    .build();
                Ok(Self::from_providers(logs, traces, warning, level, true))
            })
        })
        .join()
        .map_err(|_| std::io::Error::other("diagnostics initialization"))?
    }
    fn from_providers(
        logs: SdkLoggerProvider,
        traces: SdkTracerProvider,
        warning: Warning,
        level: LogLevel,
        console: bool,
    ) -> Self {
        let tracer = traces.tracer("morphiecore.runtime");
        let trace_layer = tracing_opentelemetry::layer()
            .with_tracer(tracer)
            .with_error_fields_to_exceptions(false)
            .with_error_events_to_exceptions(false)
            .with_error_records_to_exceptions(false)
            .with_error_events_to_status(false)
            .with_location(false)
            .with_threads(false)
            .with_tracked_inactivity(false)
            .with_filter(filter_fn(application));
        let log_layer = OpenTelemetryTracingBridge::new(&logs).with_filter(filter_fn(move |m| {
            application(m) && level.allows_event(m.level())
        }));
        let console_layer = console.then(|| {
            tracing_subscriber::fmt::layer()
                .with_writer(std::io::stderr)
                .with_target(false)
                .with_ansi(false)
                .with_filter(filter_fn(move |m| {
                    application(m) && level.allows_event(m.level())
                }))
        });
        let dispatch = tracing::Dispatch::new(
            tracing_subscriber::registry()
                .with(warning.clone())
                .with(trace_layer)
                .with(log_layer)
                .with(console_layer),
        );
        let logger = logs.logger("morphiecore.gateway");
        Self(Arc::new(Telemetry {
            logger,
            logs,
            traces,
            dispatch,
            warning,
            level,
        }))
    }
    pub(crate) fn dispatch(&self) -> tracing::Dispatch {
        self.0.dispatch.clone()
    }
    #[cfg(any(test, feature = "test-hooks"))]
    pub(crate) fn memory() -> (Self, opentelemetry_sdk::logs::InMemoryLogExporter) {
        Self::memory_at(LogLevel::Warn)
    }
    #[cfg(any(test, feature = "test-hooks"))]
    pub(crate) fn memory_at(
        level: LogLevel,
    ) -> (Self, opentelemetry_sdk::logs::InMemoryLogExporter) {
        let (sink, logs, _) = Self::memory_signals(level);
        (sink, logs)
    }
    #[cfg(any(test, feature = "test-hooks"))]
    pub(crate) fn memory_signals(
        level: LogLevel,
    ) -> (
        Self,
        opentelemetry_sdk::logs::InMemoryLogExporter,
        opentelemetry_sdk::trace::InMemorySpanExporter,
    ) {
        let logs = opentelemetry_sdk::logs::InMemoryLogExporter::default();
        let traces = opentelemetry_sdk::trace::InMemorySpanExporter::default();
        let sink = Self::from_providers(
            SdkLoggerProvider::builder()
                .with_resource(Resource::builder_empty().build())
                .with_simple_exporter(logs.clone())
                .build(),
            SdkTracerProvider::builder()
                .with_resource(Resource::builder_empty().build())
                .with_sampler(Sampler::AlwaysOn)
                .with_span_limits(SpanLimits::default())
                .with_simple_exporter(traces.clone())
                .build(),
            Warning::default(),
            level,
            false,
        );
        (sink, logs, traces)
    }
    // Existing adaptation diagnostics retain native structured values during migration.
    pub(crate) fn emit(
        &self,
        name: &'static str,
        severity: Severity,
        attributes: Vec<(opentelemetry::Key, AnyValue)>,
        body: AnyValue,
    ) {
        if !self.enabled(severity) {
            return;
        }
        tracing::dispatcher::with_default(&self.0.dispatch, || {
            let mut record = self.0.logger.create_log_record();
            record.set_event_name(name);
            record.set_target("morphiecore.gateway");
            record.set_timestamp(SystemTime::now());
            record.set_severity_number(severity);
            record.set_severity_text(match severity {
                Severity::Debug => "DEBUG",
                Severity::Info => "INFO",
                Severity::Warn => "WARN",
                _ => "ERROR",
            });
            record.add_attributes(attributes);
            record.set_body(body);
            self.0.logger.emit(record);
        });
    }
    pub(crate) fn enabled(&self, severity: Severity) -> bool {
        self.0.level.allows(severity)
    }
    pub(crate) async fn flush(&self) {
        let state = self.0.clone();
        let warning = state.warning.clone();
        if !matches!(
            tokio::task::spawn_blocking(move || {
                let traces = state.traces.force_flush();
                let logs = state.logs.force_flush();
                traces.and(logs)
            })
            .await,
            Ok(Ok(()))
        ) {
            warning.report();
        }
    }
    pub(crate) async fn shutdown(&self) {
        let state = self.0.clone();
        let warning = state.warning.clone();
        if !matches!(
            tokio::task::spawn_blocking(move || {
                let deadline = Instant::now() + Duration::from_secs(5);
                let traces = state
                    .traces
                    .shutdown_with_timeout(deadline.saturating_duration_since(Instant::now()));
                let logs = state
                    .logs
                    .shutdown_with_timeout(deadline.saturating_duration_since(Instant::now()));
                traces.and(logs)
            })
            .await,
            Ok(Ok(()))
        ) {
            warning.report();
        }
    }
}
