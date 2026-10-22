//! Frozen media activation needs explicit selection and a single non-fallback source.
use super::test_support::{selected_models, write_configuration};
use super::*;
use crate::credential::{CredentialPool, CredentialRef};
use serde_json::json;

#[test]
fn embeddings_need_explicit_selection_and_a_single_nonfallback_key() {
    for binding in catalog::EMBEDDING_BINDINGS {
        let provider = (binding.provider)();
        let dir = crate::credential::tests::support::private_directory();
        let manager = CredentialManager::new(dir.path().join("credentials"), vec![]).unwrap();
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
                binding.credential().as_str(),
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
        let service = serde_json::json!({"client_key":"synthetic-gateway-key-at-least-32-bytes"});
        write_configuration(
            dir.path(),
            service.clone(),
            selected_models(&[binding.model], &[provider.id.as_str()]),
        );
        let boot = Bootstrap::from_directory(dir.path()).unwrap();
        assert!(boot.gateway.state.entries.is_empty());
        assert_eq!(boot.gateway.state.embeddings.len(), 1);
        assert!(boot.gateway.state.models.models.contains_key(binding.model));
        let snapshot: serde_json::Value =
            serde_json::from_slice(&boot.gateway.state.capabilities.0).unwrap();
        assert_eq!(snapshot["data"].as_array().unwrap().len(), 1);
        assert_eq!(snapshot["data"][0]["model"], binding.model);
        assert_eq!(snapshot["data"][0]["protocol"], "embeddings");
        assert_eq!(
            snapshot["data"][0]["embedding"]["default_dimensions"],
            binding.default_dimensions
        );
        assert_eq!(
            snapshot["data"][0]["embedding"]["output_encodings"],
            json!(["float", "base64"])
        );
        let public = snapshot.to_string();
        assert!(!public.contains(provider.origin.as_str()));
        assert!(!public.contains(binding.credential().as_str()));
        assert!(!public.contains("gemini-embedding-2"));
        if let Some(text) = catalog::API_KEY_BINDINGS.iter().find(|b| {
            b.credential() == binding.credential() && !b.public_model().contract.audio_input
        }) {
            write_configuration(
                dir.path(),
                service.clone(),
                selected_models(&[text.model], &[provider.id.as_str()]),
            );
            let boot = Bootstrap::from_directory(dir.path()).unwrap();
            assert!(boot.gateway.state.embeddings.is_empty());
            assert!(!boot.gateway.state.entries.is_empty());
            let snapshot: serde_json::Value =
                serde_json::from_slice(&boot.gateway.state.capabilities.0).unwrap();
            assert!(
                snapshot["data"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|v| v["protocol"] != "embeddings")
            );
        }
        write_configuration(
            dir.path(),
            service,
            selected_models(&[binding.model], &[provider.id.as_str()]),
        );
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
                    binding.credential().as_str(),
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
        let directory = crate::credential::tests::support::private_directory();
        let manager = CredentialManager::new(directory.path().join("credentials"), vec![]).unwrap();
        let text = catalog::API_KEY_BINDINGS
            .iter()
            .find(|b| b.credential().as_str() == "aliyun-tokenplan-cn-api-key")
            .unwrap();
        write_configuration(
            directory.path(),
            json!({"client_key":"synthetic-entry-key-at-least-32-bytes"}),
            selected_models(
                models.as_deref().unwrap_or(&[text.model]),
                &["aliyun-tokenplan-cn"],
            ),
        );
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
        let dir = crate::credential::tests::support::private_directory();
        let manager = CredentialManager::new(dir.path().join("credentials"), vec![]).unwrap();
        let text = catalog::API_KEY_BINDINGS
            .iter()
            .find(|b| b.credential().as_str() == "openrouter-api-key")
            .unwrap();
        write_configuration(
            dir.path(),
            json!({"client_key":"synthetic-gateway-key-at-least-32-bytes"}),
            selected_models(
                selection.as_deref().unwrap_or(&[text.model]),
                &["openrouter"],
            ),
        );
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
