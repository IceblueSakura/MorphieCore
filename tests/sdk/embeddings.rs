//! Pinned SDK defaults and explicit output encodings through the production Router.
use crate::{child_process, embedding_loop::Fixture};
use morphiecore::lowering::embeddings::EncodingSupport;
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
