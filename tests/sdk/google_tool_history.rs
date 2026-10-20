//! Pinned SDK tool correction and signed final-history return through one Google candidate.
use crate::google_tool_fixture as fixture;
use morphiecore::protocol::openai::Profile;
use std::time::Duration;

#[tokio::test]
#[ignore = "requires pinned OpenAI SDK; synthetic generateContent JSON/SSE tool correction"]
async fn google_tool_correction_and_signed_final_history() {
    for (profile, name) in [(Profile::Chat, "chat"), (Profile::Responses, "responses")] {
        let fixture = fixture::start(profile).await;
        let home = tempfile::tempdir().unwrap();
        let mut command = tokio::process::Command::new(
            std::env::var_os("MORPHIECORE_SDK_PYTHON").unwrap_or_else(|| "python3".into()),
        );
        command
            .args(["tests/sdk/google_tool_history.py", &fixture.base, name])
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .env_clear()
            .env("HOME", home.path())
            .env("PATH", std::env::var_os("PATH").unwrap_or_default())
            .kill_on_drop(true);
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
        assert_eq!(fixture.seen.lock().unwrap().len(), 8);
        fixture.gateway.flush_diagnostics().await;
    }
}
