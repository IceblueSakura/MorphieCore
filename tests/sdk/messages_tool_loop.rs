//! Pinned SDK traverses the production native Router with the selected native Messages profile.
use super::child_process;
use crate::messages_tool_fixture as fixture;
use serde_json::Value;
use std::time::Duration;

#[tokio::test]
#[ignore = "requires locked Anthropic SDK; synthetic native Messages JSON/SSE five-turn loops"]
async fn native_response_sdk_keeps_changed_tools_effort_and_replay() {
    history_case(None).await;
}

#[tokio::test]
#[ignore = "requires locked Anthropic SDK; synthetic native credential migration JSON/SSE"]
async fn native_response_sdk_tool_feedback_sticks_after_primary_recovery() {
    for stream in [false, true] {
        history_case(Some(stream)).await;
    }
}

async fn history_case(recovery_stream: Option<bool>) {
    let fixture = if recovery_stream.is_some() {
        fixture::start_recovery().await
    } else {
        fixture::start().await
    };
    let home = tempfile::tempdir().unwrap();
    let mut command = super::isolated_sdk_python(home.path());
    command.args([
        "tests/sdk/messages_tool_loop.py",
        &fixture.base,
        fixture::KEY,
    ]);
    if let Some(stream) = recovery_stream {
        command.arg(if stream { "sse" } else { "json" });
    }
    let input = serde_json::to_vec(&fixture::input(false)).unwrap();
    let output = child_process::run(&mut command, &input, Duration::from_secs(30), 16 << 10)
        .await
        .unwrap();
    assert!(
        output.status.success(),
        "synthetic Messages SDK failure: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    let requests = if recovery_stream.is_some() { 6 } else { 10 };
    assert_eq!(
        report,
        serde_json::json!({"requests":requests,"http_requests":requests,"tool_changes":true,"ttl":"1h"})
    );
    let seen = fixture.seen.lock().unwrap();
    assert_eq!(
        seen.len(),
        requests + usize::from(recovery_stream.is_some())
    );
    if let Some(recovery) = &fixture.recovery {
        recovery.assert_complete(5, true);
    } else {
        for (static_body, streamed_body) in seen[..5].iter().zip(&seen[5..]) {
            let mut static_body = static_body.clone();
            static_body["stream"] = serde_json::json!(true);
            assert_eq!(&static_body, streamed_body);
        }
    }
}
