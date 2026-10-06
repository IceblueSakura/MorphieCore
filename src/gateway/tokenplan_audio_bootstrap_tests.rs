//! Explicit activation with isolated synthetic plan credentials; no live account inputs.
use super::*;
use crate::credential::{CredentialPool, CredentialRef};
use serde_json::json;
#[test]
fn tokenplan_audio_requires_explicit_selection_and_nonfallback_single_source() {
    for (models, members, fallback, accepted) in [
        (None, 1, false, true),
        (Some(vec!["qwen-audio-3.0-tts-plus"]), 1, false, true),
        (Some(vec!["qwen-audio-3.0-asr-flash"]), 1, false, true),
        (
            Some(vec!["qwen-audio-3.0-tts-plus", "qwen-audio-3.0-asr-flash"]),
            1,
            false,
            true,
        ),
        (Some(vec!["qwen-audio-3.0-asr-flash"]), 2, false, false),
        (Some(vec!["qwen-audio-3.0-tts-plus"]), 1, true, false),
    ] {
        let directory = crate::credential::test_support::private_directory();
        let manager = CredentialManager::new(directory.path(), vec![]).unwrap();
        let mut config = json!({"client_key":"synthetic-entry-key-at-least-32-bytes"});
        if let Some(models) = &models {
            config["models"] = json!(models);
        }
        manager.write_gateway_config_for_test(&config);
        let mut sources = vec![];
        for index in 0..members {
            let alias = format!("synthetic-{index}");
            manager
                .add_api_key(
                    "aliyun-tokenplan-cn",
                    &alias,
                    Secret::new("synthetic-only".into()).unwrap(),
                )
                .unwrap();
            sources.push(CredentialRef::ApiKey { alias });
        }
        assert!(Bootstrap::from_directory(directory.path()).is_err());
        manager
            .set_pool(
                "aliyun-tokenplan-cn",
                "aliyun-tokenplan-cn-api-key",
                0,
                CredentialPool {
                    members: sources,
                    fallback,
                    max_attempts: 1,
                },
            )
            .unwrap();
        let result = Bootstrap::from_directory(directory.path());
        assert_eq!(result.is_ok(), accepted, "selection {models:?}");
        if let Ok(boot) = result {
            let value: serde_json::Value =
                serde_json::from_slice(&boot.gateway.state.models.list).unwrap();
            let labels = value["data"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v["id"].as_str().unwrap())
                .collect::<Vec<_>>();
            for model in ["qwen-audio-3.0-tts-plus", "qwen-audio-3.0-asr-flash"] {
                assert_eq!(
                    labels.contains(&model),
                    models.as_ref().is_some_and(|m| m.contains(&model))
                );
            }
        }
    }
}
