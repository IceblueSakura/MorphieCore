//! Pinned SDK traverses the production native Router with the selected R2 profile.
#[path = "../support/r2.rs"]
mod fixture;
use super::child_process;
use serde_json::Value;
use std::time::Duration;
use tokio::process::Command;

#[tokio::test]
#[ignore = "requires locked Anthropic SDK; synthetic native R2 JSON/SSE five-turn loops"]
async fn selected_r2_sdk_keeps_changed_tools_effort_and_replay() {
    let fixture = fixture::start().await;
    let home = tempfile::tempdir().unwrap();
    let mut command = Command::new(
        std::env::var_os("MORPHIECORE_SDK_PYTHON").unwrap_or_else(|| "python3".into()),
    );
    command
        .env_clear()
        .env("HOME", home.path())
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .args(["tests/sdk/r2_loop.py", &fixture.base, fixture::KEY])
        .current_dir(env!("CARGO_MANIFEST_DIR"));
    let input = serde_json::to_vec(&fixture::input(false)).unwrap();
    let output = child_process::run(&mut command, &input, Duration::from_secs(30), 16 << 10)
        .await
        .unwrap();
    assert!(
        output.status.success(),
        "synthetic R2 SDK failure: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        report,
        serde_json::json!({"requests":10,"tool_changes":true,"ttl":"1h"})
    );
    let seen = fixture.seen.lock().unwrap();
    assert_eq!(seen.len(), 10);
    for (static_body, streamed_body) in seen[..5].iter().zip(&seen[5..]) {
        let mut static_body = static_body.clone();
        static_body["stream"] = serde_json::json!(true);
        assert_eq!(&static_body, streamed_body);
    }
}
