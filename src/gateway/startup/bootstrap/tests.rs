//! Independent activation/target assertions over synthetic file configuration.
use super::*;

pub(super) fn write_private(path: &std::path::Path, bytes: &[u8]) {
    use std::io::Write;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path).unwrap().write_all(bytes).unwrap();
}

pub(super) fn write_configuration(
    directory: &std::path::Path,
    mut service: serde_json::Value,
    models: serde_json::Value,
) {
    service["models_file"] = serde_json::json!("models.json");
    write_private(
        &directory.join("gateway.json"),
        &serde_json::to_vec(&service).unwrap(),
    );
    write_private(
        &directory.join("models.json"),
        &serde_json::to_vec(&models).unwrap(),
    );
}

/// Fixtures explicitly select catalog targets; the loader never discovers models.
pub(super) fn selected_models(labels: &[&str], providers: &[&str]) -> serde_json::Value {
    let catalog = catalog::default_topology().unwrap();
    let mut models = serde_json::Map::new();
    for label in labels {
        let model = if let Some(public) = catalog.model(label) {
            let candidates: Vec<_> = catalog
                .route_endpoints(&public.route)
                .into_iter()
                .filter(|endpoint| providers.contains(&endpoint.provider.as_str()))
                .map(|endpoint| serde_json::json!({"binding":endpoint.id.as_str()}))
                .collect();
            serde_json::json!({"candidates":candidates})
        } else {
            serde_json::json!({})
        };
        models.insert((*label).into(), model);
    }
    serde_json::json!({"models":models})
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
                &boot.gateway.state.entries[&(crate::gateway::family(protocol), model.into())];
            let candidate = entry
                .candidates
                .iter()
                .find(|c| {
                    c.endpoint.protocol
                        == match protocol {
                            Profile::Chat => crate::topology::ProtocolProfile::OpenAiChat,
                            Profile::Responses => crate::topology::ProtocolProfile::OpenAiResponses,
                        }
                })
                .unwrap();
            assert_eq!(candidate.endpoint.upstream_model, upstream);
            assert_eq!(
                candidate.provider.id.as_str(),
                credential.strip_suffix("-api-key").unwrap()
            );
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
                crate::gateway::ingress::admission::prepare(
                    &boot.gateway.state,
                    protocol,
                    &serde_json::to_vec(&request).unwrap()
                )
                .is_ok()
            );
        }
    }
    let boot = Fixture::from_lookup(|name| {
        Ok(match name {
            "client_key" => Some("synthetic-gateway-client-token-0001".into()),
            "aliyun-dashscope-cn-api-key" => Some("synthetic-metered-key".into()),
            "models" => Some(r#"["glm-5.3-prime"]"#.into()),
            _ => None,
        })
    })
    .unwrap();
    let (entry, request) = crate::gateway::ingress::admission::prepare(
        &boot.gateway.state,
        Profile::Chat,
        br#"{"model":"glm-5.3-prime","messages":[{"role":"user","content":"hi"}],"max_completion_tokens":64}"#,
    ).unwrap();
    assert_eq!(entry.public.canonical_model.as_str(), "glm-5.3");
    assert_eq!(entry.candidates.len(), 1);
    let candidate = &entry.candidates[0];
    assert_eq!(candidate.provider.id.as_str(), "aliyun-dashscope-cn");
    let prepared = crate::execution::prepare(
        &candidate.endpoint,
        &candidate.provider,
        &candidate.secret.resolve().unwrap().0,
        &request,
    )
    .unwrap();
    assert_eq!(prepared.origin, "https://dashscope.aliyuncs.com");
    assert_eq!(prepared.path, "/compatible-mode/v1/chat/completions");
    let wire: serde_json::Value = serde_json::from_slice(&prepared.body).unwrap();
    assert_eq!(wire["model"], "glm-5.3-prime");
    assert_eq!(wire["max_tokens"], 64);
    assert!(boot.gateway.state.entries.contains_key(&(
        crate::gateway::family(Profile::Responses),
        "glm-5.3-prime".into(),
    )));
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
            for model in ["qwen3.8-flash", "qwen3.8-max"] {
                let value = if profile == Profile::Chat {
                    serde_json::json!({"model":model,"messages":[{"role":"user","content":"hi"}],"max_completion_tokens":32})
                } else {
                    serde_json::json!({"model":model,"input":"hi","max_output_tokens":32})
                };
                let admitted = crate::gateway::ingress::admission::prepare(
                    &boot.gateway.state,
                    profile,
                    &serde_json::to_vec(&value).unwrap(),
                );
                let (entry, request) = admitted.unwrap();
                assert_eq!(
                    entry.candidates.len(),
                    2 * (usize::from(subscription) + usize::from(metered))
                );
                for candidate in &entry.candidates {
                    let (origin, secret) = match candidate.provider.id.as_str() {
                        "aliyun-tokenplan-cn" => {
                            assert!(subscription);
                            (
                                "https://token-plan.cn-beijing.maas.aliyuncs.com",
                                "synthetic-subscription-key",
                            )
                        }
                        "aliyun-dashscope-cn" => {
                            assert!(metered);
                            ("https://dashscope.aliyuncs.com", "synthetic-metered-key")
                        }
                        _ => panic!("unexpected provider"),
                    };
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
                        if candidate.endpoint.protocol
                            == crate::topology::ProtocolProfile::OpenAiChat
                        {
                            "/compatible-mode/v1/chat/completions"
                        } else {
                            "/compatible-mode/v1/responses"
                        }
                    );
                    let wire: serde_json::Value = serde_json::from_slice(&prepared.body).unwrap();
                    assert_eq!(wire["model"], model);
                    assert_eq!(
                        wire[if candidate.endpoint.protocol
                            == crate::topology::ProtocolProfile::OpenAiChat
                        {
                            "max_tokens"
                        } else {
                            "max_output_tokens"
                        }],
                        32
                    );
                }
            }
        }
        if subscription && metered {
            let subscription_entry = &boot.gateway.state.entries[&(
                crate::gateway::family(Profile::Chat),
                "qwen3.8-flash".into(),
            )];
            let metered_entry = &boot.gateway.state.entries
                [&(crate::gateway::family(Profile::Chat), "qwen3.8-max".into())];
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
        let directory = crate::credential::tests::support::private_directory();
        let manager =
            crate::credential::CredentialManager::new(directory.path().join("credentials"), vec![])
                .unwrap();
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
        let service = serde_json::json!({"client_key":get("client_key")?.ok_or(StartupError::Credentials)?,"bind":get("bind")?.unwrap_or("127.0.0.1:0".into())});
        let pools = manager.pools().unwrap();
        let providers: Vec<_> = pools
            .iter()
            .map(|(provider, _, _)| provider.as_str())
            .collect();
        let labels: Vec<String> = match get("models")? {
            Some(value) => serde_json::from_str(&value).map_err(|_| StartupError::Binding)?,
            None => catalog::API_KEY_BINDINGS
                .iter()
                .filter(|b| {
                    providers.contains(&(b.provider)().id.as_str())
                        && !b.public_model().contract.audio_input
                })
                .map(|b| b.model.to_owned())
                .collect(),
        };
        let mut models = selected_models(
            &labels.iter().map(String::as_str).collect::<Vec<_>>(),
            &providers,
        );
        if let Some(value) = get("model_config")? {
            models = serde_json::from_str(&value).map_err(|_| StartupError::Binding)?;
        }
        if get("reasoning_as_text")?.as_deref() == Some("true") {
            for model in models["models"].as_object_mut().unwrap().values_mut() {
                if let Some(candidates) = model
                    .get_mut("candidates")
                    .and_then(serde_json::Value::as_array_mut)
                {
                    for candidate in candidates {
                        candidate["reasoning_as_text"] = serde_json::json!(true);
                    }
                }
            }
        }
        write_configuration(directory.path(), service, models);
        let boot = Bootstrap::from_directory(directory.path())?;
        Ok(Self {
            boot,
            _directory: directory,
        })
    }
}

#[test]
fn responses_chat_bridge_is_explicit_and_replaces_not_falls_back_to_native_responses() {
    let binding = catalog::API_KEY_BINDINGS
        .iter()
        .find(|b| b.model == "gpt-6-luna" && (b.provider)().id.as_str() == "openrouter")
        .unwrap()
        .endpoint_id(crate::topology::ProtocolProfile::OpenAiChat);
    let config =
        serde_json::json!({"models":{"gpt-6-luna":{"candidates":[{"binding":binding.as_str()}]}}});
    let settings = |name: &str| {
        Ok(match name {
            "client_key" => Some("synthetic-gateway-client-token-0001".into()),
            "openrouter-api-key" => Some("synthetic-upstream-only".into()),
            "model_config" => Some(config.to_string()),
            _ => None,
        })
    };
    let boot = Fixture::from_lookup(settings).unwrap();
    let entry = &boot.gateway.state.entries[&(
        crate::gateway::family(Profile::Responses),
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
    for invalid in ["unknown", "deepseek-flash-chat"] {
        let mut bad = config.clone();
        bad["models"]["gpt-6-luna"]["candidates"][0]["binding"] = serde_json::json!(invalid);
        assert!(
            Fixture::from_lookup(|name| if name == "model_config" {
                Ok(Some(bad.to_string()))
            } else {
                settings(name)
            })
            .is_err()
        );
    }
    assert!(
        Fixture::from_lookup(|name| if name == "model_config" {
            Ok(Some(
                r#"{"models":{"gpt-6-luna":{"candidates":[]}}}"#.into(),
            ))
        } else {
            settings(name)
        })
        .is_err()
    );
}

#[test]
fn native_messages_binding_activates_only_when_listed() {
    // Both protocol families share the same public contract; activation remains explicit.
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
        let key = (crate::gateway::family(protocol), haiku.into());
        assert!(!boot.gateway.state.entries.contains_key(&key));
    }
    let boot = Fixture::from_lookup(settings(Some(r#"["claude-haiku-5-5"]"#))).unwrap();
    // Chat, Responses and Messages clients.
    assert_eq!(boot.gateway.state.entries.len(), 3);
    for protocol in [Profile::Chat, Profile::Responses] {
        let entry = &boot.gateway.state.entries[&(crate::gateway::family(protocol), haiku.into())];
        let endpoint = &entry.candidates[0].endpoint;
        assert_eq!(
            endpoint.protocol,
            crate::topology::ProtocolProfile::AnthropicMessages
        );
        assert_eq!(endpoint.target.path.as_str(), "/zen/go/v1/messages");
    }
    let mixed = Fixture::from_lookup(|name| {
        if name == "openrouter-api-key" {
            Ok(Some("synthetic-router-key".into()))
        } else {
            settings(Some(r#"["claude-haiku-5-5"]"#))(name)
        }
    })
    .unwrap();
    let body = br#"{"model":"claude-haiku-5-5","max_tokens":64,"messages":[
        {"role":"user","content":"read the file"},
        {"role":"assistant","content":[{"type":"tool_use","id":"call-1","name":"read","input":{}}]},
        {"role":"user","content":[{"type":"tool_result","tool_use_id":"call-1","content":"failed","is_error":true}]}
    ]}"#;
    let (_, native_request) =
        crate::gateway::ingress::admission::prepare_messages(&boot.gateway.state, body).unwrap();
    let (entry, mut request) =
        crate::gateway::ingress::admission::prepare_messages(&mixed.gateway.state, body).unwrap();
    assert_eq!(request, native_request);
    request.conversation = Some(
        crate::semantic::context::ConversationContext::conversation("synthetic-conversation")
            .unwrap(),
    );
    let original = request.clone();
    for candidate in &entry.candidates {
        let secret = candidate.secret.resolve().unwrap().0;
        let (prepared, rules) = crate::execution::prepare_with_rules(
            &candidate.endpoint,
            &candidate.provider,
            &secret,
            &request,
        )
        .unwrap();
        let wire: serde_json::Value = serde_json::from_slice(&prepared.body).unwrap();
        if candidate.endpoint.protocol == crate::topology::ProtocolProfile::AnthropicMessages {
            assert_eq!(wire["messages"][2]["content"][0]["is_error"], true);
            assert!(!rules.contains(&"openai.omit-tool-result-errors.v1"));
        } else {
            assert_eq!(wire["messages"][2]["content"], "failed");
            assert!(rules.contains(&"openai.omit-tool-result-errors.v1"));
        }
    }
    assert_eq!(entry.eligible(&request).unwrap().len(), 2);
    assert_eq!(request, original);
}

#[test]
fn xiaomi_audio_is_explicit_and_prepares_its_own_json_contracts() {
    let settings = |selected| {
        move |name: &str| {
            Ok(match name {
                "client_key" => Some("synthetic-gateway-client-token-0001".into()),
                "xiaomi-api-key" => Some("synthetic-upstream-only".into()),
                "models" if selected => Some(r#"["mimo-v2.5-tts","mimo-v2.5-asr"]"#.into()),
                _ => None,
            })
        }
    };
    let implicit = Fixture::from_lookup(settings(false)).unwrap();
    assert!(implicit.gateway.state.speech.is_empty());
    assert!(implicit.gateway.state.transcriptions.is_empty());
    let explicit = Fixture::from_lookup(settings(true)).unwrap();
    assert!(explicit.gateway.state.entries.is_empty());
    assert_eq!(explicit.gateway.state.speech.len(), 1);
    assert_eq!(explicit.gateway.state.transcriptions.len(), 1);
    let topology = catalog::default_topology().unwrap();
    let provider = crate::provider::catalog::xiaomi();
    let secret = SecretMaterial::new("synthetic-upstream-only").unwrap();
    let speech = crate::adapter::speech::Request::new(
        "mimo-v2.5-tts",
        crate::semantic::task::speech_synthesis::SpeechRequest::new("Hello.", "Chloe").unwrap(),
    );
    let transcription = crate::adapter::transcription::Request {
        model: "mimo-v2.5-asr".into(),
        task: crate::semantic::task::speech_recognition::RecognitionRequest::new(
            crate::semantic::task::speech_recognition::AudioInput::new(
                bytes::Bytes::from_static(b"synthetic"),
                crate::semantic::task::speech_recognition::InputFormat::Wav,
            )
            .unwrap(),
        ),
    };
    for prepared in [
        crate::execution::speech::prepare(
            topology.speech_route(&speech.model).unwrap(),
            &provider,
            &secret,
            &speech,
        )
        .unwrap(),
        crate::execution::transcription::prepare(
            topology.transcription_route(&transcription.model).unwrap(),
            &provider,
            &secret,
            &transcription,
        )
        .unwrap(),
    ] {
        assert_eq!(prepared.origin, "https://api.xiaomimimo.com");
        assert_eq!(prepared.path, "/v1/chat/completions");
        assert!(
            prepared
                .safe_headers
                .contains(&("accept".into(), "application/json".into()))
        );
        assert!(
            !prepared
                .safe_headers
                .iter()
                .any(|(name, _)| name == "x-dashscope-sse")
        );
        assert_eq!(
            prepared.auth_header,
            (
                "authorization".into(),
                "Bearer synthetic-upstream-only".into()
            )
        );
    }
}

#[test]
fn codex_history_reaches_the_native_binding_by_named_rules() {
    let boot = Fixture::from_lookup(|name| {
        Ok(match name {
            "client_key" => Some("synthetic-gateway-client-token-0001".into()),
            "opencode-go-api-key" => Some("synthetic-upstream-only".into()),
            "models" => Some(r#"["claude-haiku-5-5"]"#.into()),
            "reasoning_as_text" => Some("true".into()),
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
        let mut body = serde_json::json!({"model":"claude-haiku-5-5","instructions":"brief","max_output_tokens":64,
        "temperature":0.2,"store":false,"include":["reasoning.encrypted_content"],
        "tools":[{"type":"function","name":"lookup","parameters":{"type":"object","properties":{}},"strict":false}],
        "input":input});
        if !reasoning.is_null() {
            body["reasoning"] = reasoning;
        }
        body
    };
    let prepare = |reasoning, input: &[serde_json::Value]| {
        let (entry, mut request) = crate::gateway::ingress::admission::prepare(
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
    let (wire, rules) = prepare(serde_json::Value::Null, &input).unwrap();
    let projected = "mc_0af6315558b41599b4a0fbfa5d31d4431b2e5c81";
    assert_eq!(
        wire,
        serde_json::json!({"model":"claude-haiku-5-5","max_tokens":64,"system":"brief",
        "tools":[{"name":"lookup","input_schema":{"type":"object","properties":{}},"strict":false}],
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
        ["messages.omit-sampling.v1", "messages.project-call-ids.v1",]
    );
    // Reasoning-only effort cannot become response-wide effort, and display
    // alone cannot enable adaptive thinking.
    assert!(prepare(serde_json::json!({"effort":"xhigh"}), &input).is_err());
    assert!(prepare(serde_json::json!({"summary":"auto"}), &input).is_err());
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
    let (entry, mut rejected) = crate::gateway::ingress::admission::prepare(
        &boot.gateway.state,
        Profile::Responses,
        &serde_json::to_vec(&body(serde_json::Value::Null, &history)).unwrap(),
    )
    .unwrap();
    rejected.conversation =
        Some(crate::semantic::context::ConversationContext::conversation("synthetic").unwrap());
    let mut disabled = entry.candidates[0].endpoint.clone();
    assert!(
        disabled
            .representation
            .messages()
            .unwrap()
            .reasoning_as_text
    );
    disabled
        .representation
        .messages_mut()
        .unwrap()
        .reasoning_as_text = false;
    assert!(disabled.encode_request(&rejected).is_err());
    let (wire, rules) = prepare(serde_json::Value::Null, &history).unwrap();
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
    let error = prepare(serde_json::Value::Null, &input).unwrap_err();
    assert!(format!("{error:?}").contains("duplicate call"), "{error:?}");
}

#[test]
fn configured_failover_orders_and_limits_a_models_providers() {
    let directory = crate::credential::tests::support::private_directory();
    let store = directory.path().join("credentials");
    let manager = crate::credential::CredentialManager::new(&store, vec![]).unwrap();
    manager
        .add_api_key(
            "openrouter",
            "one",
            crate::credential::Secret::new("synthetic-configured-key".into()).unwrap(),
        )
        .unwrap();
    manager
        .set_pool(
            "openrouter",
            "selected-key",
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
    let binding = catalog::API_KEY_BINDINGS
        .iter()
        .find(|b| b.model == "claude-haiku-5-5" && (b.provider)().id.as_str() == "openrouter")
        .unwrap();
    let gateway_file = directory.path().join("gateway.json");
    let models_file = directory.path().join("models.json");
    let private_write = |path: &std::path::Path, value: serde_json::Value| {
        use std::io::Write;
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        options
            .open(path)
            .unwrap()
            .write_all(&serde_json::to_vec(&value).unwrap())
            .unwrap();
    };
    private_write(
        &gateway_file,
        serde_json::json!({
            "client_key":"synthetic-gateway-client-token-0001",
            "bind":"127.0.0.1:0",
            "models_file":"models.json"
        }),
    );
    let model_config = serde_json::json!({"models":{"claude-haiku-5-5":{
        "max_attempts":2,
        "total_timeout_ms":12000,
        "candidates":[{
            "binding":binding.endpoint_id(crate::topology::ProtocolProfile::OpenAiChat).as_str(),
            "base_url":"https://primary.example/custom/v1",
            "upstream_model":"configured-wire-model",
            "credential":"selected-key",
            "attempt_timeout_ms":3000
        },{
            "binding":binding.endpoint_id(crate::topology::ProtocolProfile::OpenAiChat).as_str(),
            "base_url":"https://secondary.example/v1",
            "credential":"selected-key",
            "attempt_timeout_ms":4000
        }]
    }}});
    private_write(&models_file, model_config.clone());
    let configured = Bootstrap::from_files(&gateway_file, &store).unwrap();
    assert!(configured.gateway.state.limits.delta_coalescing.is_none());
    let mut service = serde_json::json!({
        "client_key":"synthetic-gateway-client-token-0001",
        "bind":"127.0.0.1:0","models_file":"models.json",
        "delta_coalescing":{"max_delay_ms":10,"max_pending_bytes":16384}
    });
    private_write(&gateway_file, service.clone());
    let enabled = Bootstrap::from_files(&gateway_file, &store).unwrap();
    let policy = enabled.gateway.state.limits.delta_coalescing.unwrap();
    assert_eq!((policy.max_delay_ms, policy.max_pending_bytes), (10, 16384));
    for invalid in [
        serde_json::json!({"max_delay_ms":0,"max_pending_bytes":16384}),
        serde_json::json!({"max_delay_ms":10,"max_pending_bytes":0}),
    ] {
        service["delta_coalescing"] = invalid;
        private_write(&gateway_file, service.clone());
        assert!(Bootstrap::from_files(&gateway_file, &store).is_err());
    }
    service.as_object_mut().unwrap().remove("delta_coalescing");
    private_write(&gateway_file, service);
    let entry = &configured.gateway.state.entries[&(
        crate::gateway::family(Profile::Chat),
        "claude-haiku-5-5".into(),
    )];
    assert_eq!(entry.policy.max_attempts, 2);
    assert_eq!(entry.policy.total_timeout_ms, Some(12000));
    assert_eq!(entry.candidates.len(), 2);
    assert_eq!(
        entry.candidates[0].endpoint.target.origin.as_str(),
        "https://primary.example"
    );
    assert_eq!(
        entry.candidates[0].endpoint.target.path.as_str(),
        "/custom/v1/chat/completions"
    );
    assert_eq!(entry.candidates[0].endpoint.execution.timeout_ms, 3000);
    assert_eq!(
        entry.candidates[0].endpoint.upstream_model,
        "configured-wire-model"
    );
    assert_eq!(
        entry.candidates[1].endpoint.target.origin.as_str(),
        "https://secondary.example"
    );
    assert_ne!(
        entry.candidates[0].cache_scope,
        entry.candidates[1].cache_scope
    );
    assert_ne!(
        entry.candidates[0]
            .endpoint
            .adapter()
            .unwrap()
            .openai()
            .unwrap()
            .adaptation
            .scope,
        entry.candidates[1]
            .endpoint
            .adapter()
            .unwrap()
            .openai()
            .unwrap()
            .adaptation
            .scope,
    );
    assert!(
        !entry.candidates[0]
            .endpoint
            .representation
            .openai()
            .unwrap()
            .reasoning_as_text
    );
    let mut invalid = model_config.clone();
    invalid["models"]["claude-haiku-5-5"]["candidates"][0]["base_url"] =
        serde_json::json!("https://user:secret@primary.example/v1");
    private_write(&models_file, invalid);
    assert!(Bootstrap::from_files(&gateway_file, &store).is_err());

    let mut ordered = selected_models(&["claude-haiku-5-5"], &["openrouter", "opencode-go"]);
    ordered["models"]["claude-haiku-5-5"]["candidates"]
        .as_array_mut()
        .unwrap()
        .reverse();
    let settings = |name: &str| {
        Ok(match name {
            "client_key" => Some("synthetic-gateway-client-token-0001".into()),
            "opencode-go-api-key" | "openrouter-api-key" => Some("synthetic-upstream".into()),
            "model_config" => Some(ordered.to_string()),
            _ => None,
        })
    };
    let providers = |boot: &Fixture, protocol| {
        boot.gateway
            .state
            .entries
            .get(&(crate::gateway::family(protocol), "claude-haiku-5-5".into()))
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
    let boot = Fixture::from_lookup(settings).unwrap();
    assert_eq!(
        providers(&boot, Profile::Chat).unwrap(),
        ["opencode-go", "openrouter"]
    );
    assert_eq!(
        providers(&boot, Profile::Responses).unwrap(),
        ["opencode-go", "openrouter"]
    );
    let mut unlisted = ordered.clone();
    unlisted["models"]["claude-haiku-5-5"]["candidates"]
        .as_array_mut()
        .unwrap()
        .remove(1);
    let boot = Fixture::from_lookup(|name| {
        if name == "model_config" {
            Ok(Some(unlisted.to_string()))
        } else {
            settings(name)
        }
    })
    .unwrap();
    assert_eq!(providers(&boot, Profile::Chat).unwrap(), ["opencode-go"]);
    write_private(&models_file, br#"{"models":{},"models":{}}"#);
    assert!(Bootstrap::from_files(&gateway_file, &store).is_err());
    let mut invalid = model_config;
    invalid["models"]["claude-haiku-5-5"]["max_attempts"] = serde_json::json!(0);
    private_write(&models_file, invalid);
    assert!(Bootstrap::from_files(&gateway_file, &store).is_err());

    let replay_binding = catalog::API_KEY_BINDINGS
        .iter()
        .find(|b| b.model == "gpt-6-luna" && (b.provider)().id.as_str() == "openrouter")
        .unwrap()
        .endpoint_id(crate::topology::ProtocolProfile::OpenAiResponses);
    private_write(
        &models_file,
        serde_json::json!({"models":{"gpt-6-luna":{
            "candidates":[
                {"binding":replay_binding.as_str(),"credential":"selected-key","base_url":"https://primary.example/first/v1"},
                {"binding":replay_binding.as_str(),"credential":"selected-key","base_url":"https://primary.example/second/v1"}
            ]
        }}}),
    );
    let configured = Bootstrap::from_files(&gateway_file, &store).unwrap();
    let entry = &configured.gateway.state.entries[&(
        crate::gateway::family(Profile::Responses),
        "gpt-6-luna".into(),
    )];
    let first = &entry.candidates[0].endpoint;
    let second = &entry.candidates[1].endpoint;
    assert_ne!(first.replay_origin(), second.replay_origin());
    let sealed = crate::protocol::replay_envelope::seal(
        &first.replay_origin(),
        crate::semantic::task::generation::ReplayFormat::ResponsesCompaction,
        "synthetic-compacted-history",
    )
    .unwrap();
    let mut replay = crate::protocol::openai::Adapter::new(
        Profile::Responses,
        crate::protocol::openai::Dialect::Standard,
        Some(crate::execution::replay::CLIENT_ORIGIN.clone()),
    )
    .decode_request(
        &serde_json::to_vec(&serde_json::json!({
            "model":"gpt-6-luna",
            "input":[{"type":"compaction","id":"cmp","encrypted_content":sealed.as_str()}]
        }))
        .unwrap(),
    )
    .unwrap();
    crate::execution::replay::open_from_client(&mut replay).unwrap();
    assert!(first.encode_request(&replay).is_err());
    // Catalog compaction admission stays closed. Synthetic caps isolate replay
    // authority independently of that unchanged public admission boundary.
    let mut first = first.clone();
    let mut second = second.clone();
    first
        .representation
        .openai_mut()
        .unwrap()
        .semantics
        .compaction = true;
    second
        .representation
        .openai_mut()
        .unwrap()
        .semantics
        .compaction = true;
    assert_eq!(
        first.encode_request(&replay).unwrap()["input"][0]["encrypted_content"],
        "synthetic-compacted-history"
    );
    assert!(second.encode_request(&replay).is_err());

    let google = &catalog::GOOGLE_BINDINGS[0];
    let provider = google.provider();
    manager
        .add_api_key(
            provider.id.as_str(),
            "one",
            Secret::new("synthetic-google-key".into()).unwrap(),
        )
        .unwrap();
    manager
        .set_pool(
            provider.id.as_str(),
            google.credential,
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
    let mut models = serde_json::json!({"models":{google.model:{
        "thinking_tokens":{"minimal":1024,"low":2048,"medium":3072,"high":8192,"xhigh":16384,"max":24576},
        "candidates":[{"binding":google.endpoint().id.as_str(),"thinking_conversion":"effort_to_budget"}]
    }}});
    private_write(&models_file, models.clone());
    let configured = Bootstrap::from_files(&gateway_file, &store).unwrap();
    let (entry, request) = crate::gateway::ingress::admission::prepare(
        &configured.gateway.state, Profile::Responses,
        &serde_json::to_vec(&serde_json::json!({
            "model":google.model,"input":"hi","reasoning":{"effort":"medium"},"max_output_tokens":6000
        })).unwrap(),
    ).unwrap();
    let before = request.clone();
    let (wire, rules) = entry.candidates[0]
        .endpoint
        .encode_request_with_rules(&request)
        .unwrap();
    assert_eq!(
        wire["generationConfig"]["thinkingConfig"]["thinkingBudget"],
        3072
    );
    assert!(rules.contains(&"configured.effort-to-budget.v1"));
    assert_eq!(request, before);
    models["models"][google.model]["thinking_tokens"]["medium"] = serde_json::json!(7000);
    private_write(&models_file, models);
    let configured = Bootstrap::from_files(&gateway_file, &store).unwrap();
    let entry = &configured.gateway.state.entries[&(
        crate::gateway::family(Profile::Responses),
        google.model.into(),
    )];
    assert!(
        entry.candidates[0]
            .endpoint
            .encode_request(&request)
            .is_err()
    );

    private_write(
        &gateway_file,
        serde_json::json!({
            "client_key":"synthetic-gateway-client-token-0001",
            "credentials_dir":"credentials"
        }),
    );
    assert!(Bootstrap::from_options(&gateway_file, None, Some("http://127.0.0.1:9")).is_ok());
    assert!(Bootstrap::from_files(&gateway_file, &directory.path().join("empty-store")).is_err());
}
