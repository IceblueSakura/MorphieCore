//! Pinned OpenAI SDK through catalog-backed Chat/Responses Routers and synthetic origins.
use crate::openai_tool_loop as fixture;
use morphiecore::protocol::openai::Profile;
use std::time::Duration;

async fn history_case(profile: Profile) {
    let (script, requests) = match profile {
        Profile::Chat => ("tests/sdk/chat_tool_history.py", 6),
        Profile::Responses => ("tests/sdk/responses_history.py", 12),
    };
    let fixture = fixture::start(profile, false).await;
    let home = tempfile::tempdir().unwrap();
    let mut command = tokio::process::Command::new(
        std::env::var_os("MORPHIECORE_SDK_PYTHON").unwrap_or_else(|| "python3".into()),
    );
    command
        .args([script, &fixture.base])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .env_clear()
        .env("HOME", home.path())
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .kill_on_drop(true);
    let output = super::child_process::run(&mut command, b"", Duration::from_secs(30), 64 << 10)
        .await
        .expect("bounded SDK tool-history process");
    assert!(
        output.status.success(),
        "{profile:?} SDK failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["requests"], requests);
    assert_eq!(report["protocols"], 1);
    assert_eq!(report["deliveries"], 2);
    assert_eq!(report["three_turn_history"], true);
    if profile == Profile::Responses {
        assert_eq!(report["stored_stateless"], true);
    }
    assert_eq!(fixture.seen.lock().unwrap().len(), requests);
    fixture.gateway.flush_diagnostics().await;
}

#[tokio::test]
#[ignore = "requires pinned OpenAI SDK; synthetic DashScope Chat JSON/SSE tool history"]
async fn chat_tool_result_and_reasoning_history() {
    history_case(Profile::Chat).await;
}

#[tokio::test]
#[ignore = "requires pinned OpenAI SDK; synthetic Responses JSON/SSE stored/stateless history"]
async fn responses_stored_and_stateless_history() {
    history_case(Profile::Responses).await;
}
