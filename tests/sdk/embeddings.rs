//! Pinned SDK defaults and explicit output encodings through the production Router.
use crate::{child_process, embedding_loop::Fixture};
use morphiecore::protocol::embedding::projection::EncodingSupport;
#[tokio::test]
#[ignore = "requires pinned OpenAI SDK; synthetic float-only/base64-only/both upstreams"]
async fn embeddings_sdk_default_float_and_base64() {
    for support in [
        EncodingSupport::Float,
        EncodingSupport::Base64,
        EncodingSupport::Both,
    ] {
        let fixture = Fixture::start(support, None, None).await;
        let home = tempfile::tempdir().unwrap();
        let mut command = super::isolated_sdk_python(home.path());
        command.args([
            "tests/sdk/embeddings.py",
            &fixture.base,
            if support != EncodingSupport::Base64 {
                "float"
            } else {
                "binary"
            },
        ]);
        let output =
            child_process::run(&mut command, b"", std::time::Duration::from_secs(20), 8192)
                .await
                .unwrap();
        assert!(
            output.status.success(),
            "SDK failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(report["requests"], 3);
        fixture.gateway.flush_diagnostics().await;
        let seen = fixture.seen.lock().unwrap();
        assert_eq!(seen.len(), 3);
        let expected = match support {
            EncodingSupport::Float => ["float", "float", "float"],
            EncodingSupport::Base64 => ["base64", "base64", "base64"],
            EncodingSupport::Both => ["base64", "float", "base64"],
        };
        for (request, encoding) in seen.iter().zip(expected) {
            assert_eq!(request["encoding_format"], encoding);
        }
    }
}

#[tokio::test]
#[ignore = "requires pinned OpenAI SDK; synthetic Google and scoped-token upstreams"]
async fn embeddings_google_reports_and_scoped_token_inputs() {
    for google in [true, false] {
        let fixture = if google {
            Fixture::start_google(None, None).await
        } else {
            Fixture::start(EncodingSupport::Both, None, None).await
        };
        let home = tempfile::tempdir().unwrap();
        let mut command = super::isolated_sdk_python(home.path());
        command.args([
            "tests/sdk/embedding_inputs.py",
            &fixture.base,
            if google { "google" } else { "tokens" },
        ]);
        let output =
            child_process::run(&mut command, b"", std::time::Duration::from_secs(20), 8192)
                .await
                .unwrap();
        assert!(
            output.status.success(),
            "SDK failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(report["requests"], if google { 6 } else { 2 });
        let seen = fixture.seen.lock().unwrap();
        assert_eq!(seen.len(), if google { 6 } else { 2 });
        if google {
            for request in seen.iter() {
                assert!(request.get("user").is_none());
            }
        } else {
            assert_eq!(seen[0]["input"], serde_json::json!([[1, 2, 3]]));
            assert_eq!(seen[1]["input"], serde_json::json!([[1, 2], [3]]));
        }
    }
}
