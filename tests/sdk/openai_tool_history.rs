//! Pinned OpenAI SDK through catalog-backed Chat/Responses Routers and synthetic origins.
use crate::openai_tool_loop as fixture;
use morphiecore::protocol::openai::GenerationApi;
use std::time::Duration;

async fn history_case(profile: GenerationApi, recovery_stream: Option<bool>) {
    let (script, requests) = match profile {
        GenerationApi::Chat => ("tests/sdk/chat_tool_history.py", 6),
        GenerationApi::Responses => ("tests/sdk/responses_history.py", 16),
    };
    let requests = if recovery_stream.is_some() {
        4
    } else {
        requests
    };
    let fixture = if recovery_stream.is_some() {
        fixture::start_recovery().await
    } else {
        fixture::start(profile, false).await
    };
    let home = tempfile::tempdir().unwrap();
    let mut command = super::isolated_sdk_python(home.path());
    command.args([script, &fixture.base]);
    if let Some(stream) = recovery_stream {
        command.arg(if stream { "sse" } else { "json" });
    }
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
    assert_eq!(
        report["deliveries"],
        if recovery_stream.is_some() { 1 } else { 2 }
    );
    assert_eq!(report["three_turn_history"], true);
    if profile == GenerationApi::Responses {
        assert_eq!(report["stored_stateless"], true);
    } else {
        assert_eq!(report["http_requests"], requests);
    }
    assert_eq!(
        fixture.seen.lock().unwrap().len(),
        requests + usize::from(recovery_stream.is_some())
    );
    if let Some(recovery) = &fixture.recovery {
        recovery.assert_complete(3, false);
    }
    fixture.gateway.flush_diagnostics().await;
}

#[tokio::test]
#[ignore = "requires pinned OpenAI SDK; synthetic DashScope Chat JSON/SSE tool history"]
async fn chat_tool_result_and_final_history() {
    history_case(GenerationApi::Chat, None).await;
}

#[tokio::test]
#[ignore = "requires pinned OpenAI SDK; opt-in readable reasoning accumulator and history"]
async fn chat_readable_reasoning_tool_history() {
    let fixture = fixture::start_readable().await;
    let home = tempfile::tempdir().unwrap();
    let mut command = super::isolated_sdk_python(home.path());
    command.args(["tests/sdk/chat_tool_history.py", &fixture.base, "reasoning"]);
    let output = super::child_process::run(&mut command, b"", Duration::from_secs(30), 64 << 10)
        .await
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["requests"], 6);
    assert_eq!(report["three_turn_history"], true);
    assert_eq!(fixture.seen.lock().unwrap().len(), 6);
}

#[tokio::test]
#[ignore = "requires pinned OpenAI SDK; synthetic Responses JSON/SSE stored/stateless history"]
async fn responses_stored_and_stateless_history() {
    history_case(GenerationApi::Responses, None).await;
}

#[tokio::test]
#[ignore = "requires pinned OpenAI SDK; synthetic Chat credential migration JSON/SSE"]
async fn chat_sdk_tool_feedback_sticks_after_primary_recovery() {
    for stream in [false, true] {
        history_case(GenerationApi::Chat, Some(stream)).await;
    }
}
