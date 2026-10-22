//! Messages activation, all-native contract publication and owner-local cache admission.
use super::test_support::{Fixture, selected_models, service};
use super::*;
use crate::{
    adapter::{AdapterError, ClientAdapter},
    execution::{AttemptError, admit, plan::representable, prepare},
    lowering::generation::RepresentationError,
    semantic::task::generation::{GenerationFeature, Item, PrefixCacheTtl},
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
    for protocol in [Profile::Chat, Profile::Responses] {
        let key = (crate::gateway::family(protocol), haiku.into());
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
    for protocol in [Profile::Chat, Profile::Responses] {
        let entry = &boot.gateway.state.entries[&(crate::gateway::family(protocol), haiku.into())];
        let endpoint = &entry.candidates[0].endpoint;
        assert_eq!(
            endpoint.protocol,
            crate::topology::ProtocolProfile::AnthropicMessages
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
            crate::gateway::family(Profile::Responses),
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
        let mut request = crate::protocol::openai::Adapter::new(
            Profile::Responses,
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
                1
            );
        } else {
            assert!(representable(&candidate.endpoint, &request).is_err());
            assert!(
                matches!(prepare(&candidate.endpoint, &candidate.provider, &secret, &request),
                Err(AttemptError::Adapter(AdapterError::Codec(crate::protocol::CodecError::Unsupported(value))))
                    if value == "Messages missing target output default")
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
        .find(|endpoint| endpoint.protocol == crate::topology::ProtocolProfile::OpenAiChat)
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
async fn configured_messages_cache_admission_requires_all_native_candidates() {
    let input = cached_tool_result_input();
    for (providers, admitted) in [
        (&["opencode-go"][..], true),
        (&["openrouter"][..], false),
        (&["openrouter", "opencode-go"][..], false),
    ] {
        let boot = configured_messages(providers);
        let state = &boot.gateway.state;
        let entry = &state.entries[&(crate::gateway::MESSAGES_FAMILY, "claude-haiku-5-5".into())];
        assert_eq!(
            entry
                .candidates
                .iter()
                .map(|c| c.provider.id.as_str())
                .collect::<Vec<_>>(),
            providers
        );
        assert!(
            state
                .entries
                .values()
                .all(|entry| entry.public.contract.explicit_cache == admitted)
        );
        assert_eq!(entry.public.contract.automatic_prefix_cache, admitted);
        assert_eq!(entry.public.contract.strict_tools, admitted);
        assert_eq!(entry.public.contract.ordered_instructions, admitted);
        assert_eq!(entry.public.contract.tool_changes, admitted);
        assert!(!entry.public.contract.cached_context);

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
        assert_eq!(
            cache,
            vec![
                json!({"feature":"generation.cache","domain":"valid_owner_local_breakpoints_5m_or_1h",
                "treatment":if admitted {"faithful"} else {"rejected"}}),
                json!({"feature":"generation.cache","domain":"request_wide_automatic_policy",
                    "treatment":if admitted {"faithful"} else {"rejected"}}),
            ]
        );
        assert_eq!(capabilities["provider_effectiveness"], "not_asserted");
        assert_eq!(capabilities["full_loop"], "not_asserted");
        assert!(!capabilities.to_string().contains("opencode-go"));

        let request = ClientAdapter::Messages
            .decode_client_request(&serde_json::to_vec(&input).unwrap())
            .unwrap();
        if !admitted {
            assert!(matches!(
                admit(&entry.public, &request),
                Err(AttemptError::Adapter(AdapterError::Representation(
                    RepresentationError::Admission(GenerationFeature::ExplicitCache)
                )))
            ));
            // pi's agent profile follows the same all-native rule.
            let strict = json!({"model":"claude-haiku-5-5","max_tokens":64,
                "tools":[{"name":"lookup","strict":true,
                    "input_schema":{"type":"object","additionalProperties":false}}],
                "messages":[{"role":"user","content":"lookup"}]});
            let strict = ClientAdapter::Messages
                .decode_client_request(&serde_json::to_vec(&strict).unwrap())
                .unwrap();
            assert!(matches!(
                admit(&entry.public, &strict),
                Err(AttemptError::Adapter(AdapterError::Representation(
                    RepresentationError::Admission(GenerationFeature::StrictTools)
                )))
            ));
            assert!(
                crate::gateway::ingress::admission::prepare_messages(
                    state,
                    &serde_json::to_vec(&input).unwrap()
                )
                .is_err()
            );
            continue;
        }
        for stream in [false, true] {
            let mut body = input.clone();
            body["stream"] = json!(stream);
            let (_, mut request) = crate::gateway::ingress::admission::prepare_messages(
                state,
                &serde_json::to_vec(&body).unwrap(),
            )
            .unwrap();
            assert_eq!(
                request.task.semantic.settings().automatic_prefix_cache,
                None
            );
            let Item::Instruction(system) = &request.task.semantic.items()[0].1 else {
                panic!("system")
            };
            assert_eq!(
                system.parts.iter().map(|p| p.cache).collect::<Vec<_>>(),
                [
                    Some(PrefixCacheTtl::OneHour),
                    Some(PrefixCacheTtl::FiveMinutes)
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
            assert_eq!(wire["stream"], stream);
            assert_eq!(wire["max_tokens"], 64);
            assert_eq!(request, original);
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
            assert_eq!(wire["cache_control"], json!({"type":"ephemeral"}));
        }
    }
}

#[test]
fn native_response_admit_strict_changed_tools_and_message_effort() {
    let boot = configured_messages(&["opencode-go"]);
    let state = &boot.gateway.state;
    let entry = &state.entries[&(crate::gateway::MESSAGES_FAMILY, "claude-haiku-5-5".into())];
    // The same configured admission must accept pi's complete native profile,
    // not only a plain tool plus cache markers.
    let mut rich = cached_tool_result_input();
    rich["tools"][0]["strict"] = json!(true);
    rich["tools"][0]["eager_input_streaming"] = json!(true);
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
    let (_, mut request) = crate::gateway::ingress::admission::prepare_messages(
        state,
        &serde_json::to_vec(&rich).unwrap(),
    )
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

#[tokio::test]
async fn configured_messages_input_features_require_every_native_candidate() {
    let schema = json!({"type":"object","properties":{"key":{"type":"string"}},"required":["key"]});
    let root = json!({"name":"lookup","strict":false,"input_schema":schema,
        "input_examples":[{"key":"cedar"},{"key":"birch"}]});
    let inline = json!({"name":"inspect","strict":false,"input_schema":schema,"input_examples":[]});
    let examples = json!({"model":"claude-haiku-5-5","max_tokens":64,"tools":[root],
        "messages":[{"role":"user","content":[{"type":"text","text":"lookup"}]},
        {"role":"system","content":[{"type":"tool_addition","tool":{
            "type":"tool_definition","definition":inline}}]}]});
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
        (
            GenerationFeature::ToolExamples,
            "bounded_ordered_input_examples",
            examples,
        ),
        (
            GenerationFeature::DocumentContext,
            "per_use_presence_in_documents_and_tool_results",
            context,
        ),
        (
            GenerationFeature::AutomaticPrefixCache,
            "request_wide_automatic_policy",
            automatic,
        ),
    ];
    for (providers, admitted) in [
        (&["opencode-go"][..], true),
        (&["openrouter"][..], false),
        (&["openrouter", "opencode-go"][..], false),
    ] {
        let boot = configured_messages(providers);
        let state = &boot.gateway.state;
        assert!(
            state
                .entries
                .values()
                .all(|entry| entry.public.contract.tool_examples == admitted
                    && entry.public.contract.document_context == admitted
                    && entry.public.contract.automatic_prefix_cache == admitted)
        );
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
        let view: Value = serde_json::from_slice(&bytes).unwrap();
        let messages = view["data"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["protocol"] == "messages")
            .unwrap();
        let entry = &state.entries[&(crate::gateway::MESSAGES_FAMILY, "claude-haiku-5-5".into())];
        for (feature, domain, input) in &cases {
            let case = messages["request"]
                .as_array()
                .unwrap()
                .iter()
                .find(|case| case["domain"] == *domain)
                .unwrap();
            assert_eq!(
                case["treatment"],
                if admitted { "faithful" } else { "rejected" }
            );
            let decoded = ClientAdapter::Messages
                .decode_client_request(&serde_json::to_vec(input).unwrap())
                .unwrap();
            if !admitted {
                assert!(matches!(admit(&entry.public, &decoded),
                    Err(AttemptError::Adapter(AdapterError::Representation(
                        RepresentationError::Admission(found)))) if found == *feature));
                assert!(
                    crate::gateway::ingress::admission::prepare_messages(
                        state,
                        &serde_json::to_vec(input).unwrap()
                    )
                    .is_err()
                );
                continue;
            }
            let (_, mut request) = crate::gateway::ingress::admission::prepare_messages(
                state,
                &serde_json::to_vec(input).unwrap(),
            )
            .unwrap();
            request.conversation = Some(
                crate::semantic::context::ConversationContext::conversation("synthetic").unwrap(),
            );
            let original = request.clone();
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
            assert_eq!(wire["tools"], input["tools"]);
            assert_eq!(wire["messages"], input["messages"]);
            if let Some(control) = input.get("cache_control") {
                assert_eq!(&wire["cache_control"], control);
            }
            assert_eq!(request, original);
        }
    }
}
