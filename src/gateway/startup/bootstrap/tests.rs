//! Independent activation/target assertions over synthetic file configuration.
use super::*;

use super::test_support::*;
use crate::adapter::ClientProtocol;
use crate::gateway::family;

#[test]
fn readable_chat_profile_is_explicit_and_does_not_widen_other_protocols() {
    for selected in [None, Some("standard"), Some("readable_reasoning")] {
        let mut models = selected_models(&["qwen3.8-flash"], &["aliyun-dashscope-cn"]);
        if let Some(selected) = selected {
            models["models"]["qwen3.8-flash"]["chat_profile"] = serde_json::json!(selected);
        }
        let boot = Fixture::new(
            service(),
            models,
            &[(
                "aliyun-dashscope-cn",
                "aliyun-dashscope-cn-api-key",
                "synthetic",
            )],
        )
        .unwrap();
        let chat = &boot.gateway.state.entries[&(ClientProtocol::Chat, "qwen3.8-flash".into())];
        let (adapter, _) = chat.client.as_openai().unwrap();
        assert_eq!(
            adapter.adaptation.rules.readable_reasoning,
            selected == Some("readable_reasoning")
        );
        assert!(!adapter.adaptation.rules.thinking_controls);
        assert!(!adapter.adaptation.rules.structured_chat_reasoning);
        assert!(!adapter.adaptation.rules.chat_video_url);
        let responses =
            &boot.gateway.state.entries[&(ClientProtocol::Responses, "qwen3.8-flash".into())];
        assert_eq!(
            responses
                .client
                .as_openai()
                .unwrap()
                .0
                .adaptation
                .profile_id,
            "standard-v1"
        );
    }
    let mut models = selected_models(&["qwen3.8-flash"], &["aliyun-dashscope-cn"]);
    models["models"]["qwen3.8-flash"]["chat_profile"] = serde_json::json!("provider");
    assert!(
        Fixture::new(
            service(),
            models,
            &[(
                "aliyun-dashscope-cn",
                "aliyun-dashscope-cn-api-key",
                "synthetic"
            )]
        )
        .is_err()
    );
}

#[test]
fn shared_provider_credentials_keep_model_and_protocol_targets_distinct() {
    let boot = Fixture::new(
        service(),
        selected_models(
            &["mimo-v2.6-pro", "mimo-v2.6-flash", "glm-5.3"],
            &["xiaomi", "zhipu"],
        ),
        &[
            ("xiaomi", "xiaomi-api-key", "synthetic-upstream"),
            ("zhipu", "zhipu-api-key", "synthetic-upstream"),
        ],
    )
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
                    family(protocol),
                    &serde_json::to_vec(&request).unwrap(),
                    &crate::execution::report::Report::default()
                )
                .is_ok()
            );
        }
    }
    let boot = Fixture::new(
        service(),
        selected_models(&["glm-5.3-prime"], &["aliyun-dashscope-cn"]),
        &[(
            "aliyun-dashscope-cn",
            "aliyun-dashscope-cn-api-key",
            "synthetic-metered-key",
        )],
    )
    .unwrap();
    let (entry, request) = crate::gateway::ingress::admission::prepare(
        &boot.gateway.state,
        ClientProtocol::Chat,
        br#"{"model":"glm-5.3-prime","messages":[{"role":"user","content":"hi"}],"max_completion_tokens":64}"#,&crate::execution::report::Report::default()).unwrap();
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
    assert_eq!(wire["max_completion_tokens"], 64);
    assert!(wire.get("max_tokens").is_none());
    assert!(boot.gateway.state.entries.contains_key(&(
        crate::gateway::family(Profile::Responses),
        "glm-5.3-prime".into(),
    )));
}

#[test]
fn subscription_and_metered_entries_never_exchange_credentials_or_origins() {
    for (subscription, metered) in [(true, false), (false, true), (true, true)] {
        let mut credentials = vec![];
        if subscription {
            credentials.push((
                "aliyun-tokenplan-cn",
                "aliyun-tokenplan-cn-api-key",
                "synthetic-subscription-key",
            ));
        }
        if metered {
            credentials.push((
                "aliyun-dashscope-cn",
                "aliyun-dashscope-cn-api-key",
                "synthetic-metered-key",
            ));
        }
        let providers: Vec<_> = credentials
            .iter()
            .map(|(provider, _, _)| *provider)
            .collect();
        let boot = Fixture::new(
            service(),
            selected_models(&["qwen3.8-flash", "qwen3.8-max"], &providers),
            &credentials,
        )
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
                    family(profile),
                    &serde_json::to_vec(&value).unwrap(),
                    &crate::execution::report::Report::default(),
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
                            assert!(wire.get("max_tokens").is_none());
                            "max_completion_tokens"
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
    let models = selected_models(&["deepseek-flash"], &["deepseek"]);
    let credentials = [("deepseek", "deepseek-api-key", "synthetic-upstream-only")];
    assert!(Fixture::new(service(), models.clone(), &credentials).is_ok());
    assert!(matches!(
        Fixture::new(serde_json::json!({}), models.clone(), &credentials),
        Err(StartupError::Configuration("gateway"))
    ));
    let mut public = service();
    public["bind"] = serde_json::json!("0.0.0.0:8080");
    assert!(matches!(
        Fixture::new(public, models.clone(), &credentials),
        Err(StartupError::Listener)
    ));
    assert!(matches!(
        Fixture::new(service(), models, &[]),
        Err(StartupError::Credentials)
    ));
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
    let credentials = [(
        "openrouter",
        "openrouter-api-key",
        "synthetic-upstream-only",
    )];
    let boot = Fixture::new(service(), config.clone(), &credentials).unwrap();
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
        assert!(Fixture::new(service(), bad, &credentials).is_err());
    }
    assert!(
        Fixture::new(
            service(),
            serde_json::json!({"models":{"gpt-6-luna":{"candidates":[]}}}),
            &credentials
        )
        .is_err()
    );
}

#[test]
fn transcription_model_requires_one_explicit_binding_when_providers_share_identity() {
    let model = "qwen-audio-3.0-asr-flash";
    let config = |selection: serde_json::Value| serde_json::json!({"models":{model:selection}});
    for (binding, provider, pool) in [
        (
            "tokenplan-qwen-asr",
            "aliyun-tokenplan-cn",
            "aliyun-tokenplan-cn-api-key",
        ),
        (
            "dashscope-qwen-asr",
            "aliyun-dashscope-cn",
            "aliyun-dashscope-cn-api-key",
        ),
    ] {
        let credentials = [(provider, pool, "synthetic-asr-key")];
        let selection = serde_json::json!({"binding":binding});
        let boot = Fixture::new(service(), config(selection.clone()), &credentials).unwrap();
        assert_eq!(boot.gateway.state.transcriptions.len(), 1);
        assert!(boot.gateway.state.transcriptions.contains_key(model));
        assert!(boot.gateway.state.entries.is_empty());
        let discovery: serde_json::Value =
            serde_json::from_slice(&boot.gateway.state.models.list).unwrap();
        assert_eq!(discovery["data"].as_array().unwrap().len(), 1);
        assert_eq!(discovery["data"][0]["id"], model);
        assert_eq!(discovery["data"][0]["owned_by"], "Alibaba");
        assert!(matches!(
            Fixture::new(service(), config(selection.clone()), &[]),
            Err(StartupError::Credentials)
        ));
        for invalid in [
            serde_json::json!({}),
            serde_json::json!({"binding":"unknown"}),
            serde_json::json!({"binding":"xiaomi-asr"}),
            serde_json::json!({"binding":binding,"max_attempts":2}),
            serde_json::json!({"binding":binding,"candidates":[{"binding":binding}]}),
        ] {
            assert!(Fixture::new(service(), config(invalid), &credentials).is_err());
        }
        let old = serde_json::json!({"models":{
            "qwen-audio-3.0-asr-flash-dashscope":selection
        }});
        assert!(Fixture::new(service(), old, &credentials).is_err());
    }
    let wrong_pool = [(
        "aliyun-tokenplan-cn",
        "aliyun-tokenplan-cn-api-key",
        "synthetic-asr-key",
    )];
    assert!(matches!(
        Fixture::new(
            service(),
            config(serde_json::json!({"binding":"dashscope-qwen-asr"})),
            &wrong_pool
        ),
        Err(StartupError::Credentials)
    ));
}

#[test]
fn xiaomi_audio_is_explicit_and_prepares_its_own_json_contracts() {
    let credentials = [("xiaomi", "xiaomi-api-key", "synthetic-upstream-only")];
    let implicit = Fixture::new(
        service(),
        selected_models(&["mimo-v2.6-pro"], &["xiaomi"]),
        &credentials,
    )
    .unwrap();
    assert!(implicit.gateway.state.speech.is_empty());
    assert!(implicit.gateway.state.transcriptions.is_empty());
    let explicit = Fixture::new(
        service(),
        selected_models(&["mimo-v2.5-tts", "mimo-v2.5-asr"], &["xiaomi"]),
        &credentials,
    )
    .unwrap();
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
    let mut models = selected_models(&["claude-haiku-5-5"], &["opencode-go"]);
    models["models"]["claude-haiku-5-5"]["candidates"][0]["reasoning_as_text"] =
        serde_json::json!(true);
    let boot = Fixture::new(
        service(),
        models,
        &[(
            "opencode-go",
            "opencode-go-api-key",
            "synthetic-upstream-only",
        )],
    )
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
            ClientProtocol::Responses,
            &serde_json::to_vec(&body(reasoning, input)).unwrap(),
            &crate::execution::report::Report::default(),
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
        [
            "messages.default-automatic-cache.v1",
            "messages.normalize-tool-arguments.v1",
            "messages.omit-sampling.v1",
            "messages.project-call-ids.v1",
        ]
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
        ClientProtocol::Responses,
        &serde_json::to_vec(&body(serde_json::Value::Null, &history)).unwrap(),
        &crate::execution::report::Report::default(),
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
            {"role":"assistant","content":[{"type":"text","text":"Plan B"},
                {"type":"tool_use","id":"call_2","name":"lookup","input":{}}]},
            {"role":"user","content":[{"type":"tool_result","tool_use_id":"call_2","content":"again"}]}
        ])
    );
    assert_eq!(
        rules,
        [
            "messages.default-automatic-cache.v1",
            "messages.normalize-tool-arguments.v1",
            "messages.omit-sampling.v1",
            "messages.reasoning-as-text.v1",
            "messages.transfer-visible-history.v1",
        ]
    );
    // A client ID equal to a projected digest fails instead of merging two calls.
    input.truncate(3);
    input.extend(call(projected, "again"));
    let error = prepare(serde_json::Value::Null, &input).unwrap_err();
    assert!(
        matches!(
            error.codec(),
            Some(crate::protocol::CodecError::Invalid(
                crate::protocol::failure::ShapeReason::CallIdentity
            ))
        ),
        "{error:?}"
    );
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
        write_private(path, &serde_json::to_vec(&value).unwrap());
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
    let credentials = [
        ("opencode-go", "opencode-go-api-key", "synthetic-upstream"),
        ("openrouter", "openrouter-api-key", "synthetic-upstream"),
    ];
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
    let boot = Fixture::new(test_support::service(), ordered.clone(), &credentials).unwrap();
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
    let boot = Fixture::new(test_support::service(), unlisted, &credentials).unwrap();
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
            google.credential().as_str(),
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
    let limits_config = serde_json::json!({"models":{google.model:{"candidates":[
        {"binding":google.endpoint().id.as_str(),
            "allowed_effort":["medium","high"],"maximum_output_tokens":4096},
        {"binding":google.interactions_endpoint().id.as_str()}
    ]}}});
    private_write(&models_file, limits_config.clone());
    let configured = Bootstrap::from_files(&gateway_file, &store).unwrap();
    for protocol in [Profile::Chat, Profile::Responses] {
        let entry = &configured.gateway.state.entries
            [&(crate::gateway::family(protocol), google.model.into())];
        assert_eq!(entry.candidates.len(), 2);
        let limits = entry.candidates[0].endpoint.representation.model_limits();
        assert_eq!(limits.maximum_output_tokens, Some(4096));
        assert_eq!(
            limits.allowed_effort.as_deref(),
            Some(
                &[
                    crate::semantic::task::generation::EffortLevel::Medium,
                    crate::semantic::task::generation::EffortLevel::High,
                ][..]
            )
        );
        assert_eq!(
            entry.candidates[1].endpoint.representation.model_limits(),
            google.interactions_endpoint().representation.model_limits()
        );
    }
    // The loader retains the catalog scope policy when no conversion override is supplied.
    let (entry, request) = crate::gateway::ingress::admission::prepare(
        &configured.gateway.state,
        ClientProtocol::Messages,
        &serde_json::to_vec(&serde_json::json!({"model":google.model,"max_tokens":2048,
            "thinking":{"type":"adaptive"},"output_config":{"effort":"medium"},
            "messages":[{"role":"user","content":"go"}]}))
        .unwrap(),
        &crate::execution::report::Report::default(),
    )
    .unwrap();
    let before = request.clone();
    for candidate in &entry.candidates {
        use crate::semantic::task::generation::{EffortControl, EffortLevel, EffortScope};
        let projection = candidate
            .endpoint
            .encode_request_with_evidence(&request)
            .unwrap();
        let evidence: Vec<_> = projection
            .evidence
            .iter()
            .filter_map(|c| match c {
                crate::lowering::generation::Effect::Reasoning {
                    rule,
                    input,
                    output,
                    preset_tokens,
                    ..
                } => Some((rule, input, output, preset_tokens)),
                _ => None,
            })
            .collect();
        assert_eq!(evidence.len(), 1);
        let (rule, input, output, preset) = evidence[0];
        assert_eq!(
            rule.as_str(),
            "google.gemini-3.8-flash.response-effort-to-thinking.v1"
        );
        assert_eq!(
            input.effort.value(),
            Some(&EffortControl {
                scope: EffortScope::Response,
                level: EffortLevel::Medium,
            })
        );
        let mut expected = input.clone();
        expected.effort = crate::semantic::value::Presence::Value(EffortControl {
            scope: EffortScope::Reasoning,
            level: EffortLevel::Medium,
        });
        assert_eq!(output, &expected);
        assert_eq!(*preset, None);
        assert_eq!(input.max_output_tokens, Some(2048));
        assert!(
            !candidate
                .endpoint
                .representation
                .semantics()
                .response_effort
        );
    }
    assert_eq!(request, before);
    let scoped_request = request;
    for (name, value) in [
        ("allowed_effort", serde_json::json!([])),
        ("allowed_effort", serde_json::json!(["high", "high"])),
        ("allowed_effort", serde_json::json!(["minimal"])),
        ("allowed_effort", serde_json::json!(["unknown"])),
        (
            "allowed_effort",
            serde_json::json!(["high", "medium", "low", "minimal", "xhigh", "max", "high"]),
        ),
        ("allowed_effort", serde_json::json!("high")),
        ("maximum_output_tokens", serde_json::json!(0)),
        ("maximum_output_tokens", serde_json::json!(65537)),
        ("maximum_output_tokens", serde_json::json!(-1)),
        ("maximum_output_tokens", serde_json::json!(1.5)),
    ] {
        let mut invalid = limits_config.clone();
        invalid["models"][google.model]["candidates"][0][name] = value;
        private_write(&models_file, invalid);
        assert!(
            Bootstrap::from_files(&gateway_file, &store).is_err(),
            "{name}"
        );
    }
    let mut models = serde_json::json!({"models":{google.model:{
        "thinking_tokens":{"minimal":1024,"low":2048,"medium":3072,"high":8192,"xhigh":16384,"max":24576},
        "candidates":[{"binding":google.endpoint().id.as_str(),"thinking_conversion":"effort_to_budget"}]
    }}});
    private_write(&models_file, models.clone());
    let configured = Bootstrap::from_files(&gateway_file, &store).unwrap();
    let (entry, request) = crate::gateway::ingress::admission::prepare(
        &configured.gateway.state, ClientProtocol::Responses,
        &serde_json::to_vec(&serde_json::json!({
            "model":google.model,"input":"hi","reasoning":{"effort":"medium"},"max_output_tokens":6000
        })).unwrap(),&crate::execution::report::Report::default()).unwrap();
    let before = request.clone();
    let (wire, rules) = entry.candidates[0]
        .endpoint
        .encode_request_with_rules(&request)
        .unwrap();
    assert_eq!(
        wire["generationConfig"]["thinkingConfig"]["thinkingLevel"],
        "MEDIUM"
    );
    assert!(
        wire["generationConfig"]["thinkingConfig"]
            .get("thinkingBudget")
            .is_none()
    );
    assert!(!rules.contains(&"configured.effort-to-budget.v1"));
    assert_eq!(request, before);
    assert!(
        matches!(
            entry.candidates[0].endpoint.encode_request(&scoped_request),
            Err(crate::adapter::ConversionFailure { cause: crate::adapter::ConversionCause::Codec(crate::protocol::CodecError::Unsupported(reason)), .. })
                if matches!(reason,crate::protocol::failure::CarrierReason::Reasoning)
        ),
        "an explicit numeric policy replaces, rather than chains with, the scope policy"
    );
    models["models"][google.model]["thinking_tokens"]["medium"] = serde_json::json!(7000);
    private_write(&models_file, models);
    let configured = Bootstrap::from_files(&gateway_file, &store).unwrap();
    let entry = &configured.gateway.state.entries[&(
        crate::gateway::family(Profile::Responses),
        google.model.into(),
    )];
    let wire = entry.candidates[0]
        .endpoint
        .encode_request(&request)
        .unwrap();
    assert_eq!(
        wire["generationConfig"]["thinkingConfig"]["thinkingLevel"],
        "MEDIUM"
    );
    assert_eq!(wire["generationConfig"]["maxOutputTokens"], 6000);

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
