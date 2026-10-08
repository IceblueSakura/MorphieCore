//! Independent activation/target assertions over synthetic file configuration.
use super::*;
#[test]
fn openrouter_key_enables_only_its_fixed_model_and_rejects_empty_credentials() {
    let settings = |name: &str| {
        Ok(match name {
            "client_key" => Some("synthetic-gateway-client-token-0001".into()),
            "openrouter-api-key" => Some("synthetic-openrouter-only".into()),
            _ => None,
        })
    };
    let boot = Fixture::from_lookup(settings).unwrap();
    for profile in [Profile::Chat, Profile::Responses] {
        let entry =
            &boot.gateway.state.entries[&(super::super::family(profile), "gpt-6-luna".into())];
        let candidate = &entry.candidates[0];
        assert_eq!(candidate.endpoint.upstream_model, "openai/gpt-6-luna");
        assert_eq!(candidate.provider.origin.as_str(), "https://openrouter.ai");
        assert_eq!(candidate.endpoint.credential.as_str(), "openrouter-api-key");
    }
    assert!(
        boot.gateway
            .state
            .entries
            .keys()
            .all(|(_, model)| model == "gpt-6-luna")
    );
    assert!(matches!(
        Fixture::from_lookup(|name| {
            if name == "openrouter-api-key" {
                Ok(Some(String::new()))
            } else {
                settings(name)
            }
        }),
        Err(StartupError::Credentials)
    ));
}

#[test]
fn opencode_go_key_only_activates_native_hy4_chat() {
    let boot = Fixture::from_lookup(|name| {
        Ok(match name {
            "client_key" => Some("synthetic-gateway-client-token-0001".into()),
            "opencode-go-api-key" => Some("synthetic-go-key".into()),
            _ => None,
        })
    })
    .unwrap();
    let (_, request) = super::super::admission::prepare(
            &boot.gateway.state, Profile::Chat,
            br#"{"model":"hy4-preview","session_id":"synthetic-conversation","messages":[{"role":"user","content":"hi"}]}"#,
        ).unwrap();
    assert_eq!(
        request.task.semantic.controls().max_output_tokens,
        Some(1024)
    );
    let entry =
        &boot.gateway.state.entries[&(super::super::family(Profile::Chat), "hy4-preview".into())];
    assert_eq!(entry.public.route.as_str(), "opencode-go-generation");
    let candidate = &entry.candidates[0];
    assert_eq!(candidate.endpoint.id.as_str(), "opencode-go-chat");
    assert_eq!(candidate.provider.id.as_str(), "opencode-go");
    assert_eq!(boot.gateway.state.entries.len(), 1);
    assert!(candidate.provider.responses.is_none());
    let prepared = crate::execution::prepare(
        &candidate.endpoint,
        &candidate.provider,
        &candidate.secret.resolve().unwrap().0,
        &request,
    )
    .unwrap();
    assert_eq!(prepared.origin, "https://opencode.ai");
    assert_eq!(prepared.path, "/zen/go/v1/chat/completions");
    assert_eq!(prepared.auth_header.1, "Bearer synthetic-go-key");
    let wire: serde_json::Value = serde_json::from_slice(&prepared.body).unwrap();
    assert_eq!(wire["model"], "hy4-preview");
    assert_eq!(wire["max_tokens"], 1024);
    for (profile, body) in [
        (
            Profile::Responses,
            br#"{"model":"hy4-preview","input":"hi"}"#.as_slice(),
        ),
        (Profile::Chat, br#"{"model":"gpt-6-luna-go","messages":[]}"#),
        (
            Profile::Responses,
            br#"{"model":"gpt-6-luna-go","input":"hi"}"#,
        ),
        (
            Profile::Chat,
            br#"{"model":"opencode-go/hy4-preview","messages":[]}"#,
        ),
        (
            Profile::Chat,
            br#"{"model":"deepseek-flash","messages":[]}"#,
        ),
    ] {
        assert_eq!(
            super::super::admission::prepare(&boot.gateway.state, profile, body)
                .err()
                .unwrap()
                .status,
            404
        );
    }
}

#[test]
fn shared_provider_credentials_keep_model_and_protocol_targets_distinct() {
    let boot = Fixture::from_lookup(|name| {
        Ok(match name {
            "client_key" => Some("synthetic-gateway-client-token-0001".into()),
            "xiaomi-api-key" | "zhipu-api-key" => Some("synthetic-upstream".into()),
            _ => None,
        })
    })
    .unwrap();
    for (model, upstream, credential, chat_prefix, responses_prefix) in [
        (
            "mimo-v2.6-pro",
            "mimo-v2.6-pro",
            "xiaomi-api-key",
            "/v1",
            "/v1",
        ),
        (
            "mimo-v2.6-flash",
            "mimo-v2.6-flash",
            "xiaomi-api-key",
            "/v1",
            "/v1",
        ),
        (
            "glm-5.3",
            "glm-5.3",
            "zhipu-api-key",
            "/api/paas/v4",
            "/api/v1",
        ),
    ] {
        for protocol in [Profile::Chat, Profile::Responses] {
            let entry =
                &boot.gateway.state.entries[&(super::super::family(protocol), model.into())];
            let candidate = &entry.candidates[0];
            assert_eq!(candidate.endpoint.upstream_model, upstream);
            assert_eq!(candidate.endpoint.credential.as_str(), credential);
            assert_eq!(
                candidate.endpoint.target.path.as_str(),
                format!(
                    "{}/{}",
                    if protocol == Profile::Chat {
                        chat_prefix
                    } else {
                        responses_prefix
                    },
                    if protocol == Profile::Chat {
                        "chat/completions"
                    } else {
                        "responses"
                    }
                )
            );
            let request = if protocol == Profile::Chat {
                serde_json::json!({"model":model,"messages":[{"role":"user","content":"hi"}]})
            } else {
                serde_json::json!({"model":model,"input":"hi"})
            };
            assert!(
                super::super::admission::prepare(
                    &boot.gateway.state,
                    protocol,
                    &serde_json::to_vec(&request).unwrap()
                )
                .is_ok()
            );
        }
    }
}

#[test]
fn subscription_and_metered_entries_never_exchange_credentials_or_origins() {
    for (subscription, metered) in [(true, false), (false, true), (true, true)] {
        let boot = Fixture::from_lookup(|name| {
            Ok(match name {
                "client_key" => Some("synthetic-gateway-client-token-0001".into()),
                "aliyun-tokenplan-cn-api-key" if subscription => {
                    Some("synthetic-subscription-key".into())
                }
                "aliyun-dashscope-cn-api-key" if metered => Some("synthetic-metered-key".into()),
                _ => None,
            })
        })
        .unwrap();
        for profile in [Profile::Chat, Profile::Responses] {
            for (model, enabled, origin, secret) in [
                (
                    "qwen3.8-flash",
                    subscription,
                    "https://token-plan.cn-beijing.maas.aliyuncs.com",
                    "synthetic-subscription-key",
                ),
                (
                    "qwen3.8-max",
                    metered,
                    "https://dashscope.aliyuncs.com",
                    "synthetic-metered-key",
                ),
            ] {
                let value = if profile == Profile::Chat {
                    serde_json::json!({"model":model,"messages":[{"role":"user","content":"hi"}],"max_completion_tokens":32})
                } else {
                    serde_json::json!({"model":model,"input":"hi","max_output_tokens":32})
                };
                let admitted = super::super::admission::prepare(
                    &boot.gateway.state,
                    profile,
                    &serde_json::to_vec(&value).unwrap(),
                );
                if !enabled {
                    assert!(admitted.is_err());
                    continue;
                }
                let (entry, request) = admitted.unwrap();
                let candidate = &entry.candidates[0];
                let prepared = crate::execution::prepare(
                    &candidate.endpoint,
                    &candidate.provider,
                    &candidate.secret.resolve().unwrap().0,
                    &request,
                )
                .unwrap();
                assert_eq!(prepared.origin, origin);
                assert_eq!(
                    prepared.auth_header,
                    ("authorization".into(), format!("Bearer {secret}"))
                );
                assert_eq!(
                    prepared.path,
                    if profile == Profile::Chat {
                        "/compatible-mode/v1/chat/completions"
                    } else {
                        "/compatible-mode/v1/responses"
                    }
                );
                let wire: serde_json::Value = serde_json::from_slice(&prepared.body).unwrap();
                assert_eq!(wire["model"], model);
                assert_eq!(
                    wire[if profile == Profile::Chat {
                        "max_tokens"
                    } else {
                        "max_output_tokens"
                    }],
                    32
                );
            }
        }
        if subscription && metered {
            let subscription_entry = &boot.gateway.state.entries
                [&(super::super::family(Profile::Chat), "qwen3.8-flash".into())];
            let metered_entry = &boot.gateway.state.entries
                [&(super::super::family(Profile::Chat), "qwen3.8-max".into())];
            assert_ne!(
                subscription_entry.candidates[0]
                    .endpoint
                    .representation
                    .adaptation
                    .scope,
                metered_entry.candidates[0]
                    .endpoint
                    .representation
                    .adaptation
                    .scope
            );
        }
    }
}

#[test]
fn bootstrap_only_admits_explicit_protocol_entries() {
    let boot = Fixture::from_lookup(|name| {
        Ok(match name {
            "client_key" => Some("synthetic-gateway-client-token-0001".into()),
            "nvidia-api-key" | "modelbest-api-key" => Some("synthetic-upstream".into()),
            _ => None,
        })
    })
    .unwrap();
    assert!(
        super::super::admission::prepare(
            &boot.gateway.state,
            Profile::Chat,
            br#"{"model":"nemotron-3-super","messages":[{"role":"user","content":"hi"}]}"#
        )
        .is_ok()
    );
    assert!(
        super::super::admission::prepare(
            &boot.gateway.state,
            Profile::Responses,
            br#"{"model":"nemotron-3-super","input":"hi"}"#
        )
        .is_ok()
    );
    assert!(
        super::super::admission::prepare(
            &boot.gateway.state,
            Profile::Chat,
            br#"{"model":"minicpm5-2b","messages":[{"role":"user","content":"hi"}]}"#
        )
        .is_ok()
    );
    assert!(
        super::super::admission::prepare(
            &boot.gateway.state,
            Profile::Responses,
            br#"{"model":"minicpm5-2b","input":"hi"}"#
        )
        .is_err()
    );
}

#[test]
fn bootstrap_is_loopback_and_requires_explicit_auth_and_provider_keys() {
    let settings = |name: &str| {
        Ok(match name {
            "client_key" => Some("synthetic-gateway-client-token-0001".into()),
            "deepseek-api-key" => Some("synthetic-upstream-only".into()),
            _ => None,
        })
    };
    assert!(Fixture::from_lookup(settings).is_ok());
    assert!(matches!(
        Fixture::from_lookup(|_| Ok(None)),
        Err(StartupError::Credentials)
    ));
    assert!(matches!(
        Fixture::from_lookup(|n| if n == "bind" {
            Ok(Some("0.0.0.0:8080".into()))
        } else {
            settings(n)
        }),
        Err(StartupError::Listener)
    ));
    assert!(
        Fixture::from_lookup(|n| if n == "deepseek-api-key" {
            Ok(None)
        } else {
            settings(n)
        })
        .is_err()
    );
}

struct Fixture {
    boot: Bootstrap,
    _directory: tempfile::TempDir,
}
impl std::ops::Deref for Fixture {
    type Target = Bootstrap;
    fn deref(&self) -> &Bootstrap {
        &self.boot
    }
}
impl Fixture {
    fn from_lookup(
        mut get: impl FnMut(&str) -> Result<Option<String>, StartupError>,
    ) -> Result<Self, StartupError> {
        let directory = crate::credential::test_support::private_directory();
        let manager = crate::credential::CredentialManager::new(directory.path(), vec![]).unwrap();
        let mut added = std::collections::BTreeSet::new();
        for binding in catalog::API_KEY_BINDINGS {
            if let Some(key) = get(binding.credential)? {
                if !added.insert(binding.credential) {
                    continue;
                }
                let provider = (binding.provider)().id;
                manager
                    .add_api_key(
                        provider.as_str(),
                        "one",
                        crate::credential::Secret::new(key)
                            .map_err(|_| StartupError::Credentials)?,
                    )
                    .unwrap();
                manager
                    .set_pool(
                        provider.as_str(),
                        binding.credential,
                        0,
                        crate::credential::CredentialPool {
                            members: vec![crate::credential::CredentialRef::ApiKey {
                                alias: "one".into(),
                            }],
                            fallback: false,
                            max_attempts: 1,
                        },
                    )
                    .unwrap();
            }
        }
        let mut config = serde_json::json!({"client_key":get("client_key")?.ok_or(StartupError::Credentials)?,"bind":get("bind")?.unwrap_or("127.0.0.1:0".into())});
        for field in ["models", "responses_via_chat"] {
            if let Some(value) = get(field)? {
                config[field] = serde_json::from_str(&value).map_err(|_| StartupError::Binding)?;
            }
        }
        manager.write_gateway_config_for_test(&config);
        let boot = Bootstrap::from_directory(directory.path())?;
        Ok(Self {
            boot,
            _directory: directory,
        })
    }
}

#[test]
fn responses_chat_bridge_is_explicit_and_replaces_not_falls_back_to_native_responses() {
    let settings = |name: &str| {
        Ok(match name {
            "client_key" => Some("synthetic-gateway-client-token-0001".into()),
            "openrouter-api-key" => Some("synthetic-upstream-only".into()),
            "models" | "responses_via_chat" => Some(r#"["gpt-6-luna"]"#.into()),
            _ => None,
        })
    };
    let boot = Fixture::from_lookup(settings).unwrap();
    let entry = &boot.gateway.state.entries[&(
        super::super::family(Profile::Responses),
        "gpt-6-luna".into(),
    )];
    assert_eq!(entry.candidates.len(), 1);
    assert_eq!(
        entry.candidates[0].endpoint.protocol,
        crate::topology::ProtocolProfile::OpenAiChat
    );
    assert_eq!(
        entry.candidates[0].endpoint.target.path.as_str(),
        "/api/v1/chat/completions"
    );
    for invalid in [
        r#"["unknown"]"#,
        r#"["gpt-6-luna","gpt-6-luna"]"#,
        r#"["hy4-preview"]"#,
    ] {
        assert!(
            Fixture::from_lookup(|name| if name == "responses_via_chat" {
                Ok(Some(invalid.into()))
            } else {
                settings(name)
            })
            .is_err()
        );
    }
    assert!(
        Fixture::from_lookup(|name| if name == "models" {
            Ok(None)
        } else {
            settings(name)
        })
        .is_err()
    );
}
