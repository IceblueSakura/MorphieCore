//! Pinned SDK tool correction and signed final-history return through one Google Interactions candidate.
use crate::google_tool_fixture as fixture;
use morphiecore::protocol::openai::GenerationApi;
use std::time::Duration;

#[tokio::test]
#[ignore = "requires pinned SDKs; synthetic Interactions JSON/SSE tool correction"]
async fn interactions_tool_correction_and_signed_final_history() {
    for (profile, name, count) in [
        (GenerationApi::Chat, "chat-json", 1),
        (GenerationApi::Chat, "chat-sse", 1),
        (GenerationApi::Chat, "messages-json", 1),
        (GenerationApi::Chat, "messages-sse", 1),
    ] {
        let messages = name.starts_with("messages-");
        let fixture = if messages {
            fixture::start_messages().await
        } else {
            fixture::start(profile, false).await
        };
        let home = tempfile::tempdir().unwrap();
        let mut command = super::isolated_sdk_python(home.path());
        command.args([
            "tests/sdk/google_tool_history.py",
            &fixture.base,
            name,
            "interactions",
        ]);
        let output =
            super::child_process::run(&mut command, b"", Duration::from_secs(30), 64 << 10)
                .await
                .expect("bounded Google SDK history process");
        assert!(
            output.status.success(),
            "{name} SDK failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(report["requests"], count);
        assert_eq!(report["http_requests"], count);
        assert_eq!(report["unrepresentable_signature_rejected"], true);
        assert_eq!(report["upstream"], "interactions");
        assert_eq!(fixture.seen.lock().unwrap().len(), count);
        fixture.gateway.flush_diagnostics().await;
    }
}
