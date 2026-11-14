//! Native failure logs. The SDK owns batching; the receiver owns persistence.
use opentelemetry::{
    KeyValue,
    logs::{AnyValue, LogRecord as _, Logger as _, LoggerProvider as _, Severity},
};
use opentelemetry_otlp::{Protocol, WithExportConfig, WithHttpConfig};
use opentelemetry_sdk::{
    Resource,
    error::OTelSdkResult,
    logs::{
        BatchConfigBuilder, BatchLogProcessor, LogBatch, LogExporter, SdkLogger, SdkLoggerProvider,
    },
};
use serde::{Deserialize, Serialize};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, SystemTime},
};
use tracing::instrument::WithSubscriber;
use tracing_subscriber::{Layer, layer::SubscriberExt};

/// Trusted deployment destination; no business input participates in this configuration.
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DiagnosticsConfig {
    /// Complete OTLP/HTTP Logs destination URL (typically `/v1/logs`).
    pub endpoint: String,
    /// Generation timeline verbosity; failure-only by default.
    #[serde(default)]
    pub level: LogLevel,
    /// Explicit opt-in for sanitized business payload snippets on failures.
    #[serde(default)]
    pub private_bodies: bool,
    /// Shared retained-body budget across the four HTTP boundaries.
    #[serde(default = "body_bytes")]
    pub body_bytes: usize,
}
#[derive(Clone, Copy, Default, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Error,
    #[default]
    Warn,
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
        // The exporter merges ambient headers even when with_headers is empty.
        // Collector authentication is not an alternate credential-loading path.
        if [
            "OTEL_EXPORTER_OTLP_HEADERS",
            "OTEL_EXPORTER_OTLP_LOGS_HEADERS",
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
struct Warning(Arc<AtomicBool>);
impl<S: tracing::Subscriber> Layer<S> for Warning {
    fn on_event(&self, event: &tracing::Event<'_>, _: tracing_subscriber::layer::Context<'_, S>) {
        let metadata = event.metadata();
        if metadata.target().starts_with("opentelemetry")
            && *metadata.level() <= tracing::Level::WARN
        {
            self.0.store(true, Ordering::Relaxed);
        }
    }
}
#[derive(Debug)]
struct Exporter {
    inner: opentelemetry_otlp::LogExporter,
    warning: Arc<AtomicBool>,
    dispatch: tracing::Dispatch,
}
impl LogExporter for Exporter {
    async fn export(&self, batch: LogBatch<'_>) -> OTelSdkResult {
        let result = self
            .inner
            .export(batch)
            .with_subscriber(self.dispatch.clone())
            .await;
        if self.warning.swap(false, Ordering::Relaxed) {
            eprintln!("MorphieCore telemetry queue warning; records may be lost");
        }
        if result.is_err() {
            eprintln!("MorphieCore telemetry export failed; records may be lost");
            // SDK diagnostics must not echo a Collector's response data.
            return Err(opentelemetry_sdk::error::OTelSdkError::InternalFailure(
                "OTLP log export failed".into(),
            ));
        }
        result
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
    provider: SdkLoggerProvider,
    warnings: tracing::Dispatch,
    level: LogLevel,
}
impl Drop for Telemetry {
    fn drop(&mut self) {
        // Explicit flush owns draining; a last request/body owner must never wait on export.
        tracing::dispatcher::with_default(&self.warnings, || {
            let _ = self.provider.shutdown_with_timeout(Duration::ZERO);
        });
    }
}
#[derive(Clone)]
pub(in crate::gateway) struct Sink(Arc<Telemetry>);
impl Sink {
    pub(in crate::gateway) fn open(config: &DiagnosticsConfig) -> std::io::Result<Self> {
        config.validate()?;
        let endpoint = config.endpoint.clone();
        let level = config.level;
        // reqwest's blocking client must be constructed outside a Tokio runtime.
        std::thread::spawn(move || {
            tracing::dispatcher::with_default(&tracing::Dispatch::none(), || {
                let client = reqwest::blocking::Client::builder()
                    .no_proxy()
                    .redirect(reqwest::redirect::Policy::none())
                    .timeout(Duration::from_secs(2))
                    .build()
                    .map_err(|_| std::io::Error::other("diagnostics HTTP client"))?;
                let exporter = opentelemetry_otlp::LogExporter::builder()
                    .with_http()
                    .with_endpoint(endpoint)
                    .with_protocol(Protocol::HttpJson)
                    .with_timeout(Duration::from_secs(2))
                    .with_http_client(client)
                    .with_retry_policy(
                        opentelemetry_otlp::RetryPolicy::default().with_max_retries(0),
                    )
                    .with_max_request_body_size(1 << 20)
                    .build()
                    .map_err(|_| std::io::Error::other("diagnostics exporter"))?;
                let warning = Arc::new(AtomicBool::new(false));
                let dispatch = tracing::Dispatch::new(
                    tracing_subscriber::registry().with(Warning(warning.clone())),
                );
                let processor = BatchLogProcessor::builder(Exporter {
                    inner: exporter,
                    warning: warning.clone(),
                    dispatch,
                })
                .with_batch_config(
                    BatchConfigBuilder::default()
                        .with_max_queue_size(64)
                        .with_max_export_batch_size(4)
                        .with_scheduled_delay(Duration::from_millis(100))
                        .build(),
                )
                .build();
                Ok(Self::from_provider(
                    SdkLoggerProvider::builder()
                        .with_resource(
                            Resource::builder_empty()
                                .with_service_name("morphiecore")
                                .with_attribute(KeyValue::new(
                                    "service.version",
                                    env!("CARGO_PKG_VERSION"),
                                ))
                                .with_attribute(KeyValue::new(
                                    "service.revision",
                                    option_env!("MORPHIECORE_REVISION").unwrap_or("unknown"),
                                ))
                                .build(),
                        )
                        .with_log_processor(processor)
                        .build(),
                    warning,
                    level,
                ))
            })
        })
        .join()
        .map_err(|_| std::io::Error::other("diagnostics initialization"))?
    }
    fn from_provider(
        provider: SdkLoggerProvider,
        warning: Arc<AtomicBool>,
        level: LogLevel,
    ) -> Self {
        let logger = provider.logger("morphiecore.gateway");
        Self(Arc::new(Telemetry {
            logger,
            provider,
            warnings: tracing::Dispatch::new(tracing_subscriber::registry().with(Warning(warning))),
            level,
        }))
    }
    #[cfg(test)]
    pub(in crate::gateway) fn memory() -> (Self, opentelemetry_sdk::logs::InMemoryLogExporter) {
        Self::memory_at(LogLevel::Warn)
    }
    #[cfg(test)]
    pub(in crate::gateway) fn memory_at(
        level: LogLevel,
    ) -> (Self, opentelemetry_sdk::logs::InMemoryLogExporter) {
        let exporter = opentelemetry_sdk::logs::InMemoryLogExporter::default();
        (
            Self::from_provider(
                SdkLoggerProvider::builder()
                    .with_resource(Resource::builder_empty().build())
                    .with_simple_exporter(exporter.clone())
                    .build(),
                Arc::new(AtomicBool::new(false)),
                level,
            ),
            exporter,
        )
    }
    pub(in crate::gateway) fn emit(
        &self,
        name: &'static str,
        severity: Severity,
        attributes: Vec<(opentelemetry::Key, AnyValue)>,
        body: AnyValue,
    ) {
        if !self.enabled(severity) {
            return;
        }
        tracing::dispatcher::with_default(&self.0.warnings, || {
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
    pub(in crate::gateway) fn enabled(&self, severity: Severity) -> bool {
        self.0.level.allows(severity)
    }
    pub(in crate::gateway) async fn flush(&self) {
        let provider = self.0.provider.clone();
        if !matches!(
            tokio::task::spawn_blocking(move || provider.force_flush()).await,
            Ok(Ok(()))
        ) {
            eprintln!("MorphieCore telemetry flush incomplete; records may be lost");
        }
    }
}
