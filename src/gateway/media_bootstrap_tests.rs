//! Frozen media activation needs explicit selection and a single non-fallback source.
use super::*;
use crate::credential::{CredentialPool, CredentialRef};
use serde_json::json;

#[test]
fn embeddings_need_explicit_selection_and_a_single_nonfallback_key() {
    for binding in catalog::EMBEDDING_BINDINGS {
        let provider = (binding.provider)();
        let dir = crate::credential::test_support::private_directory();
        let manager = CredentialManager::new(dir.path(), vec![]).unwrap();
        manager
            .add_api_key(
                provider.id.as_str(),
                "one",
                Secret::new("synthetic-embedding-key".into()).unwrap(),
            )
            .unwrap();
        let mut status = manager
            .set_pool(
                provider.id.as_str(),
                binding.credential,
                0,
                CredentialPool {
                    members: vec![CredentialRef::ApiKey {
                        alias: "one".into(),
                    }],
                    fallback: false,
                    max_attempts: 1,
                },
            )
            .unwrap();
        manager.write_gateway_config_for_test(&serde_json::json!({
            "client_key":"synthetic-gateway-key-at-least-32-bytes","models":[binding.model]}));
        let boot = Bootstrap::from_directory(dir.path()).unwrap();
        assert!(boot.gateway.state.entries.is_empty());
        assert_eq!(boot.gateway.state.embeddings.len(), 1);
        assert!(boot.gateway.state.models.models.contains_key(binding.model));
        manager.write_gateway_config_for_test(&serde_json::json!({
            "client_key":"synthetic-gateway-key-at-least-32-bytes"}));
        let boot = Bootstrap::from_directory(dir.path()).unwrap();
        assert!(boot.gateway.state.embeddings.is_empty());
        assert!(!boot.gateway.state.entries.is_empty());
        manager.write_gateway_config_for_test(&serde_json::json!({
            "client_key":"synthetic-gateway-key-at-least-32-bytes","models":[binding.model]}));
        manager
            .add_api_key(
                provider.id.as_str(),
                "two",
                Secret::new("synthetic-second-key".into()).unwrap(),
            )
            .unwrap();
        for (aliases, fallback) in [(&["one", "two"][..], false), (&["one"][..], true)] {
            status = manager
                .set_pool(
                    provider.id.as_str(),
                    binding.credential,
                    status.revision,
                    CredentialPool {
                        members: aliases
                            .iter()
                            .map(|alias| CredentialRef::ApiKey {
                                alias: (*alias).into(),
                            })
                            .collect(),
                        fallback,
                        max_attempts: 1,
                    },
                )
                .unwrap();
            assert!(matches!(
                Bootstrap::from_directory(dir.path()),
                Err(StartupError::Credentials)
            ));
        }
    }
}

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
        (Some(vec!["qwen-audio-3.0-asr-flash"]), 1, true, false),
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
        if !accepted {
            assert!(matches!(result, Err(StartupError::Credentials)));
            continue;
        }
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
