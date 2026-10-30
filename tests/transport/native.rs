//! Native Messages bindings, catalog activation and preparation.
mod native_bindings {
    //! Operation binding and pure preparation; never opens a socket or reads a credential store.
    use morphiecore::{
        adapter::UpstreamAdapter,
        execution::{Attempt, UpstreamRequest, plan::representable, prepare},
        protocol::{
            GenerationRequestEnvelope as Request,
            anthropic::messages::{
                Dialect as NativeDialect, Profile as NativeProfile, TargetContract,
            },
            native::ReplayTarget,
            openai::{Dialect, GenerationApi, GenerationCodec, TargetContract as Contract},
            stream::SseLimits,
        },
        provider::{
            AuthScheme, CredentialBindingId, CredentialKind, EndpointPath, ProviderDefinition,
            ProviderId, SecretMaterial, TrustedOrigin,
        },
        semantic::{
            context::ConversationContext,
            task::generation::{GenerationFeatureSet, LocalScope},
            value::Presence,
        },
        topology::*,
    };
    use serde_json::{Value, json};

    const HELLO: &[u8] =
    br#"{"model":"synthetic-model","max_tokens":32,"messages":[{"role":"user","content":"hello"}]}"#;

    fn provider() -> ProviderDefinition {
        ProviderDefinition {
            id: ProviderId::new("synthetic").unwrap(),
            origin: TrustedOrigin::parse("https://synthetic.invalid").unwrap(),
            endpoints: [
                (
                    morphiecore::provider::ProviderOperation::ChatCompletions,
                    morphiecore::provider::ProviderEndpoint {
                        path: EndpointPath::new("/chat/completions").unwrap(),
                        auth: None,
                    },
                ),
                (
                    morphiecore::provider::ProviderOperation::Messages,
                    morphiecore::provider::ProviderEndpoint {
                        path: EndpointPath::new("/messages").unwrap(),
                        auth: Some(AuthScheme::ApiKeyHeader("x-api-key")),
                    },
                ),
            ]
            .into(),
            credential_pool: None,
            auth: AuthScheme::Bearer,
        }
    }
    fn endpoint(protocol: GenerationProtocol) -> Endpoint {
        let p = provider();
        let chat = protocol == GenerationProtocol::OpenAiChat;
        Endpoint {
            id: EndpointId::new(if chat { "chat" } else { "messages" }).unwrap(),
            provider: p.id.clone(),
            target: EndpointTarget {
                origin: p.origin,
                path: EndpointPath::new(if chat {
                    "/chat/completions"
                } else {
                    "/messages"
                })
                .unwrap(),
            },
            task: TaskKind::Generation,
            protocol,
            upstream_model: "synthetic-model".into(),
            canonical_model: ModelId::new("synthetic-model").unwrap(),
            representation: if chat {
                GenerationCodec::new(GenerationApi::Chat, Dialect::OpenCodeGo, None)
                    .contract(&Contract::full())
                    .into()
            } else {
                TargetContract::new(NativeProfile::AdaptiveTextTools, NativeDialect::OpenCodeGo)
                    .into()
            },
            execution: ExecutionContract {
                streaming: false,
                retry_before_commit: false,
                request_body_limit: 4096,
                response_body_limit: 4096,
                credential_kind: CredentialKind::ApiKey,
                timeout_ms: 1000,
            },
            credential: CredentialBindingId::new("synthetic-key").unwrap(),
        }
    }
    fn standard_native() -> Endpoint {
        let mut native = endpoint(GenerationProtocol::AnthropicMessages);
        native.representation =
            TargetContract::new(NativeProfile::AdaptiveTextTools, NativeDialect::Standard).into();
        native
    }
    fn compile_endpoints(
        p: ProviderDefinition,
        endpoints: Vec<Endpoint>,
    ) -> Result<CompiledTopology, TopologyError> {
        compile(
            vec![p],
            endpoints,
            vec![],
            vec![],
            vec![CanonicalModel {
                id: ModelId::new("synthetic-model").unwrap(),
                task: TaskKind::Generation,
                contract: GenerationFeatureSet::full(),
            }],
        )
    }
    fn native_request() -> Request {
        let target = ReplayTarget::new("synthetic", "synthetic-model").unwrap();
        let decoded = NativeProfile::AdaptiveTextTools
            .decode_generation_request(HELLO, LocalScope::new(1), &target)
            .unwrap();
        Request::from_generation(decoded, "public").unwrap()
    }
    fn secret() -> SecretMaterial {
        SecretMaterial::new("synthetic-secret").unwrap()
    }
    fn body(prepared: &UpstreamRequest) -> Value {
        serde_json::from_slice(&prepared.body).unwrap()
    }
    fn header<'a>(prepared: &'a UpstreamRequest, name: &str) -> Option<&'a str> {
        prepared
            .safe_headers
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }
    fn hello_wire() -> Value {
        json!({"model":"synthetic-model","max_tokens":32,
            "messages":[{"role":"user","content":[{"type":"text","text":"hello"}]}]})
    }

    #[test]
    fn compilation_rejects_contracts_the_native_slice_cannot_keep() {
        let native = endpoint(GenerationProtocol::AnthropicMessages);
        let route = Route {
            id: RouteId::new("synthetic-route").unwrap(),
            task: TaskKind::Generation,
            endpoints: vec![native.id.clone()],
            policy: RoutePolicy::default(),
        };
        let public = |contract| {
            PublicModel::new(
                ModelId::new("public-native").unwrap(),
                native.canonical_model.clone(),
                TaskKind::Generation,
                route.id.clone(),
                contract,
            )
        };
        let compile_with = |model: PublicModel| {
            compile(
                vec![provider()],
                vec![native.clone()],
                vec![route.clone()],
                vec![model],
                vec![CanonicalModel {
                    id: native.canonical_model.clone(),
                    task: TaskKind::Generation,
                    contract: GenerationFeatureSet::full(),
                }],
            )
        };
        assert_eq!(
            compile_with(public(GenerationFeatureSet::text_images())).unwrap_err(),
            TopologyError::ContractUnsatisfiable
        );
        // Native targets do not carry standard OpenAI execution context.
        assert_eq!(
            compile_with(public(NativeProfile::AdaptiveTextTools.semantic_contract())).unwrap_err(),
            TopologyError::ContractUnsatisfiable
        );
        let mut model = public(NativeProfile::AdaptiveTextTools.semantic_contract());
        model.standard_context = false;
        compile_with(model).unwrap();

        let p = provider();
        for (default, maximum, valid) in [
            (Some(0), None, false),
            (Some(32_000), Some(8192), false),
            (Some(4096), Some(4096), true),
            (None, Some(1), true),
        ] {
            let mut configured = native.clone();
            let caps = configured.representation.messages_mut().unwrap();
            caps.default_output_tokens = default;
            caps.model_limits.maximum_output_tokens = maximum;
            let result = compile_endpoints(p.clone(), vec![configured]);
            if valid {
                result.unwrap();
            } else {
                assert_eq!(result.unwrap_err(), TopologyError::ContractUnsatisfiable);
            }
        }
        for change in 0..3 {
            let mut bad = native.clone();
            match change {
                0 => bad.target.path = EndpointPath::new("/chat/completions").unwrap(),
                1 => bad.target.origin = TrustedOrigin::parse("https://other.invalid").unwrap(),
                _ => bad.execution.credential_kind = CredentialKind::OAuth("openai"),
            }
            assert_eq!(
                compile_endpoints(p.clone(), vec![bad]).unwrap_err(),
                TopologyError::TargetMismatch
            );
        }
        let mut missing_path = p.clone();
        missing_path
            .endpoints
            .remove(&morphiecore::provider::ProviderOperation::Messages);
        assert_eq!(
            compile_endpoints(missing_path, vec![native.clone()]).unwrap_err(),
            TopologyError::TargetMismatch
        );
        let mut foreign_caps = native.clone();
        foreign_caps.representation = Contract::full().into();
        assert!(foreign_caps.adapter().is_err());
        assert_eq!(
            compile_endpoints(p.clone(), vec![foreign_caps.clone()]).unwrap_err(),
            TopologyError::ContractUnsatisfiable
        );
        let mut foreign_protocol = native;
        foreign_protocol.protocol = GenerationProtocol::OpenAiChat;
        assert!(compile_endpoints(p.clone(), vec![foreign_protocol.clone()]).is_err());
        let request = native_request();
        for wrong in [foreign_caps, foreign_protocol] {
            assert!(representable(&wrong, &request).is_err());
            assert!(prepare(&wrong, &p, &secret(), &request).is_err());
        }
    }

    #[test]
    fn output_defaults_are_candidate_local_and_preflight_matches_preparation() {
        use morphiecore::{
            execution::{AttemptError, prepare_with_rules},
            protocol::{CodecError, ConversionFailure},
            semantic::task::generation::{
                GenerationError, ReasoningActivation, ReasoningMode, ReasoningRequest,
            },
        };
        let mut request = native_request();
        let mut settings = request.task.semantic.settings().clone();
        settings.controls.max_output_tokens = None;
        settings.reasoning = ReasoningRequest::present(None);
        settings.reasoning.activation = Presence::Value(ReasoningActivation::Enabled);
        settings.reasoning.mode = Presence::Value(ReasoningMode::Budgeted);
        settings.reasoning.budget_tokens = Some(2048);
        request.task.semantic = request.task.semantic.with_settings(settings).unwrap();
        let before = request.clone();
        let mut first = standard_native();
        let caps = first.representation.messages_mut().unwrap();
        caps.semantics = NativeProfile::AdaptiveTextTools.representation_contract();
        caps.default_output_tokens = Some(1024);
        let mut second = first.clone();
        second
            .representation
            .messages_mut()
            .unwrap()
            .default_output_tokens = Some(4096);
        assert_eq!(
            representable(&first, &request).unwrap_err().code(),
            "semantic.invalid_control"
        );
        assert!(matches!(
            prepare(&first, &provider(), &secret(), &request),
            Err(AttemptError::Conversion(ConversionFailure {
                cause: morphiecore::protocol::ConversionCause::Codec(CodecError::Semantic(
                    GenerationError::InvalidControl
                )),
                ..
            }))
        ));
        assert_eq!(request, before);
        representable(&second, &request).unwrap();
        let (prepared, rules) =
            prepare_with_rules(&second, &provider(), &secret(), &request).unwrap();
        assert_eq!(body(&prepared)["max_tokens"], 4096);
        assert_eq!(body(&prepared)["thinking"]["budget_tokens"], 2048);
        assert_eq!(
            rules
                .iter()
                .filter(|rule| **rule == "messages.default-max-tokens.v1")
                .count(),
            1
        );
        assert_eq!(request, before);
        assert_eq!(
            first
                .representation
                .messages()
                .unwrap()
                .default_output_tokens,
            Some(1024)
        );
    }

    #[test]
    fn preflight_and_preparation_agree_on_native_body_headers_and_delivery() {
        let p = provider();
        let mut native = standard_native();
        let mut request = native_request();
        request.context.store = Presence::Value(false);
        request.context.background = Presence::Value(false);
        request.context.previous_response_id = Presence::Null;
        request.context.conversation = Presence::Null;
        request.context.prompt = Presence::Null;
        request.context.moderation = Presence::Null;
        let original = request.clone();
        representable(&native, &request).unwrap();
        let prepared = prepare(&native, &p, &secret(), &request).unwrap();
        assert_eq!(prepared.path, "/messages");
        assert_eq!(
            prepared.auth_header,
            ("x-api-key".into(), "synthetic-secret".into())
        );
        assert!(!format!("{prepared:?}").contains("synthetic-secret"));
        assert_eq!(header(&prepared, "anthropic-version"), Some("2026-10-21"));
        assert!(header(&prepared, "user-agent").is_some_and(|v| v.starts_with("MorphieCore/")));
        assert_eq!(header(&prepared, "x-opencode-session"), None);
        assert_eq!(body(&prepared), hello_wire());
        assert_eq!(request, original);

        let mut extended = request.clone();
        extended.context.cache.prompt_cache_retention =
            Presence::Value(morphiecore::semantic::context::CacheRetention::Day);
        assert!(representable(&native, &extended).is_err());
        assert!(prepare(&native, &p, &secret(), &extended).is_err());
        let mut approved = native.clone();
        approved
            .representation
            .messages_mut()
            .unwrap()
            .cache
            .omit_day_retention = true;
        representable(&approved, &extended).unwrap();
        assert_eq!(
            body(&prepare(&approved, &p, &secret(), &extended).unwrap()),
            hello_wire()
        );
        extended.context.cache.prompt_cache_retention =
            Presence::Value(morphiecore::semantic::context::CacheRetention::InMemory);
        assert!(representable(&approved, &extended).is_err());
        assert!(prepare(&approved, &p, &secret(), &extended).is_err());

        // Count escaped UTF-8 wire bytes, not characters or the unescaped source.
        let text = "中文 \"quoted\"\\path";
        let capped = GenerationCodec::new(GenerationApi::Chat, Dialect::Standard, None)
            .decode_request(
                &serde_json::to_vec(&json!({
                    "model":"public", "max_tokens":32,
                    "messages":[{"role":"user","content":text}]
                }))
                .unwrap(),
            )
            .unwrap();
        let expected = json!({
            "model":"synthetic-model", "max_tokens":32,
            "messages":[{"role":"user","content":[{"type":"text","text":text}]}]
        });
        let mut bounded = native.clone();
        bounded.execution.request_body_limit = serde_json::to_vec(&expected).unwrap().len();
        representable(&bounded, &capped).unwrap();
        let encoded = prepare(&bounded, &p, &secret(), &capped).unwrap();
        assert_eq!(body(&encoded), expected);
        assert_eq!(encoded.body.len(), bounded.execution.request_body_limit);
        bounded.execution.request_body_limit -= 1;
        assert_eq!(
            representable(&bounded, &capped),
            Err(morphiecore::execution::plan::RejectionReason::BodyLimit)
        );
        assert!(matches!(
            prepare(&bounded, &p, &secret(), &capped),
            Err(morphiecore::execution::AttemptError::Limit)
        ));

        let mut unsupported = request.clone();
        unsupported.context.store = Presence::Null;
        assert!(representable(&native, &unsupported).is_err());
        assert!(prepare(&native, &p, &secret(), &unsupported).is_err());

        request.delivery.stream = Presence::Value(true);
        assert!(representable(&native, &request).is_err());
        assert!(prepare(&native, &p, &secret(), &request).is_err());
        native.execution.streaming = true;
        representable(&native, &request).unwrap();
        let streaming = prepare(&native, &p, &secret(), &request).unwrap();
        assert_eq!(body(&streaming)["stream"], true);
        assert_eq!(header(&streaming, "accept"), Some("text/event-stream"));
        request.delivery.stream = Presence::Null;
        assert!(representable(&native, &request).is_err());

        let mut restricted = standard_native();
        restricted
            .representation
            .messages_mut()
            .unwrap()
            .semantics
            .max_output_tokens = false;
        let request = native_request();
        assert!(representable(&restricted, &request).is_err());
        assert!(prepare(&restricted, &p, &secret(), &request).is_err());
        let chat_request = GenerationCodec::new(GenerationApi::Chat, Dialect::Standard, None)
            .decode_request(br#"{"model":"public","messages":[{"role":"user","content":"hello"}]}"#)
            .unwrap();
        // Only trusted target policy may fill the missing Chat ceiling.
        assert!(representable(&native, &chat_request).is_err());
        assert!(prepare(&native, &p, &secret(), &chat_request).is_err());
        native
            .representation
            .messages_mut()
            .unwrap()
            .default_output_tokens = Some(32_000);
        representable(&native, &chat_request).unwrap();
        let prepared = prepare(&native, &p, &secret(), &chat_request).unwrap();
        assert_eq!(body(&prepared)["max_tokens"], 32_000);
    }

    #[test]
    fn one_provider_keeps_operation_paths_and_auth_separate() {
        let p = provider();
        let chat = endpoint(GenerationProtocol::OpenAiChat);
        let native = standard_native();
        compile_endpoints(p.clone(), vec![chat.clone(), native.clone()]).unwrap();
        let mut request = GenerationCodec::new(GenerationApi::Chat, Dialect::Standard, None)
            .decode_request(br#"{"model":"public","messages":[{"role":"user","content":"hello"}]}"#)
            .unwrap();
        request.conversation = Some(ConversationContext::conversation("synthetic").unwrap());
        let prepared = prepare(&chat, &p, &secret(), &request).unwrap();
        assert_eq!(prepared.path, "/chat/completions");
        assert_eq!(
            prepared.auth_header,
            ("authorization".into(), "Bearer synthetic-secret".into())
        );
        let prepared = prepare(&native, &p, &secret(), &native_request()).unwrap();
        assert_eq!(prepared.path, "/messages");
        assert_eq!(prepared.auth_header.0, "x-api-key");
    }

    #[test]
    fn native_static_intake_waits_for_the_complete_body() {
        let native = standard_native();
        assert!(matches!(
            native.adapter().unwrap(),
            UpstreamAdapter::AnthropicMessages { .. }
        ));
        let body = br#"{"id":"r","type":"message","role":"assistant","model":"synthetic-model","content":[{"type":"text","text":"answer"}],"stop_reason":"end_turn","usage":{"input_tokens":1,"output_tokens":2}}"#;
        let attempt = || Attempt::new(native.adapter().unwrap(), 4096, SseLimits::default());
        for fragment in [1, 7, body.len()] {
            let mut attempt = attempt();
            attempt.begin(200, "application/json").unwrap();
            for chunk in body.chunks(fragment) {
                assert!(attempt.push(chunk).unwrap().1.is_empty());
            }
            assert!(attempt.response().is_err());
            assert_eq!(attempt.finish().unwrap().metadata.created, None);
        }
        let mut trailing = attempt();
        trailing.begin(200, "application/json").unwrap();
        trailing.push(body).unwrap();
        trailing.push(b"x").unwrap();
        assert!(trailing.finish().is_err());
        assert!(trailing.begin(200, "application/json").is_err());
        let mut empty_stream = attempt();
        empty_stream.begin(200, "text/event-stream").unwrap();
        assert!(empty_stream.finish().is_err());
        assert!(attempt().begin(200, "text/plain").is_err());
    }
}
mod native_catalog {
    //! Native registration, OpenAI activation and Provider acceptance are distinct.
    use morphiecore::{
        execution::{admit, plan::representable, prepare},
        protocol::{
            GenerationRequestEnvelope as Request, anthropic::messages::Profile,
            native::ReplayTarget,
        },
        provider::SecretMaterial,
        semantic::{task::generation::LocalScope, value::Presence},
        topology::catalog,
    };
    #[test]
    fn native_binding_resolves_conditional_strictness_before_preflight_and_preparation() {
        use morphiecore::{
            execution::AttemptError,
            protocol::{
                ConversionFailure,
                conversion::generation::RepresentationError,
                openai::{Dialect, GenerationApi as Client, GenerationCodec},
            },
            semantic::task::generation::GenerationFeature,
        };
        let binding = &catalog::MESSAGES_BINDINGS[0];
        let topology = catalog::default_topology().unwrap();
        // pi's shape: a developer prompt and a tool whose `strict` is omitted.
        let client = GenerationCodec::new(Client::Responses, Dialect::Standard, None);
        let mut request = client
            .decode_client_request(
                br#"{"model":"claude-haiku-5-5","max_output_tokens":64,"store":false,
                "temperature":0.5,"top_p":0.8,
                "input":[{"role":"developer","content":"Be brief."},{"role":"user","content":"hi"}],
                "tools":[{"type":"function","name":"read","description":"Read a file.",
                    "parameters":{"type":"object","properties":{"path":{"type":"string"}}}}]}"#,
            )
            .unwrap();
        request.conversation = Some(
            morphiecore::semantic::context::ConversationContext::conversation("synthetic").unwrap(),
        );
        admit(topology.model(binding.model).unwrap(), &request).unwrap();
        representable(&binding.endpoint(), &request).unwrap();
        let original = request.clone();
        let (prepared, rules) = morphiecore::execution::prepare_with_rules(
            &binding.endpoint(),
            &binding.provider(),
            &SecretMaterial::new("synthetic-key").unwrap(),
            &request,
        )
        .unwrap();
        let wire: serde_json::Value = serde_json::from_slice(&prepared.body).unwrap();
        assert_eq!(wire["tools"][0]["strict"], false);
        assert_eq!(
            wire["tools"][0]["input_schema"],
            serde_json::json!({"type":"object","properties":{"path":{"type":"string"}}})
        );
        assert!(rules.contains(&"openai.omit-conditional-tool-strictness.v1"));
        assert_eq!(request, original);
        for parameters in [
            Some(serde_json::json!({"type":"object","x-unknown":true})),
            None,
        ] {
            let mut projected = request.clone();
            let mut settings = projected.task.semantic.settings().clone();
            let morphiecore::semantic::task::generation::ToolDefinition::Function(tool) =
                &mut settings.tools.as_mut().unwrap()[0]
            else {
                panic!("function")
            };
            tool.parameters = parameters.clone().map(Into::into);
            projected.task.semantic = projected.task.semantic.with_settings(settings).unwrap();
            let before = projected.clone();
            admit(topology.model(binding.model).unwrap(), &projected).unwrap();
            representable(&binding.endpoint(), &projected).unwrap();
            let (prepared, rules) = morphiecore::execution::prepare_with_rules(
                &binding.endpoint(),
                &binding.provider(),
                &SecretMaterial::new("synthetic-key").unwrap(),
                &projected,
            )
            .unwrap();
            let wire: serde_json::Value = serde_json::from_slice(&prepared.body).unwrap();
            assert_eq!(wire["tools"][0]["strict"], parameters.is_none());
            assert_eq!(
                rules.contains(&"openai.omit-conditional-tool-strictness.v1"),
                parameters.is_some()
            );
            assert_eq!(wire["tools"][0]["input_schema"], parameters.unwrap_or_else(||
                serde_json::json!({"type":"object","properties":{},"additionalProperties":false})
            ));
            assert_eq!(projected, before);
        }
        let mut settings = request.task.semantic.settings().clone();
        let morphiecore::semantic::task::generation::ToolDefinition::Function(tool) =
            &mut settings.tools.as_mut().unwrap()[0]
        else {
            panic!()
        };
        tool.strict = morphiecore::semantic::task::generation::FunctionStrictness::Explicit(false);
        request.task.semantic = request.task.semantic.with_settings(settings).unwrap();
        admit(topology.model(binding.model).unwrap(), &request).unwrap();
        representable(&binding.endpoint(), &request).unwrap();
        let original = request.clone();
        let (prepared, rules) = morphiecore::execution::prepare_with_rules(
            &binding.endpoint(),
            &binding.provider(),
            &SecretMaterial::new("synthetic-key").unwrap(),
            &request,
        )
        .unwrap();
        let wire: serde_json::Value = serde_json::from_slice(&prepared.body).unwrap();
        assert_eq!(
            wire,
            serde_json::json!({
                "model":"claude-haiku-5-5", "max_tokens":64,
                "cache_control":{"type":"ephemeral"},
                "system":[{"type":"text","text":"Be brief."}],
                "messages":[{"role":"user","content":[{"type":"text","text":"hi"}]}],
                "tools":[{"name":"read","description":"Read a file.","strict":false,
                    "input_schema":{"type":"object","properties":{"path":{"type":"string"}}}}]
            })
        );
        assert_eq!(
            rules,
            [
                "messages.default-automatic-cache.v1",
                "messages.omit-sampling.v1"
            ]
        );
        assert_eq!(request, original);
        let mut settings = request.task.semantic.settings().clone();
        let morphiecore::semantic::task::generation::ToolDefinition::Function(tool) =
            &mut settings.tools.as_mut().unwrap()[0]
        else {
            panic!("function tool")
        };
        tool.cache = Some(morphiecore::semantic::task::generation::PrefixCacheTtl::OneHour);
        request.task.semantic = request.task.semantic.with_settings(settings).unwrap();
        assert!(matches!(
            admit(topology.model(binding.model).unwrap(), &request),
            Err(AttemptError::Conversion(ConversionFailure {
                cause: morphiecore::protocol::ConversionCause::Representation(
                    RepresentationError::Admission(GenerationFeature::ExplicitCache)
                ),
                ..
            }))
        ));
        // The native endpoint is opted in, but the mixed catalog public route is not.
        representable(&binding.endpoint(), &request).unwrap();
        let prepared = prepare(
            &binding.endpoint(),
            &binding.provider(),
            &SecretMaterial::new("synthetic-key").unwrap(),
            &request,
        )
        .unwrap();
        let wire: serde_json::Value = serde_json::from_slice(&prepared.body).unwrap();
        assert_eq!(
            wire["tools"][0]["cache_control"],
            serde_json::json!({"type":"ephemeral","ttl":"1h"})
        );
        assert!(wire.get("cache_control").is_none());
    }
    #[test]
    fn declared_native_binding_prepares_the_messages_wire() {
        let binding = &catalog::MESSAGES_BINDINGS[0];
        let topology = catalog::default_topology().unwrap();
        let endpoint = binding.endpoint();
        let provider = binding.provider();
        let target = ReplayTarget::new(provider.id.as_str(), binding.upstream).unwrap();
        let source=Profile::AdaptiveTextTools.decode_generation_request(
        br#"{"model":"claude-haiku-5-5","max_tokens":32,"messages":[{"role":"user","content":"synthetic"}]}"#,
        LocalScope::new(1),&target).unwrap();
        let mut request = Request::from_generation(source, binding.model).unwrap();
        request.conversation = Some(
            morphiecore::semantic::context::ConversationContext::conversation("synthetic").unwrap(),
        );
        for stream in [false, true] {
            request.delivery.stream = Presence::Value(stream);
            admit(topology.model(binding.model).unwrap(), &request).unwrap();
            representable(&endpoint, &request).unwrap();
            let original = request.clone();
            let prepared = prepare(
                &endpoint,
                &provider,
                &SecretMaterial::new("synthetic-key").unwrap(),
                &request,
            )
            .unwrap();
            assert_eq!(prepared.path, "/zen/go/v1/messages");
            assert_eq!(prepared.auth_header.0, "x-api-key");
            assert!(
                prepared
                    .safe_headers
                    .contains(&("anthropic-version".into(), "2026-10-21".into()))
            );
            assert!(
                prepared
                    .safe_headers
                    .iter()
                    .any(|(n, v)| n == "user-agent" && v.starts_with("MorphieCore/"))
            );
            assert!(
                prepared
                    .safe_headers
                    .iter()
                    .any(|(n, v)| n == "x-opencode-session" && v.len() == 64)
            );
            let wire: serde_json::Value = serde_json::from_slice(&prepared.body).unwrap();
            assert_eq!(
                wire,
                serde_json::json!({
                    "model":"claude-haiku-5-5", "max_tokens":32, "stream":stream,
                    "cache_control":{"type":"ephemeral"},
                    "messages":[{"role":"user","content":[{"type":"text","text":"synthetic"}]}]
                })
            );
            assert_eq!(original, request);
        }
        let mut controlled = request.clone();
        let mut settings = controlled.task.semantic.settings().clone();
        settings.reasoning =
            morphiecore::semantic::task::generation::ReasoningRequest::present(None);
        settings.reasoning.budget_tokens = Some(1024);
        assert!(
            controlled
                .task
                .semantic
                .clone()
                .with_settings(settings.clone())
                .is_err()
        );
        settings.controls.max_output_tokens = Some(2048);
        controlled.task.semantic = controlled.task.semantic.with_settings(settings).unwrap();
        // This native profile has no numeric carrier; do not substitute an effort.
        assert!(representable(&endpoint, &controlled).is_err());
        assert!(
            prepare(
                &endpoint,
                &provider,
                &SecretMaterial::new("synthetic-key").unwrap(),
                &controlled,
            )
            .is_err()
        );
    }
    #[test]
    fn native_unknown_response_fields_are_ignored_and_request_errors_do_not_leak_keys() {
        let p = Profile::AdaptiveTextTools;
        let base = serde_json::json!({"id":"msg","type":"message","role":"assistant","model":"synthetic-model",
        "content":[{"type":"text","text":"ok"}],"stop_reason":"end_turn",
        "usage":{"input_tokens":1,"output_tokens":1}});
        let expected = p.decode_response(base.to_string().as_bytes()).unwrap();
        for key in ["future-report", "private-business-key"] {
            let mut wire = base.clone();
            wire[key] = serde_json::Value::Null;
            assert_eq!(
                p.decode_response(wire.to_string().as_bytes()).unwrap(),
                expected
            );
        }
        let request = serde_json::json!({"model":"m","max_tokens":8,
        "messages":[{"role":"user","content":"hi"}],"private-business-key":null});
        let error = p
            .decode_request(request.to_string().as_bytes())
            .unwrap_err();
        assert!(!error.to_string().contains("private-business-key"));
    }
    #[test]
    fn native_caps_and_reasoning_policies_do_not_widen_unrelated_admission() {
        let mut endpoint = catalog::MESSAGES_BINDINGS[0].endpoint();
        endpoint.validate().unwrap();
        endpoint.protocol = morphiecore::topology::GenerationProtocol::OpenAiChat;
        assert!(endpoint.validate().is_err());
        endpoint.protocol = morphiecore::topology::GenerationProtocol::OpenAiResponses;
        assert!(endpoint.validate().is_err());
        use morphiecore::{
            protocol::reasoning::{ReasoningConversion, ReasoningRequestPolicy, SHARED_BUDGETS},
            semantic::task::generation::{EffortScope, ReasoningMode, ReasoningRequest},
        };
        static OFF: ReasoningRequestPolicy = ReasoningRequestPolicy {
            canonical_model: std::borrow::Cow::Borrowed("synthetic-model"),
            rule: "synthetic.off.v1",
            conversion: ReasoningConversion::PreserveDisabled,
        };
        static ALLOCATE: ReasoningRequestPolicy = ReasoningRequestPolicy {
            canonical_model: std::borrow::Cow::Borrowed("synthetic-model"),
            rule: "synthetic.allocation.v1",
            conversion: ReasoningConversion::EffortToBudget {
                scope: EffortScope::Reasoning,
                table: SHARED_BUDGETS,
            },
        };
        let mut google = endpoint;
        google.protocol = morphiecore::topology::GenerationProtocol::GoogleGenerateContent;
        google.canonical_model = morphiecore::topology::ModelId::new("synthetic-model").unwrap();
        let mut request = morphiecore::protocol::openai::GenerationCodec::new(
            morphiecore::protocol::openai::GenerationApi::Responses,
            morphiecore::protocol::openai::Dialect::Standard,
            None,
        )
        .decode_request(br#"{"model":"m","input":"hello"}"#)
        .unwrap();
        let mut settings = request.task.semantic.settings().clone();
        settings.reasoning = ReasoningRequest::present(None);
        settings.reasoning.budget_tokens = Some(1024);
        settings.reasoning.mode = Presence::Value(ReasoningMode::Budgeted);
        settings.controls.max_output_tokens = Some(2048);
        request.task.semantic = request
            .task
            .semantic
            .clone()
            .with_settings(settings)
            .unwrap();
        let mut caps = morphiecore::protocol::google::generate_content::TargetContract::default();
        google.representation = caps.clone().into();
        assert!(google.encode_request(&request).is_err());
        caps.reasoning_policy = Some(OFF.clone());
        google.representation = caps.clone().into();
        assert!(google.encode_request(&request).is_err());
        caps.reasoning_policy = Some(ALLOCATE.clone());
        google.representation = caps.into();
        assert_eq!(
            google.encode_request(&request).unwrap()["generationConfig"]["thinkingConfig"]["thinkingBudget"],
            1024
        );
    }
}
