//! Native Messages bindings, catalog activation and preparation.
mod native_bindings {
    //! Operation binding and pure preparation; never opens a socket or reads a credential store.
    use morphiecore::{
        adapter::{Request, UpstreamAdapter},
        execution::{Attempt, UpstreamRequest, plan::representable, prepare},
        protocol::{
            anthropic::{Dialect as NativeDialect, Profile as NativeProfile, TargetCaps},
            native::ReplayTarget,
            openai::{Adapter, Dialect, Profile, TargetCaps as Contract},
            stream::SseLimits,
        },
        provider::{
            AuthScheme, CredentialBindingId, CredentialKind, EndpointPath, ProviderDefinition,
            ProviderId, SecretMaterial, TrustedOrigin,
        },
        semantic::{
            context::ConversationContext,
            task::generation::{GenerationSemanticContract, LocalScope},
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
            chat_completions: Some(EndpointPath::new("/chat/completions").unwrap()),
            responses: None,
            messages: Some(EndpointPath::new("/messages").unwrap()),
            generate_content: None,
            interactions: None,
            auth: AuthScheme::Bearer,
        }
    }
    fn endpoint(protocol: ProtocolProfile) -> Endpoint {
        let p = provider();
        let chat = protocol == ProtocolProfile::OpenAiChat;
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
                Adapter::new(Profile::Chat, Dialect::OpenCodeGo, None)
                    .contract(&Contract::full())
                    .into()
            } else {
                TargetCaps::new(NativeProfile::AdaptiveTextTools, NativeDialect::OpenCodeGo).into()
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
        let mut native = endpoint(ProtocolProfile::AnthropicMessages);
        native.representation =
            TargetCaps::new(NativeProfile::AdaptiveTextTools, NativeDialect::Standard).into();
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
                contract: GenerationSemanticContract::full(),
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
        json!({"model":"synthetic-model","max_tokens":32,"cache_control":{"type":"ephemeral"},
            "messages":[{"role":"user","content":[{"type":"text","text":"hello"}]}]})
    }

    #[test]
    fn compilation_rejects_contracts_the_native_slice_cannot_keep() {
        let native = endpoint(ProtocolProfile::AnthropicMessages);
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
                    contract: GenerationSemanticContract::full(),
                }],
            )
        };
        assert_eq!(
            compile_with(public(GenerationSemanticContract::text_images())).unwrap_err(),
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
        missing_path.messages = None;
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
        foreign_protocol.protocol = ProtocolProfile::OpenAiChat;
        assert!(compile_endpoints(p.clone(), vec![foreign_protocol.clone()]).is_err());
        let request = native_request();
        for wrong in [foreign_caps, foreign_protocol] {
            assert!(representable(&wrong, &request).is_err());
            assert!(prepare(&wrong, &p, &secret(), &request).is_err());
        }
    }

    #[test]
    fn preflight_and_preparation_agree_on_native_body_headers_and_delivery() {
        let p = provider();
        let mut native = standard_native();
        let mut request = native_request();
        let original = request.clone();
        representable(&native, &request).unwrap();
        let prepared = prepare(&native, &p, &secret(), &request).unwrap();
        assert_eq!(prepared.path, "/messages");
        assert_eq!(
            prepared.auth_header,
            ("x-api-key".into(), "synthetic-secret".into())
        );
        assert!(!format!("{prepared:?}").contains("synthetic-secret"));
        assert_eq!(header(&prepared, "anthropic-version"), Some("2023-06-01"));
        assert!(header(&prepared, "user-agent").is_some_and(|v| v.starts_with("MorphieCore/")));
        assert_eq!(header(&prepared, "x-opencode-session"), None);
        assert_eq!(body(&prepared), hello_wire());
        assert_eq!(request, original);

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
        let chat_request = Adapter::new(Profile::Chat, Dialect::Standard, None)
            .decode_request(br#"{"model":"public","messages":[{"role":"user","content":"hello"}]}"#)
            .unwrap();
        // Messages requires max_tokens; an uncapped Chat request gets the fixed default.
        representable(&native, &chat_request).unwrap();
        let prepared = prepare(&native, &p, &secret(), &chat_request).unwrap();
        assert_eq!(body(&prepared)["max_tokens"], 32_000);
    }

    #[test]
    fn one_provider_keeps_operation_paths_and_auth_separate() {
        let p = provider();
        let chat = endpoint(ProtocolProfile::OpenAiChat);
        let native = standard_native();
        compile_endpoints(p.clone(), vec![chat.clone(), native.clone()]).unwrap();
        let mut request = Adapter::new(Profile::Chat, Dialect::Standard, None)
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
        adapter::Request,
        execution::{admit, plan::representable, prepare},
        protocol::{anthropic::Profile, native::ReplayTarget},
        provider::SecretMaterial,
        semantic::{task::generation::LocalScope, value::Presence},
        topology::catalog,
    };
    #[test]
    fn pi_responses_requests_reach_the_native_binding_by_named_rules() {
        use morphiecore::protocol::openai::{Adapter, Dialect, Profile as Client};
        let binding = &catalog::MESSAGES_BINDINGS[0];
        let topology = catalog::default_topology().unwrap();
        // pi's shape: a developer prompt and a tool whose `strict` is omitted.
        let client = Adapter::new(Client::Responses, Dialect::Standard, None);
        let mut request = client
            .decode_client_request(
                br#"{"model":"claude-haiku-5-5","max_output_tokens":64,"store":false,
                "input":[{"role":"developer","content":"Be brief."},{"role":"user","content":"hi"}],
                "tools":[{"type":"function","name":"read","description":"Read a file.",
                    "parameters":{"type":"object","properties":{"path":{"type":"string"}}}}]}"#,
            )
            .unwrap();
        request.conversation = Some(
            morphiecore::semantic::context::ConversationContext::conversation("synthetic").unwrap(),
        );
        admit(topology.model(binding.model).unwrap(), &request).unwrap();
        let (prepared, rules) = morphiecore::execution::prepare_with_rules(
            &binding.endpoint(),
            &binding.provider(),
            &SecretMaterial::new("synthetic-key").unwrap(),
            &request,
        )
        .unwrap();
        let wire: serde_json::Value = serde_json::from_slice(&prepared.body).unwrap();
        assert_eq!(
            wire["system"],
            serde_json::json!([{"type":"text","text":"Be brief."}])
        );
        assert!(wire["tools"][0].get("strict").is_none());
        assert_eq!(
            rules,
            [
                "messages.developer-as-system.v1",
                "messages.default-strict-as-plain.v1"
            ]
        );
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
                    .contains(&("anthropic-version".into(), "2023-06-01".into()))
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
            assert_eq!(wire["model"], binding.upstream);
            assert_eq!(wire["stream"], stream);
            assert!(wire.get("thinking").is_none());
            assert!(wire.get("session_id").is_none());
            assert_eq!(original, request);
        }
        let mut controlled = request.clone();
        let mut settings = controlled.task.semantic.settings().clone();
        settings.reasoning =
            morphiecore::semantic::task::generation::ReasoningRequest::present(None, None);
        settings.reasoning.budget_tokens = Some(1024);
        controlled.task.semantic = controlled.task.semantic.with_settings(settings).unwrap();
        // Claude 5.x has no token budget: it becomes the nearest effort.
        assert!(representable(&endpoint, &controlled).is_ok());
        let prepared = prepare(
            &endpoint,
            &provider,
            &SecretMaterial::new("synthetic-key").unwrap(),
            &controlled,
        )
        .unwrap();
        let wire: serde_json::Value = serde_json::from_slice(&prepared.body).unwrap();
        assert_eq!(wire["output_config"], serde_json::json!({"effort":"low"}));
        assert!(!prepared.body.windows(13).any(|w| w == b"budget_tokens"));
    }
    #[test]
    fn native_unknown_response_fields_are_ignored_and_request_errors_do_not_leak_keys() {
        let p = Profile::AdaptiveTextTools;
        let base = serde_json::json!({"id":"msg","type":"message","role":"assistant","model":"synthetic-model",
        "content":[{"type":"text","text":"ok"}],"stop_reason":"end_turn",
        "usage":{"input_tokens":1,"output_tokens":1}});
        let expected = p.decode_response(base.to_string().as_bytes()).unwrap();
        for key in ["stop_details", "private-business-key"] {
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
    fn native_caps_cannot_be_reinterpreted_as_an_openai_endpoint() {
        let mut endpoint = catalog::MESSAGES_BINDINGS[0].endpoint();
        endpoint.validate().unwrap();
        endpoint.protocol = morphiecore::topology::ProtocolProfile::OpenAiChat;
        assert!(endpoint.validate().is_err());
        endpoint.protocol = morphiecore::topology::ProtocolProfile::OpenAiResponses;
        assert!(endpoint.validate().is_err());
    }
}
