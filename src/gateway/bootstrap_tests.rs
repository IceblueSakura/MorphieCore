//! Independent activation/target assertions over synthetic file configuration.
use super::*;

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
                    .openai()
                    .unwrap()
                    .adaptation
                    .scope,
                metered_entry.candidates[0]
                    .endpoint
                    .representation
                    .openai()
                    .unwrap()
                    .adaptation
                    .scope
            );
        }
    }
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
        for field in ["models", "responses_via_chat", "failover"] {
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

#[test]
fn native_messages_binding_activates_only_when_listed() {
    let settings = |models: Option<&'static str>| {
        move |name: &str| {
            Ok(match name {
                "client_key" => Some("synthetic-gateway-client-token-0001".into()),
                "opencode-go-api-key" => Some("synthetic-upstream-only".into()),
                "models" => models.map(Into::into),
                _ => None,
            })
        }
    };
    let haiku = "claude-haiku-5-5";
    let boot = Fixture::from_lookup(settings(None)).unwrap();
    for protocol in [Profile::Chat, Profile::Responses] {
        let key = (super::super::family(protocol), haiku.into());
        assert!(!boot.gateway.state.entries.contains_key(&key));
    }
    let boot = Fixture::from_lookup(settings(Some(r#"["claude-haiku-5-5"]"#))).unwrap();
    assert_eq!(boot.gateway.state.entries.len(), 2);
    for protocol in [Profile::Chat, Profile::Responses] {
        let entry = &boot.gateway.state.entries[&(super::super::family(protocol), haiku.into())];
        let endpoint = &entry.candidates[0].endpoint;
        assert_eq!(
            endpoint.protocol,
            crate::topology::ProtocolProfile::AnthropicMessages
        );
        assert_eq!(endpoint.target.path.as_str(), "/zen/go/v1/messages");
    }
}

#[test]
fn codex_history_reaches_the_native_binding_by_named_rules() {
    let boot = Fixture::from_lookup(|name| {
        Ok(match name {
            "client_key" => Some("synthetic-gateway-client-token-0001".into()),
            "opencode-go-api-key" => Some("synthetic-upstream-only".into()),
            "models" => Some(r#"["claude-haiku-5-5"]"#.into()),
            _ => None,
        })
    })
    .unwrap();
    let call = |id: &str, output: &str| {
        [
            serde_json::json!({"type":"function_call","call_id":id,"name":"lookup","arguments":"{}"}),
            serde_json::json!({"type":"function_call_output","call_id":id,"output":output}),
        ]
    };
    let mut input = vec![serde_json::json!({"role":"user","content":"go"})];
    input.extend(call("call:1", "found"));
    input.extend(call("call_2", "again"));
    let body = |reasoning: serde_json::Value, input: &[serde_json::Value]| {
        serde_json::json!({"model":"claude-haiku-5-5","instructions":"brief","max_output_tokens":64,
        "temperature":0.2,"store":false,"include":["reasoning.encrypted_content"],"reasoning":reasoning,
        "tools":[{"type":"function","name":"lookup","parameters":{"type":"object","properties":{}},"strict":false}],
        "input":input})
    };
    let prepare = |reasoning, input: &[serde_json::Value]| {
        let (entry, mut request) = super::super::admission::prepare(
            &boot.gateway.state,
            Profile::Responses,
            &serde_json::to_vec(&body(reasoning, input)).unwrap(),
        )
        .unwrap();
        request.conversation =
            Some(crate::semantic::context::ConversationContext::conversation("synthetic").unwrap());
        let candidate = &entry.candidates[0];
        let (prepared, rules) = crate::execution::prepare_with_rules(
            &candidate.endpoint,
            &candidate.provider,
            &candidate.secret.resolve().unwrap().0,
            &request,
        )?;
        Ok::<_, crate::execution::AttemptError>((
            serde_json::from_slice::<serde_json::Value>(&prepared.body).unwrap(),
            rules,
        ))
    };
    let (wire, rules) = prepare(
        serde_json::json!({"effort":"minimal","summary":"detailed","context":"all_turns"}),
        &input,
    )
    .unwrap();
    let projected = "mc_0af6315558b41599b4a0fbfa5d31d4431b2e5c81";
    assert_eq!(
        wire,
        serde_json::json!({"model":"claude-haiku-5-5","max_tokens":64,"system":"brief",
        "tools":[{"name":"lookup","input_schema":{"type":"object","properties":{}},"strict":false}],
        "thinking":{"type":"adaptive","display":"summarized"},"output_config":{"effort":"low"},
        "cache_control":{"type":"ephemeral"},"messages":[
            {"role":"user","content":[{"type":"text","text":"go"}]},
            {"role":"assistant","content":[{"type":"tool_use","id":projected,"name":"lookup","input":{}}]},
            {"role":"user","content":[{"type":"tool_result","tool_use_id":projected,"content":"found"}]},
            {"role":"assistant","content":[{"type":"tool_use","id":"call_2","name":"lookup","input":{}}]},
            {"role":"user","content":[{"type":"tool_result","tool_use_id":"call_2","content":"again"}]}
        ]})
    );
    assert_eq!(
        rules,
        [
            "messages.lower-effort.v1",
            "messages.summarize-reasoning.v1",
            "messages.omit-reasoning-context.v1",
            "messages.omit-sampling.v1",
            "messages.project-call-ids.v1",
        ]
    );
    let (wire, _) = prepare(serde_json::json!({"effort":"xhigh"}), &input).unwrap();
    assert_eq!(wire["output_config"], serde_json::json!({"effort":"xhigh"}));
    assert!(wire.get("thinking").is_none());
    let (wire, _) = prepare(serde_json::json!({"summary":"auto"}), &input).unwrap();
    assert_eq!(
        wire["thinking"],
        serde_json::json!({"type":"adaptive","display":"summarized"})
    );
    assert!(wire.get("output_config").is_none());
    // Foreign opaque values are dropped; readable reasoning without this target's
    // signature becomes assistant text in every turn.
    let foreign = crate::protocol::replay_envelope::seal(
        &crate::protocol::replay_envelope::issuer("openai", "gpt-6.1-sol"),
        crate::semantic::task::generation::ReplayFormat::ResponsesEncrypted,
        "synthetic-foreign-token",
    )
    .unwrap();
    let reasoning = |id: &str, summary: &[&str], sealed: Option<&str>| {
        let mut item = serde_json::json!({"type":"reasoning","id":id,"summary":summary
            .iter().map(|text| serde_json::json!({"type":"summary_text","text":text})).collect::<Vec<_>>()});
        if let Some(sealed) = sealed {
            item["encrypted_content"] = serde_json::json!(sealed);
        }
        item
    };
    let mut history = vec![
        serde_json::json!({"role":"user","content":"go"}),
        reasoning("rs_1", &["Plan A"], Some(foreign.as_str())),
        reasoning("rs_2", &[], Some(foreign.as_str())),
        reasoning("rs_3", &["Plan B"], None),
    ];
    history.extend(call("call_2", "again"));
    let (wire, rules) = prepare(serde_json::json!({"effort":"low"}), &history).unwrap();
    assert_eq!(
        wire["messages"],
        serde_json::json!([
            {"role":"user","content":[{"type":"text","text":"go"}]},
            {"role":"assistant","content":[{"type":"text","text":"Plan A"},{"type":"text","text":"Plan B"},
                {"type":"tool_use","id":"call_2","name":"lookup","input":{}}]},
            {"role":"user","content":[{"type":"tool_result","tool_use_id":"call_2","content":"again"}]}
        ])
    );
    assert_eq!(
        rules,
        [
            "messages.omit-sampling.v1",
            "messages.omit-foreign-replay.v1",
            "messages.reasoning-as-text.v1",
        ]
    );
    // A client ID equal to a projected digest fails instead of merging two calls.
    input.truncate(3);
    input.extend(call(projected, "again"));
    let error = prepare(serde_json::json!({"effort":"low"}), &input).unwrap_err();
    assert!(format!("{error:?}").contains("duplicate call"), "{error:?}");
}

#[test]
fn configured_failover_orders_and_limits_a_models_providers() {
    let settings = |failover: Option<&'static str>| {
        move |name: &str| {
            Ok(match name {
                "client_key" => Some("synthetic-gateway-client-token-0001".into()),
                "zhipu-api-key" | "openrouter-api-key" => Some("synthetic-upstream".into()),
                "models" => Some(r#"["glm-5.3"]"#.into()),
                "failover" => failover.map(Into::into),
                _ => None,
            })
        }
    };
    let providers = |boot: &Fixture, protocol| {
        boot.gateway
            .state
            .entries
            .get(&(super::super::family(protocol), "glm-5.3".into()))
            .map(|entry| {
                assert_eq!(
                    entry.policy.fallback,
                    crate::topology::FallbackPolicy::BeforeCommit
                );
                entry
                    .candidates
                    .iter()
                    .map(|c| c.provider.id.as_str().to_owned())
                    .collect::<Vec<_>>()
            })
    };
    // Catalog order by default; OpenRouter serves Chat only.
    let boot = Fixture::from_lookup(settings(None)).unwrap();
    assert_eq!(
        providers(&boot, Profile::Chat).unwrap(),
        ["zhipu", "openrouter"]
    );
    let boot =
        Fixture::from_lookup(settings(Some(r#"{"glm-5.3":["openrouter","zhipu"]}"#))).unwrap();
    assert_eq!(
        providers(&boot, Profile::Chat).unwrap(),
        ["openrouter", "zhipu"]
    );
    assert_eq!(providers(&boot, Profile::Responses).unwrap(), ["zhipu"]);
    // An unlisted Provider is not used, even with a pool.
    let boot = Fixture::from_lookup(settings(Some(r#"{"glm-5.3":["openrouter"]}"#))).unwrap();
    assert_eq!(providers(&boot, Profile::Chat).unwrap(), ["openrouter"]);
    assert!(providers(&boot, Profile::Responses).is_none());
    for invalid in [
        r#"{"glm-5.3":[]}"#,
        r#"{"glm-5.3":["zhipu","zhipu"]}"#,
        r#"{"glm-5.3":["xiaomi"]}"#,
        r#"{"glm-5.3":["unknown"]}"#,
        r#"{"mimo-v2.6-pro":["xiaomi"]}"#,
    ] {
        assert!(
            Fixture::from_lookup(settings(Some(invalid))).is_err(),
            "{invalid}"
        );
    }
}
