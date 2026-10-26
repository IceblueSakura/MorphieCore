//! Fixed candidate planning and pre-commit fallback policy.
mod candidates {
    //! Candidate filtering must be value-sensitive and preserve trusted ordering.
    use morphiecore::{
        execution::ExecutionPlan,
        protocol::openai::{
            Dialect, GenerationApi, GenerationCodec, TargetContract as Representation,
        },
        provider::{
            AuthScheme, CredentialBindingId, EndpointPath, ProviderDefinition, ProviderId,
            TrustedOrigin,
        },
        semantic::task::generation::ImageFormat,
        topology::{catalog, *},
    };
    use serde_json::json;
    fn provider(id: &str) -> ProviderDefinition {
        ProviderDefinition {
            id: ProviderId::new(id).unwrap(),
            origin: TrustedOrigin::parse("https://synthetic.invalid").unwrap(),
            endpoints: [(
                morphiecore::provider::ProviderOperation::Responses,
                morphiecore::provider::ProviderEndpoint {
                    path: EndpointPath::new("/responses").unwrap(),
                    auth: None,
                },
            )]
            .into(),
            credential_pool: None,
            auth: AuthScheme::Bearer,
        }
    }
    fn topology(policy: CandidatePolicy) -> CompiledTopology {
        let model_id = ModelId::new("synthetic-canonical").unwrap();
        let mut endpoints = vec![];
        for (id, provider, formats) in [
            ("a", provider("first"), vec![ImageFormat::Png]),
            ("b", provider("second"), vec![ImageFormat::Bmp]),
        ] {
            let mut representation =
                GenerationCodec::new(GenerationApi::Responses, Dialect::MorphieCore, None)
                    .contract(&Representation::full());
            representation.images.inline_formats = formats;
            endpoints.push(Endpoint {
                id: EndpointId::new(id).unwrap(),
                provider: provider.id.clone(),
                target: EndpointTarget {
                    origin: provider.origin.clone(),
                    path: provider
                        .entry(morphiecore::provider::ProviderOperation::Responses)
                        .unwrap()
                        .path
                        .clone(),
                },
                canonical_model: model_id.clone(),
                task: TaskKind::Generation,
                protocol: GenerationProtocol::OpenAiResponses,
                upstream_model: format!("{id}-alias"),
                representation: representation.into(),
                execution: ExecutionContract {
                    streaming: true,
                    retry_before_commit: false,
                    request_body_limit: 4096,
                    response_body_limit: 4096,
                    timeout_ms: 1000,
                    credential_kind: morphiecore::provider::CredentialKind::ApiKey,
                },
                credential: CredentialBindingId::new(&format!("{id}-key")).unwrap(),
            });
        }
        let route = Route {
            id: RouteId::new("route").unwrap(),
            task: TaskKind::Generation,
            endpoints: vec![EndpointId::new("a").unwrap(), EndpointId::new("b").unwrap()],
            policy: RoutePolicy {
                candidates: policy,
                ..RoutePolicy::default()
            },
        };
        let public = PublicModel::new(
            ModelId::new("public").unwrap(),
            model_id.clone(),
            TaskKind::Generation,
            route.id.clone(),
            GenerationFeatureSet::text_images(),
        );
        compile(
            vec![provider("first"), provider("second")],
            endpoints,
            vec![route],
            vec![public],
            vec![CanonicalModel {
                id: model_id,
                task: TaskKind::Generation,
                contract: GenerationFeatureSet::full(),
            }],
        )
        .unwrap()
    }

    #[test]
    fn full_requests_are_filtered_without_mutation_or_reordering() {
        let client = GenerationCodec::new(GenerationApi::Responses, Dialect::MorphieCore, None);
        let request=client.decode_request(&serde_json::to_vec(&json!({"model":"public","input":[{"role":"user","content":[{"type":"input_image","image_url":"data:image/bmp;base64,AQ=="}]}]})).unwrap()).unwrap();
        let before = request.clone();
        let plan =
            ExecutionPlan::for_request(&topology(CandidatePolicy::SkipUnrepresentable), &request)
                .unwrap();
        assert_eq!(
            plan.candidates
                .iter()
                .map(|c| c.endpoint_id.as_str())
                .collect::<Vec<_>>(),
            vec!["b"]
        );
        assert_eq!(request, before);
        assert_eq!(plan.rejections.len(), 1);
        assert_eq!(plan.rejections[0].endpoint_id.as_str(), "a");
        assert_eq!(
            plan.rejections[0].reason,
            morphiecore::execution::plan::RejectionReason::Image
        );
        let compiled = topology(CandidatePolicy::SkipUnrepresentable);
        let active = morphiecore::execution::plan::select_candidates(
            &request,
            CandidatePolicy::SkipUnrepresentable,
            compiled.route_endpoints(&RouteId::new("route").unwrap()),
        )
        .unwrap();
        assert_eq!(active.candidates, plan.candidates);
        assert_eq!(active.rejections, plan.rejections);
        assert!(
            ExecutionPlan::for_request(&topology(CandidatePolicy::RequireAll), &request).is_err()
        );
        let both=client.decode_request(&serde_json::to_vec(&json!({"model":"public","input":[{"role":"user","content":[{"type":"input_image","image_url":"data:image/bmp;base64,AQ=="},{"type":"input_image","image_url":"data:image/png;base64,AQ=="}]}]})).unwrap()).unwrap();
        let error =
            ExecutionPlan::for_request(&topology(CandidatePolicy::SkipUnrepresentable), &both)
                .unwrap_err();
        let morphiecore::execution::PlanError::NoCandidate { rejections } = error else {
            panic!("expected all candidate rejections")
        };
        assert_eq!(rejections.len(), 2);
        assert!(
            rejections
                .iter()
                .all(|r| r.reason == morphiecore::execution::plan::RejectionReason::Image)
        );
        let text = client
            .decode_request(br#"{"model":"public","input":"x"}"#)
            .unwrap();
        let plan =
            ExecutionPlan::for_request(&topology(CandidatePolicy::SkipUnrepresentable), &text)
                .unwrap();
        assert_eq!(
            plan.candidates
                .iter()
                .map(|c| c.endpoint_id.as_str())
                .collect::<Vec<_>>(),
            vec!["a", "b"]
        );
        let endpoint = compiled.endpoint(&EndpointId::new("a").unwrap()).unwrap();
        assert_eq!(
            morphiecore::execution::plan::select_candidates(
                &text,
                CandidatePolicy::SkipUnrepresentable,
                std::iter::repeat_n(endpoint, 65)
            ),
            Err(morphiecore::execution::PlanError::CandidateLimit)
        );
    }
    #[test]
    fn dashscope_qwen_preserves_the_total_output_cap_in_preflight_and_preparation() {
        use morphiecore::execution::{admit, plan::representable, prepare};
        let topology = catalog::default_topology().unwrap();
        let client = GenerationCodec::new(GenerationApi::Chat, Dialect::Standard, None);
        let secret = morphiecore::provider::SecretMaterial::new("synthetic").unwrap();
        for (model, binding) in [
            ("qwen3.8-flash", "dashscope-flash-chat"),
            ("qwen3.8-max", "aliyun-dashscope-cn-chat"),
        ] {
            let request = client
                .decode_client_request(
                    json!({"model":model,"messages":[{"role":"user","content":"go"}],
                    "max_completion_tokens":64,"reasoning_effort":"low"})
                    .to_string()
                    .as_bytes(),
                )
                .unwrap();
            // Independent wire -> IR expectation, not an encoder round trip.
            assert_eq!(request.task.semantic.controls().max_output_tokens, Some(64));
            let before = request.clone();
            admit(topology.model(model).unwrap(), &request).unwrap();
            let endpoint = topology
                .endpoint(&EndpointId::new(binding).unwrap())
                .unwrap();
            representable(endpoint, &request).unwrap();
            let wire = endpoint.encode_request(&request).unwrap();
            assert_eq!(wire["max_completion_tokens"], 64);
            assert!(wire.get("max_tokens").is_none());
            assert_eq!(wire["reasoning_effort"], "low");
            let prepared = prepare(
                endpoint,
                topology.endpoint_provider(endpoint).unwrap(),
                &secret,
                &request,
            )
            .unwrap();
            assert_eq!(
                serde_json::from_slice::<serde_json::Value>(&prepared.body).unwrap(),
                wire
            );
            assert_eq!(request, before);
        }
        // All selected first-party Bailian models support the unambiguous total cap.
        let glm = topology
            .endpoint(&EndpointId::new("dashscope-glm-chat").unwrap())
            .unwrap();
        assert!(
            !glm.representation
                .openai()
                .unwrap()
                .adaptation
                .rules
                .legacy_max_tokens
        );
    }
    #[test]
    fn a_narrower_native_candidate_is_skipped_not_admitted() {
        let topology = catalog::default_topology().unwrap();
        let model = catalog::GOOGLE_BINDINGS[0].model;
        let video = GenerationCodec::new(GenerationApi::Chat, Dialect::ModelBest, None)
            .decode_client_request(
                &serde_json::to_vec(&json!({"model":model,"messages":[{"role":"user","content":[
                    {"type":"video_url","video_url":{"url":"data:video/mp4;base64,AQID"}}]}]}))
                .unwrap(),
            )
            .unwrap();
        let plan = ExecutionPlan::for_request(&topology, &video).unwrap();
        let ids = |c: &[morphiecore::execution::plan::CandidatePlan]| {
            c.iter()
                .map(|c| c.endpoint_id.as_str().to_owned())
                .collect::<Vec<_>>()
        };
        assert_eq!(
            ids(&plan.candidates),
            [format!("google-{model}-generate-content")]
        );
        assert_eq!(
            plan.rejections[0].endpoint_id.as_str(),
            format!("google-{model}-interactions")
        );
    }
    #[test]
    fn responses_tool_images_need_an_image_model_and_a_responses_carrier() {
        let topology = catalog::default_topology().unwrap();
        let read = |model: &str| {
            GenerationCodec::new(GenerationApi::Responses, Dialect::Standard, None)
                .decode_client_request(
                    &serde_json::to_vec(&json!({"model":model,
                        "tools":[{"type":"function","name":"read","description":"Read media","strict":false,"parameters":{"type":"object"}}],
                        "input":[
                            {"type":"function_call","call_id":"c","name":"read","arguments":"{}"},
                            {"type":"function_call_output","call_id":"c","output":[
                                {"type":"input_image","image_url":"data:image/png;base64,AQID"}]}]}))
                    .unwrap(),
                )
                .unwrap()
        };
        let request = read("qwen3.8-max");
        let public = topology.model("qwen3.8-max").unwrap();
        morphiecore::execution::admit(public, &request).unwrap();
        let representable = |protocol| {
            topology
                .route_endpoints(&public.route)
                .into_iter()
                .filter(|e| e.protocol == protocol)
                .map(|e| morphiecore::execution::plan::representable(e, &request).is_ok())
                .collect::<Vec<_>>()
        };
        assert_eq!(
            representable(GenerationProtocol::OpenAiResponses),
            [false, false]
        );
        // Model image support does not create a Bailian function-output media carrier.
        // Its documented output is a string; the standard codec's broader array is not enough.
        // Standard Chat has no tool-result media carrier, so Chat targets reject it.
        assert_eq!(
            representable(GenerationProtocol::OpenAiChat),
            [false, false]
        );
        let text_only = topology.model("qwen3.8-flash").unwrap();
        assert!(morphiecore::execution::admit(text_only, &read("qwen3.8-flash")).is_err());
    }
}
mod fallback_policy {
    //! Classification and commit/attempt budgets, not Provider quality, govern fallback.
    use morphiecore::{
        execution::{DeliveryState, fallback::Transition},
        provider::RecoveryClass,
        topology::{FallbackPolicy, RoutePolicy},
    };
    #[test]
    fn fixed_route_policy_never_replays_visible_or_exhausted_attempts() {
        let may_advance = |policy: &RoutePolicy, state, error, attempts, remaining| {
            Transition::Provider.may_advance(policy, state, error, attempts, remaining)
        };
        let policy = RoutePolicy {
            fallback: FallbackPolicy::BeforeCommit,
            max_attempts: 2,
            ..RoutePolicy::default()
        };
        assert!(Transition::Credential { enabled: true }.may_advance(
            &policy,
            DeliveryState::Uncommitted,
            RecoveryClass::CredentialUnavailable,
            1,
            true,
        ));
        assert!(!Transition::Provider.may_advance(
            &policy,
            DeliveryState::Uncommitted,
            RecoveryClass::CredentialUnavailable,
            1,
            true,
        ));
        for transition in [
            Transition::Credential { enabled: true },
            Transition::SameProvider,
        ] {
            assert!(!transition.may_advance(
                &policy,
                DeliveryState::Uncommitted,
                RecoveryClass::RateLimit,
                1,
                true,
            ));
        }
        for error in [
            RecoveryClass::RateLimit,
            RecoveryClass::Server,
            RecoveryClass::Timeout,
            RecoveryClass::Upstream,
        ] {
            assert!(may_advance(
                &policy,
                DeliveryState::Uncommitted,
                error,
                1,
                true
            ));
            assert!(!may_advance(
                &policy,
                DeliveryState::Committed,
                error,
                1,
                true
            ));
            assert!(!may_advance(
                &policy,
                DeliveryState::Terminal,
                error,
                1,
                true
            ));
            assert!(!may_advance(
                &policy,
                DeliveryState::Uncommitted,
                error,
                2,
                true
            ));
            assert!(!may_advance(
                &policy,
                DeliveryState::Uncommitted,
                error,
                1,
                false
            ));
            assert!(!may_advance(
                &RoutePolicy::default(),
                DeliveryState::Uncommitted,
                error,
                1,
                true
            ));
        }
        let redirect = match morphiecore::provider::classify_status(302) {
            morphiecore::provider::StatusClass::Failure(error) => error,
            _ => panic!("redirect is not success"),
        };
        assert!(!may_advance(
            &policy,
            DeliveryState::Uncommitted,
            redirect,
            1,
            true
        ));
        for error in [
            RecoveryClass::Authentication,
            RecoveryClass::PermissionDenied,
            RecoveryClass::InvalidRequest,
            RecoveryClass::NotFound,
        ] {
            assert!(!may_advance(
                &policy,
                DeliveryState::Uncommitted,
                error,
                1,
                true
            ));
        }
    }
}
