//! Pinned SDK through the actual selected R1 bindings and synthetic upstreams.
#[path = "../support/r1.rs"]
mod fixture;
use std::time::Duration;

#[tokio::test]
#[ignore = "requires pinned Python SDK; selected bindings and synthetic Providers only"]
async fn selected_r1_sdk_three_turn_json_sse_and_stored_history() {
    let fixture = fixture::start().await;
    let home = tempfile::tempdir().unwrap();
    let mut command = tokio::process::Command::new(
        std::env::var_os("MORPHIECORE_SDK_PYTHON").unwrap_or_else(|| "python3".into()),
    );
    command
        .args(["tests/sdk/r1_text_loop.py", &fixture.base])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .env_clear()
        .env("HOME", home.path())
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .kill_on_drop(true);
    let output = super::child_process::run(&mut command, b"", Duration::from_secs(30), 64 << 10)
        .await
        .expect("bounded R1 SDK process");
    assert!(
        output.status.success(),
        "R1 SDK failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["requests"], 18);
    assert_eq!(report["three_turn_history"], true);
    assert_eq!(fixture.seen.lock().unwrap().len(), 18);
    fixture.gateway.flush_diagnostics().await;
}
