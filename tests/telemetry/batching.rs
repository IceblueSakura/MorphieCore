//! A test exporter holds one batch so queue overflow and shutdown cannot race.
use super::{app_filter, isolated_child, resource};
use opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge;
use opentelemetry_sdk::{
    error::OTelSdkResult,
    logs::{BatchConfigBuilder, BatchLogProcessor, LogBatch, LogExporter, SdkLoggerProvider},
};
use std::{
    io::Write,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};
use tracing_subscriber::{Layer as _, layer::SubscriberExt};

#[derive(Debug)]
struct HeldExporter {
    entered: Arc<tokio::sync::Notify>,
    release: Mutex<Option<mpsc::Receiver<()>>>,
    stopped: mpsc::SyncSender<()>,
    exported: Arc<AtomicUsize>,
}
impl LogExporter for HeldExporter {
    async fn export(&self, batch: LogBatch<'_>) -> OTelSdkResult {
        if let Some(release) = self.release.lock().unwrap().take() {
            self.entered.notify_one();
            // Only the SDK export thread waits; even a failed test cannot hold it forever.
            release.recv_timeout(Duration::from_secs(5)).unwrap();
        }
        self.exported
            .fetch_add(batch.iter().count(), Ordering::SeqCst);
        Ok(())
    }
    fn shutdown_with_timeout(&self, _: Duration) -> OTelSdkResult {
        self.stopped.send(()).unwrap();
        Ok(())
    }
}
#[derive(Clone, Default)]
struct Buffer(Arc<Mutex<Vec<u8>>>);
impl Write for Buffer {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[tokio::test]
#[ignore = "environment-cleared ecosystem_checks child"]
async fn full_queue_drops_observably_and_shutdown_timeout_is_not_cancellation() {
    isolated_child();
    let entered = Arc::new(tokio::sync::Notify::new());
    let exported = Arc::new(AtomicUsize::new(0));
    let (release, blocked) = mpsc::sync_channel(1);
    let (stopped, finished) = mpsc::sync_channel(1);
    let provider = SdkLoggerProvider::builder()
        .with_resource(resource())
        .with_log_processor(
            BatchLogProcessor::builder(HeldExporter {
                entered: entered.clone(),
                release: Mutex::new(Some(blocked)),
                stopped,
                exported: exported.clone(),
            })
            .with_batch_config(
                BatchConfigBuilder::default()
                    .with_max_queue_size(2)
                    .with_max_export_batch_size(1)
                    .with_scheduled_delay(Duration::from_secs(60))
                    .build(),
            )
            .build(),
        )
        .build();
    let local = Buffer::default();
    let writer = local.clone();
    let subscriber = tracing_subscriber::registry()
        .with(OpenTelemetryTracingBridge::new(&provider).with_filter(app_filter()))
        .with(
            tracing_subscriber::fmt::layer()
                .json()
                .without_time()
                .with_writer(move || writer.clone())
                // SDK health is local, never fed back into its own export bridge.
                .with_filter(tracing_subscriber::filter::LevelFilter::WARN),
        );
    let dispatch = tracing::Dispatch::new(subscriber);
    tracing::dispatcher::with_default(&dispatch, || {
        tracing::info!(target: "morphiecore.telemetry", ordinal = 0_u64);
    });
    tokio::time::timeout(Duration::from_secs(2), entered.notified())
        .await
        .unwrap();
    let p = provider.clone();
    let outcome = tokio::time::timeout(
        Duration::from_secs(2),
        tokio::task::spawn_blocking(move || {
            tracing::dispatcher::with_default(&dispatch, || {
                for ordinal in 1..=8_u64 {
                    tracing::info!(target: "morphiecore.telemetry", ordinal);
                }
                // Queue saturation must not block emit; timeout bounds the caller only.
                let start = Instant::now();
                let result = p.shutdown_with_timeout(Duration::from_millis(50));
                (result, start.elapsed())
            })
        }),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(outcome.0.is_err());
    assert!(outcome.1 < Duration::from_secs(1));
    assert_eq!(exported.load(Ordering::SeqCst), 0);
    let diagnostics = String::from_utf8(local.0.lock().unwrap().clone()).unwrap();
    // A timed-out shutdown leaves the exporter running. Release before assertions.
    release.send(()).unwrap();
    tokio::task::spawn_blocking(move || finished.recv_timeout(Duration::from_secs(2)).unwrap())
        .await
        .unwrap();
    // The first warning is emitted inside the tracing bridge and may be suppressed
    // by tracing's reentrancy protection. The shutdown summary is independently visible.
    assert!(diagnostics.contains("BatchLogProcessor.Shutdown.Timeout"));
    let records: Vec<serde_json::Value> = diagnostics
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let drop_summary = records
        .iter()
        .find(|record| record["fields"]["dropped_logs_count"] == 6)
        .unwrap();
    assert_eq!(drop_summary["fields"]["max_queue_size"], 2);
    assert_eq!(
        exported.load(Ordering::SeqCst),
        3,
        "one in flight, two queued, six dropped"
    );
    assert!(matches!(
        provider.shutdown(),
        Err(opentelemetry_sdk::error::OTelSdkError::AlreadyShutdown)
    ));
}
