//! Test-only ecosystem checks, not production telemetry or a diagnostic wire schema.
#[path = "telemetry/batching.rs"]
mod batching;
#[path = "support/child_process.rs"]
mod child_process;
#[path = "telemetry/otlp.rs"]
mod otlp;
#[path = "telemetry/signals.rs"]
mod signals;

use std::time::Duration;
use tracing_subscriber::filter::Targets;

const TARGET: &str = "morphiecore.telemetry";

fn app_filter() -> Targets {
    Targets::new().with_target(TARGET, tracing::Level::TRACE)
}

fn resource() -> opentelemetry_sdk::Resource {
    opentelemetry_sdk::Resource::builder_empty()
        .with_service_name("morphiecore-telemetry-test")
        .build()
}

fn isolated_child() {
    assert_eq!(
        std::env::var("MORPHIECORE_TELEMETRY_TEST").as_deref(),
        Ok("isolated"),
        "run this target through ecosystem_checks, not directly with --ignored"
    );
}

#[tokio::test]
async fn ecosystem_checks() {
    let mut command = tokio::process::Command::new(std::env::current_exe().unwrap());
    command
        .args(["--ignored", "--test-threads=1", "--nocapture"])
        .env_clear()
        .env("MORPHIECORE_TELEMETRY_TEST", "isolated")
        // Only synthetic configuration reaches the SDK; never inherit host OTEL_*.
        .env(
            "OTEL_EXPORTER_OTLP_LOGS_ENDPOINT",
            "http://127.0.0.1:1/unused",
        )
        .env("OTEL_EXPORTER_OTLP_HEADERS", "x-environment-only=synthetic");
    let output = child_process::run(&mut command, b"", Duration::from_secs(45), 64 << 10)
        .await
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("4 passed; 0 failed"), "{stdout}");
}
