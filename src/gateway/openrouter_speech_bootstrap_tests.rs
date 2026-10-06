//! Synthetic startup files only; no inference or credential discovery outside this fixture.
use super::*;
use crate::credential::{CredentialPool, CredentialRef};
use serde_json::json;

#[test]
fn openrouter_speech_requires_explicit_selection_and_single_nonfallback_pool() {
    for (selection, members, fallback, accepted) in [
        (None, 1, false, true),
        (Some(vec!["qwen-audio-3.0-tts-flash"]), 1, false, true),
        (
            Some(vec!["qwen-audio-3.0-tts-flash", "gpt-image-2.5-flare"]),
            1,
            false,
            true,
        ),
        (Some(vec!["qwen-audio-3.0-tts-flash"]), 1, true, false),
        (Some(vec!["qwen-audio-3.0-tts-flash"]), 2, false, false),
    ] {
        let dir = crate::credential::test_support::private_directory();
        let manager = CredentialManager::new(dir.path(), vec![]).unwrap();
        let mut config = json!({"client_key":"synthetic-gateway-key-at-least-32-bytes"});
        if let Some(models) = &selection {
            config["models"] = json!(models);
        }
        manager.write_gateway_config_for_test(&config);
        let mut pool_members = Vec::new();
        for index in 0..members {
            let alias = format!("source-{index}");
            manager
                .add_api_key(
                    "openrouter",
                    &alias,
                    Secret::new(format!("synthetic-key-{index}")).unwrap(),
                )
                .unwrap();
            pool_members.push(CredentialRef::ApiKey { alias });
        }
        assert!(
            Bootstrap::from_directory(dir.path()).is_err(),
            "keys without a pool do not activate inference"
        );
        manager
            .set_pool(
                "openrouter",
                "openrouter-api-key",
                0,
                CredentialPool {
                    members: pool_members,
                    fallback,
                    max_attempts: 1,
                },
            )
            .unwrap();
        let result = Bootstrap::from_directory(dir.path());
        assert_eq!(result.is_ok(), accepted);
        if let Ok(boot) = result {
            if let Some(models) = selection {
                assert_eq!(boot.gateway.state.speech.len(), 1);
                assert!(boot.gateway.state.entries.is_empty());
                assert_eq!(boot.gateway.state.images.len(), models.len() - 1);
                let view: serde_json::Value =
                    serde_json::from_slice(&boot.gateway.state.models.list).unwrap();
                assert!(view["data"].as_array().unwrap().contains(&json!({
                    "id":"qwen-audio-3.0-tts-flash","object":"model","created":1784592000,"owned_by":"Alibaba"
                })));
            } else {
                assert!(
                    boot.gateway.state.speech.is_empty(),
                    "conversation defaults must not enable TTS"
                );
                assert!(boot.gateway.state.images.is_empty());
                assert!(!boot.gateway.state.entries.is_empty());
            }
        }
    }
}
