//! Messages activation, all-native contract publication and owner-local cache admission.
use super::{
    test_support::{Fixture, selected_models, service},
    *,
};
use crate::{
    adapter::ClientAdapter,
    execution::{AttemptError, admit, plan::representable, prepare},
    protocol::{ClientProtocol, ConversionFailure, failure::CarrierReason},
    semantic::task::generation::{Feature, Item, PrefixCacheBoundary, PrefixCacheTtl},
};
use serde_json::{Value, json};
use tower::ServiceExt;

#[test]
fn native_response_binding_activates_only_when_listed() {
    // Both protocol families share the same public contract; activation remains explicit.
    let credentials = [(
        "opencode-go",
        "opencode-go-api-key",
        "synthetic-upstream-only",
    )];
    let haiku = "claude-haiku-5-5";
    let boot = Fixture::new(
        service(),
        selected_models(&["muse-spark-1.3-contributor"], &["opencode-go"]),
        &credentials,
    )
    .unwrap();
    for protocol in [GenerationApi::Chat, GenerationApi::Responses] {
        let key = (
            crate::protocol::ClientProtocol::from(protocol),
            haiku.into(),
        );
        assert!(!boot.gateway.state.entries.contains_key(&key));
    }
    let boot = Fixture::new(
        service(),
        selected_models(&[haiku], &["opencode-go"]),
        &credentials,
    )
    .unwrap();
    // Chat, Responses and Messages clients.
    assert_eq!(boot.gateway.state.entries.len(), 3);
    for protocol in [GenerationApi::Chat, GenerationApi::Responses] {
        let entry = &boot.gateway.state.entries[&(
            crate::protocol::ClientProtocol::from(protocol),
            haiku.into(),
        )];
        let endpoint = &entry.candidates[0].endpoint;
        assert_eq!(
            endpoint.protocol,
            crate::topology::GenerationProtocol::AnthropicMessages
        );
        assert_eq!(endpoint.target.path.as_str(), "/zen/go/v1/messages");
    }
    let mixed = Fixture::new(
        service(),
        selected_models(&[haiku], &["opencode-go", "openrouter"]),
        &[
            credentials[0],
            ("openrouter", "openrouter-api-key", "synthetic-router-key"),
        ],
    )
    .unwrap();
    let body = br#"{"model":"claude-haiku-5-5","max_tokens":64,"messages":[
        {"role":"user","content":"read the file"},
        {"role":"assistant","content":[{"type":"tool_use","id":"call-1","name":"read","input":{}}]},
        {"role":"user","content":[{"type":"tool_result","tool_use_id":"call-1","content":"failed","is_error":true}]}
    ]}"#;
    let (_, native_request) = crate::gateway::ingress::admission::prepare(
        &boot.gateway.state,
        ClientProtocol::Messages,
        body,
        &crate::execution::report::Report::default(),
    )
    .unwrap();
    let (entry, mut request) = crate::gateway::ingress::admission::prepare(
        &mixed.gateway.state,
        ClientProtocol::Messages,
        body,
        &crate::execution::report::Report::default(),
    )
    .unwrap();
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
        if candidate.endpoint.protocol == crate::topology::GenerationProtocol::AnthropicMessages {
            assert_eq!(wire["messages"][2]["content"][0]["is_error"], true);
            assert!(!rules.contains(&"openai.wrap-tool-result-errors.v1"));
        } else {
            assert_eq!(
                wire["messages"][2]["content"],
                "[tool result is_error=true]\nfailed"
            );
            assert!(rules.contains(&"openai.wrap-tool-result-errors.v1"));
        }
    }
    assert_eq!(
        entry
            .eligible(&request, &mixed.gateway.state.affinity)
            .unwrap()
            .len(),
        2
    );
    assert_eq!(request, original);
}

fn configured_messages(providers: &[&str]) -> Fixture {
    let models = selected_models(&["claude-haiku-5-5"], providers);
    Fixture::new(
        service(),
        models,
        &[
            ("opencode-go", "opencode-go-api-key", "synthetic-upstream"),
            ("openrouter", "openrouter-api-key", "synthetic-upstream"),
        ],
    )
    .unwrap()
}

#[test]
fn configured_messages_adaptive_forced_tools_keep_strictness_and_effort() {
    let boot = configured_messages(&["opencode-go"]);
    let schema = json!({"type":"object","properties":{"record":{"type":"integer","enum":[71,82]}},
        "required":["record"],"additionalProperties":false});
    let tools: Vec<Value> = ["lookup", "inspect"].into_iter().map(|name| {
        json!({"name":name,"description":"Retrieve one synthetic record.","strict":true,"input_schema":schema})
    }).collect();
    let mut body = json!({"model":"claude-haiku-5-5","max_tokens":2048,"stream":false,
        "thinking":{"type":"adaptive","display":"summarized"},
        "output_config":{"effort":"high"},"tools":tools,
        "messages":[{"role":"user","content":[{"type":"text","text":"Return record 71."}]}]});
    for choice in [
        json!({"type":"any","disable_parallel_tool_use":true}),
        json!({"type":"tool","name":"inspect","disable_parallel_tool_use":true}),
    ] {
        body["tool_choice"] = choice.clone();
        let (entry, mut request) = crate::gateway::ingress::admission::prepare(
            &boot.gateway.state,
            ClientProtocol::Messages,
            body.to_string().as_bytes(),
            &crate::execution::report::Report::default(),
        )
        .unwrap();
        request.conversation =
            Some(crate::semantic::context::ConversationContext::conversation("synthetic").unwrap());
        let candidate = &entry.candidates[0];
        let secret = candidate.secret.resolve().unwrap().0;
        let (prepared, _) = crate::execution::prepare_with_rules(
            &candidate.endpoint,
            &candidate.provider,
            &secret,
            &request,
        )
        .unwrap();
        let wire: Value = serde_json::from_slice(&prepared.body).unwrap();
        assert_eq!(wire["tool_choice"], choice);
        assert_eq!(
            entry
                .eligible(&request, &boot.gateway.state.affinity)
                .unwrap()
                .len(),
            1
        );
        assert_eq!(wire["thinking"], body["thinking"]);
        assert_eq!(wire["output_config"], json!({"effort":"high"}));
        assert_eq!(wire["tools"], json!(tools));
        assert_eq!(wire["messages"], body["messages"]);
        let mut unsupported = body.clone();
        unsupported["tools"][0]["input_schema"]["properties"]["record"] =
            json!({"type":"string","enum":["cedar","birch"]});
        let (_, mut rejected) = crate::gateway::ingress::admission::prepare(
            &boot.gateway.state,
            ClientProtocol::Messages,
            unsupported.to_string().as_bytes(),
            &crate::execution::report::Report::default(),
        )
        .unwrap();
        rejected.conversation = request.conversation.clone();
        let error = crate::execution::prepare_with_rules(
            &candidate.endpoint,
            &candidate.provider,
            &secret,
            &rejected,
        )
        .unwrap_err();
        assert!(
            matches!(error, AttemptError::Conversion(ConversionFailure { cause: crate::protocol::ConversionCause::Codec(
            crate::protocol::CodecError::Unsupported(reason)), .. }) if reason == CarrierReason::Schema),
            "{error:?}"
        );
    }
    body.as_object_mut().unwrap().remove("tools");
    body.as_object_mut().unwrap().remove("tool_choice");
    body["output_config"]["format"] = json!({"type":"json_schema","schema":{
        "type":"object","properties":{"record":{"type":"integer","enum":[71,82]},"ok":{"type":"boolean"}},
        "required":["record","ok"],"additionalProperties":false}});
    let (entry, mut request) = crate::gateway::ingress::admission::prepare(
        &boot.gateway.state,
        ClientProtocol::Messages,
        body.to_string().as_bytes(),
        &crate::execution::report::Report::default(),
    )
    .unwrap();
    request.conversation =
        Some(crate::semantic::context::ConversationContext::conversation("synthetic").unwrap());
    let candidate = &entry.candidates[0];
    let secret = candidate.secret.resolve().unwrap().0;
    let (prepared, _) = crate::execution::prepare_with_rules(
        &candidate.endpoint,
        &candidate.provider,
        &secret,
        &request,
    )
    .unwrap();
    let wire: Value = serde_json::from_slice(&prepared.body).unwrap();
    assert_eq!(wire["output_config"], body["output_config"]);
}

#[test]
fn messages_output_defaults_inherit_override_or_clear_without_widening_maxima() {
    let load = |mut candidate: Value| {
        if candidate.get("binding").is_none() {
            candidate["binding"] = json!("opencode-go-haiku-messages");
        }
        let models = json!({"models":{"claude-haiku-5-5":{"candidates":[candidate]}}});
        Fixture::new(
            service(),
            models,
            &[
                ("opencode-go", "opencode-go-api-key", "synthetic-upstream"),
                ("openrouter", "openrouter-api-key", "synthetic-upstream"),
            ],
        )
    };
    for (configured, expected) in [
        (json!({}), Some(32_000)),
        (json!({"default_output_tokens":64_000}), Some(64_000)),
        (json!({"default_output_tokens":null}), None),
        (
            json!({"maximum_output_tokens":4096,"default_output_tokens":4096}),
            Some(4096),
        ),
        (
            json!({"maximum_output_tokens":4096,"default_output_tokens":null}),
            None,
        ),
    ] {
        let boot = load(configured).unwrap();
        let entry = &boot.gateway.state.entries[&(
            crate::protocol::ClientProtocol::Responses,
            "claude-haiku-5-5".into(),
        )];
        let candidate = &entry.candidates[0];
        assert_eq!(
            candidate
                .endpoint
                .representation
                .messages()
                .unwrap()
                .default_output_tokens,
            expected
        );
        let mut request = crate::protocol::openai::GenerationCodec::new(
            GenerationApi::Responses,
            crate::protocol::openai::Dialect::Standard,
            None,
        )
        .decode_request(br#"{"model":"claude-haiku-5-5","input":"go"}"#)
        .unwrap();
        request.conversation = Some(
            crate::semantic::context::ConversationContext::conversation("synthetic-defaults")
                .unwrap(),
        );
        let before = request.clone();
        let secret = candidate.secret.resolve().unwrap().0;
        if let Some(default) = expected {
            representable(&candidate.endpoint, &request).unwrap();
            let (prepared, rules) = crate::execution::prepare_with_rules(
                &candidate.endpoint,
                &candidate.provider,
                &secret,
                &request,
            )
            .unwrap();
            let wire: Value = serde_json::from_slice(&prepared.body).unwrap();
            assert_eq!(wire["max_tokens"], default);
            assert_eq!(
                rules
                    .iter()
                    .filter(|rule| **rule == "messages.default-max-tokens.v1")
                    .count(),
                0
            );
        } else {
            assert!(representable(&candidate.endpoint, &request).is_err());
            assert!(
                matches!(prepare(&candidate.endpoint, &candidate.provider, &secret, &request),
                Err(AttemptError::Conversion(ConversionFailure { cause: crate::protocol::ConversionCause::Codec(crate::protocol::CodecError::Unsupported(value)), .. }))
                    if value == CarrierReason::Unavailable)
            );
        }
        assert_eq!(request, before);
        let mut capped = request.clone();
        let mut settings = capped.task.semantic.settings().clone();
        settings.controls.max_output_tokens = Some(128);
        capped.task.semantic = capped.task.semantic.with_settings(settings).unwrap();
        let (prepared, rules) = crate::execution::prepare_with_rules(
            &candidate.endpoint,
            &candidate.provider,
            &secret,
            &capped,
        )
        .unwrap();
        let wire: Value = serde_json::from_slice(&prepared.body).unwrap();
        assert_eq!(wire["max_tokens"], 128);
        assert!(!rules.contains(&"messages.default-max-tokens.v1"));
    }
    for configured in [
        json!({"maximum_output_tokens":4096}),
        json!({"maximum_output_tokens":4096,"default_output_tokens":4097}),
        json!({"default_output_tokens":0}),
        json!({"default_output_tokens":-1}),
        json!({"default_output_tokens":1.5}),
        json!({"default_output_tokens":1e100}),
        json!({"default_output_tokens":"4096"}),
        json!({"default_output_tokens":true}),
    ] {
        assert!(matches!(
            load(configured),
            Err(StartupError::Configuration("models"))
        ));
    }
    let catalog = catalog::default_topology().unwrap();
    let public = catalog.model("claude-haiku-5-5").unwrap();
    let chat = catalog
        .route_endpoints(&public.route)
        .into_iter()
        .find(|endpoint| endpoint.protocol == crate::topology::GenerationProtocol::OpenAiChat)
        .unwrap();
    for value in [Value::Null, json!(4096)] {
        assert!(matches!(
            load(json!({"binding":chat.id.as_str(),"default_output_tokens":value})),
            Err(StartupError::Configuration("models"))
        ));
    }
}

fn cached_tool_result_input() -> Value {
    json!({
        "model":"claude-haiku-5-5","max_tokens":64,
        "tools":[{"name":"lookup","strict":false,"input_schema":{"type":"object"},
            "cache_control":{"type":"ephemeral","ttl":"1h"}}],
        "system":[
            {"type":"text","text":"stable","cache_control":{"type":"ephemeral","ttl":"1h"}},
            {"type":"text","text":"changing","cache_control":{"type":"ephemeral","ttl":"5m"}}
        ],
        "messages":[
            {"role":"user","content":"lookup"},
            {"role":"assistant","content":[{"type":"tool_use","id":"call","name":"lookup","input":{}}]},
            {"role":"user","content":[{"type":"tool_result","tool_use_id":"call","content":"found",
                "cache_control":{"type":"ephemeral"}}]}
        ]
    })
}

#[tokio::test]
async fn configured_messages_cache_projects_each_candidate_without_mutating_owners() {
    let input = cached_tool_result_input();
    for providers in [
        &["opencode-go"][..],
        &["openrouter"][..],
        &["openrouter", "opencode-go"][..],
    ] {
        let boot = configured_messages(providers);
        let state = &boot.gateway.state;
        let entry = &state.entries[&(
            crate::protocol::ClientProtocol::Messages,
            "claude-haiku-5-5".into(),
        )];
        assert_eq!(
            entry
                .candidates
                .iter()
                .map(|c| c.provider.id.as_str())
                .collect::<Vec<_>>(),
            providers
        );
        for candidate in &entry.candidates {
            assert_eq!(
                candidate
                    .endpoint
                    .representation
                    .dispositions()
                    .native(Feature::ExplicitCache),
                candidate.provider.id.as_str() == "opencode-go"
            );
        }
        let response = boot
            .gateway
            .router()
            .oneshot(
                axum::http::Request::builder()
                    .uri("/v1/morphiecore/capabilities")
                    .header(
                        "authorization",
                        "Bearer synthetic-gateway-client-token-0001",
                    )
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let bytes = axum::body::to_bytes(response.into_body(), 32 << 10)
            .await
            .unwrap();
        let capabilities: Value = serde_json::from_slice(&bytes).unwrap();
        let messages = capabilities["data"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["protocol"] == "messages")
            .unwrap();
        let cache: Vec<_> = messages["request"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|case| case["feature"] == "generation.cache")
            .cloned()
            .collect();
        assert!(cache.is_empty());
        assert_eq!(capabilities["provider_effectiveness"], "not_asserted");
        assert_eq!(capabilities["full_loop"], "not_asserted");
        assert!(!capabilities.to_string().contains("opencode-go"));

        let request = ClientAdapter::Messages
            .decode_client_request(&serde_json::to_vec(&input).unwrap())
            .unwrap();
        admit(&entry.public, &request).unwrap();
        let strict = json!({"model":"claude-haiku-5-5","max_tokens":64,
            "tools":[{"name":"lookup","strict":true,"input_schema":{"type":"object","additionalProperties":false}}],
            "messages":[{"role":"user","content":"lookup"}]});
        let mut strict = ClientAdapter::Messages
            .decode_client_request(&serde_json::to_vec(&strict).unwrap())
            .unwrap();
        admit(&entry.public, &strict).unwrap();
        strict.conversation =
            Some(crate::semantic::context::ConversationContext::conversation("synthetic").unwrap());
        for candidate in &entry.candidates {
            assert_eq!(
                representable(&candidate.endpoint, &strict).is_ok(),
                candidate.provider.id.as_str() == "opencode-go"
            );
        }
        for stream in [false, true] {
            let mut body = input.clone();
            body["stream"] = json!(stream);
            let (_, mut request) = crate::gateway::ingress::admission::prepare(
                state,
                ClientProtocol::Messages,
                &serde_json::to_vec(&body).unwrap(),
                &crate::execution::report::Report::default(),
            )
            .unwrap();
            assert_eq!(
                request.task.semantic.settings().automatic_prefix_cache,
                None
            );
            let system = request.task.semantic.instructions().value().unwrap();
            assert_eq!(
                system.parts.iter().map(|p| p.cache).collect::<Vec<_>>(),
                [
                    Some(PrefixCacheBoundary::Ttl(PrefixCacheTtl::OneHour)),
                    Some(PrefixCacheBoundary::Ttl(PrefixCacheTtl::FiveMinutes))
                ]
            );
            let Item::ToolResult(result) = &request.task.semantic.items().last().unwrap().1 else {
                panic!("result")
            };
            assert_eq!(result.cache, Some(PrefixCacheTtl::FiveMinutes));
            request.conversation = Some(
                crate::semantic::context::ConversationContext::conversation("synthetic").unwrap(),
            );
            let original = request.clone();
            for candidate in &entry.candidates {
                representable(&candidate.endpoint, &request).unwrap();
                let prepared = prepare(
                    &candidate.endpoint,
                    &candidate.provider,
                    &candidate.secret.resolve().unwrap().0,
                    &request,
                )
                .unwrap();
                let wire: Value = serde_json::from_slice(&prepared.body).unwrap();
                if candidate.provider.id.as_str() != "opencode-go" {
                    assert!(!wire.to_string().contains("cache_control"));
                    let (_, rules) = candidate
                        .endpoint
                        .encode_request_with_rules(&request)
                        .unwrap();
                    assert!(rules.contains(&"generation.omit-explicit-cache.v1"));
                    assert_eq!(request, original);
                    continue;
                }
                assert!(wire.get("cache_control").is_none());
                assert_eq!(
                    wire["tools"],
                    json!([{"name":"lookup","strict":false,
                "input_schema":{"type":"object"},"cache_control":{"type":"ephemeral","ttl":"1h"}}])
                );
                assert_eq!(
                    wire["system"],
                    json!([
                        {"type":"text","text":"stable","cache_control":{"type":"ephemeral","ttl":"1h"}},
                        {"type":"text","text":"changing","cache_control":{"type":"ephemeral"}}
                    ])
                );
                assert_eq!(
                    wire["messages"],
                    json!([
                        {"role":"user","content":[{"type":"text","text":"lookup"}]},
                        {"role":"assistant","content":[{"type":"tool_use","id":"call","name":"lookup","input":{}}]},
                        {"role":"user","content":[{"type":"tool_result","tool_use_id":"call","content":"found",
                            "cache_control":{"type":"ephemeral"}}]}
                    ])
                );
                assert_eq!(wire["stream"], true);
                assert_eq!(wire["max_tokens"], 64);
                assert_eq!(request, original);
            }
        }
        // Caller-selected automatic policy preserves the existing same-TTL tail.
        {
            let mut body = input.clone();
            body["system"].as_array_mut().unwrap().pop();
            body["cache_control"] = json!({"type":"ephemeral"});
            let mut request = ClientAdapter::Messages
                .decode_client_request(&serde_json::to_vec(&body).unwrap())
                .unwrap();
            admit(&entry.public, &request).unwrap();
            request.conversation = Some(
                crate::semantic::context::ConversationContext::conversation("synthetic").unwrap(),
            );
            for candidate in &entry.candidates {
                representable(&candidate.endpoint, &request).unwrap();
                let prepared = prepare(
                    &candidate.endpoint,
                    &candidate.provider,
                    &candidate.secret.resolve().unwrap().0,
                    &request,
                )
                .unwrap();
                let wire: Value = serde_json::from_slice(&prepared.body).unwrap();
                if candidate.provider.id.as_str() == "opencode-go" {
                    assert_eq!(wire["cache_control"], json!({"type":"ephemeral"}));
                } else {
                    assert!(!wire.to_string().contains("cache_control"));
                    let (_, rules) = candidate
                        .endpoint
                        .encode_request_with_rules(&request)
                        .unwrap();
                    assert!(rules.contains(&"generation.omit-explicit-cache.v1"));
                    assert!(rules.contains(&"generation.omit-automatic-prefix-cache.v1"));
                }
            }
        }
    }
}

#[test]
fn native_binding_keeps_beta_carriers_without_public_activation() {
    let boot = configured_messages(&["opencode-go"]);
    let state = &boot.gateway.state;
    let entry = &state.entries[&(
        crate::protocol::ClientProtocol::Messages,
        "claude-haiku-5-5".into(),
    )];
    // Native carrier support does not authorize beta semantics on a stable Client.
    let mut rich = cached_tool_result_input();
    rich["tools"][0]["strict"] = json!(true);
    rich["tools"][0]["eager_input_streaming"] = json!(true);
    rich["tools"][0]["input_schema"]["properties"] = json!({});
    rich["tools"][0]["input_schema"]["required"] = json!([]);
    rich["tools"][0]["input_schema"]["additionalProperties"] = json!(false);
    rich["tools"].as_array_mut().unwrap().push(json!({
        "name":"__pi_deferred_placeholder__","input_schema":{"type":"object"},
        "defer_loading":true
    }));
    rich["thinking"] = json!({"type":"adaptive","display":"summarized",
        "block_binding":{"prefix_mismatch_behavior":"drop_block"}});
    rich["output_config"] = json!({"effort":"high"});
    rich["messages"].as_array_mut().unwrap().extend([
        json!({"role":"assistant","content":"done"}),
        json!({"role":"user","content":"inspect next"}),
        json!({"role":"system","content":[
            {"type":"text","text":"Inspect mode."},
            {"type":"tool_removal","tool":{"type":"tool_reference","name":"lookup"}},
            {"type":"tool_addition","tool":{"type":"tool_definition","definition":{
                "name":"inspect","input_schema":{"type":"object"},
                "eager_input_streaming":true}}}
        ],"output_config":{"effort":"low"}}),
    ]);
    assert!(
        crate::gateway::ingress::admission::prepare(
            state,
            ClientProtocol::Messages,
            &serde_json::to_vec(&rich).unwrap(),
            &crate::execution::report::Report::default(),
        )
        .is_err()
    );
    let native = crate::protocol::anthropic::messages::Profile::AdaptiveTextTools
        .decode_generation_request(
            &serde_json::to_vec(&rich).unwrap(),
            crate::semantic::task::generation::LocalScope::ROOT,
            &crate::protocol::native::ReplayTarget::new("synthetic", "claude-haiku-5-5").unwrap(),
        )
        .unwrap();
    let mut request =
        crate::protocol::GenerationRequestEnvelope::from_generation(native, "claude-haiku-5-5")
            .unwrap();
    request.conversation =
        Some(crate::semantic::context::ConversationContext::conversation("synthetic").unwrap());
    let candidate = &entry.candidates[0];
    representable(&candidate.endpoint, &request).unwrap();
    let prepared = prepare(
        &candidate.endpoint,
        &candidate.provider,
        &candidate.secret.resolve().unwrap().0,
        &request,
    )
    .unwrap();
    let wire: Value = serde_json::from_slice(&prepared.body).unwrap();
    assert_eq!(wire["thinking"], rich["thinking"]);
    assert_eq!(wire["output_config"], rich["output_config"]);
    assert_eq!(wire["tools"], rich["tools"]);
    assert_eq!(
        wire["messages"].as_array().unwrap().last(),
        rich["messages"].as_array().unwrap().last()
    );
    assert!(wire.get("cache_control").is_none());
    let beta = prepared
        .safe_headers
        .iter()
        .find(|(name, _)| name == "anthropic-beta")
        .unwrap();
    assert_eq!(
        beta.1,
        "mid-conversation-output-config-2026-07-01,thinking-binding-controls-2026-08-01,inline-tools-2026-09-15"
    );
}

#[test]
fn configured_messages_input_features_keep_output_guarantees_candidate_local() {
    let schema = json!({"type":"object","properties":{"key":{"type":"string"}},"required":["key"]});
    let root = json!({"name":"lookup","strict":false,"input_schema":schema,
        "input_examples":[{"key":"cedar"},{"key":"birch"}]});
    let examples = json!({"model":"claude-haiku-5-5","max_tokens":64,"tools":[root],
        "messages":[{"role":"user","content":[{"type":"text","text":"lookup"}]}]});
    let context = json!({"model":"claude-haiku-5-5","max_tokens":64,
    "messages":[{"role":"user","content":[
        {"type":"document","source":{"type":"text","media_type":"text/plain","data":"cedar"},
            "context":"first use"},
        {"type":"document","source":{"type":"text","media_type":"text/plain","data":"cedar"},
            "context":"second use"}
    ]}]});
    let automatic = json!({"model":"claude-haiku-5-5","max_tokens":64,
        "cache_control":{"type":"ephemeral","ttl":"1h"},
        "messages":[{"role":"user","content":[{"type":"text","text":"lookup"}]}]});
    let cases = [
        (Feature::ToolExamples, examples),
        (Feature::DocumentContext, context),
        (Feature::AutomaticPrefixCache, automatic),
    ];
    for providers in [
        &["opencode-go"][..],
        &["openrouter"][..],
        &["openrouter", "opencode-go"][..],
    ] {
        let boot = configured_messages(providers);
        let state = &boot.gateway.state;
        let entry = &state.entries[&(ClientProtocol::Messages, "claude-haiku-5-5".into())];
        for (feature, input) in &cases {
            let (_, mut request) = crate::gateway::ingress::admission::prepare(
                state,
                ClientProtocol::Messages,
                &serde_json::to_vec(input).unwrap(),
                &crate::execution::report::Report::default(),
            )
            .unwrap();
            admit(&entry.public, &request).unwrap();
            request.conversation = Some(
                crate::semantic::context::ConversationContext::conversation("synthetic").unwrap(),
            );
            let original = request.clone();
            let mut expected = vec![];
            for candidate in &entry.candidates {
                let native = candidate.provider.id.as_str() == "opencode-go";
                if !native && *feature != Feature::AutomaticPrefixCache {
                    assert_eq!(
                        representable(&candidate.endpoint, &request)
                            .unwrap_err()
                            .code(),
                        if *feature == Feature::ToolExamples {
                            "admission.tool_examples"
                        } else {
                            "admission.document_context"
                        }
                    );
                    assert!(
                        prepare(
                            &candidate.endpoint,
                            &candidate.provider,
                            &candidate.secret.resolve().unwrap().0,
                            &request
                        )
                        .is_err()
                    );
                    continue;
                }
                expected.push(candidate.provider.id.as_str());
                representable(&candidate.endpoint, &request).unwrap();
                let prepared = prepare(
                    &candidate.endpoint,
                    &candidate.provider,
                    &candidate.secret.resolve().unwrap().0,
                    &request,
                )
                .unwrap();
                let wire: Value = serde_json::from_slice(&prepared.body).unwrap();
                if native {
                    assert_eq!(wire["tools"], input["tools"]);
                    assert_eq!(wire["messages"], input["messages"]);
                    if let Some(control) = input.get("cache_control") {
                        assert_eq!(&wire["cache_control"], control);
                    }
                } else {
                    assert!(!wire.to_string().contains("cache_control"));
                    let (_, rules) = candidate
                        .endpoint
                        .encode_request_with_rules(&request)
                        .unwrap();
                    assert_eq!(rules, ["generation.omit-automatic-prefix-cache.v1"]);
                }
                assert_eq!(request, original);
            }
            let selected = entry.eligible(&request, &state.affinity);
            if expected.is_empty() {
                assert!(selected.is_err());
            } else {
                assert_eq!(
                    selected
                        .unwrap()
                        .iter()
                        .map(|c| c.candidate.provider.id.as_str())
                        .collect::<Vec<_>>(),
                    expected
                );
            }
        }
    }
}
