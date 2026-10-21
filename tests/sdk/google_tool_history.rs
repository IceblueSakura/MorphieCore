//! Pinned SDK tool correction and signed final-history return through one Google candidate.
use crate::google_tool_fixture as fixture;
use morphiecore::protocol::openai::Profile;
use std::time::Duration;

async fn exercise(api: fixture::Api, api_name: &str) {
    for (profile, name) in [
        (Profile::Chat, "chat"),
        (Profile::Responses, "responses"),
        (Profile::Chat, "messages"),
    ] {
        let fixture = if name == "messages" {
            fixture::start_messages(api).await
        } else {
            fixture::start(profile, api).await
        };
        let home = tempfile::tempdir().unwrap();
        let mut command = super::isolated_sdk_python(home.path());
        command.args([
            "tests/sdk/google_tool_history.py",
            &fixture.base,
            name,
            api_name,
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
        assert_eq!(report["requests"], 8);
        assert_eq!(report["http_requests"], 8);
        assert_eq!(report["deliveries"], 2);
        assert_eq!(report["tool_correction_and_final_history"], true);
        assert_eq!(report["upstream"], api_name);
        assert_eq!(fixture.seen.lock().unwrap().len(), 8);
        fixture.gateway.flush_diagnostics().await;
    }
}

#[tokio::test]
#[ignore = "requires pinned SDKs; synthetic generateContent JSON/SSE tool correction"]
async fn google_tool_correction_and_signed_final_history() {
    exercise(fixture::Api::GenerateContent, "generate_content").await;
}

#[tokio::test]
#[ignore = "requires pinned SDKs; synthetic Interactions JSON/SSE tool correction"]
async fn interactions_tool_correction_and_signed_final_history() {
    exercise(fixture::Api::Interactions, "interactions").await;
}
