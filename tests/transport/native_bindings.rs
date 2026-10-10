//! Operation binding and pure preparation; never opens a socket or reads a credential store.
use morphiecore::{
    adapter::{Adapter, Dialect, UpstreamAdapter},
    execution::{Attempt, plan::representable, prepare, prepare_messages},
    lowering::generation::GenerationRepresentationContract as Contract,
    protocol::{
        anthropic::Profile as NativeProfile,
        cache::CacheSession,
        openai::{Profile, sse::SseLimits},
    },
    provider::{
        AuthScheme, CredentialBindingId, CredentialKind, EndpointPath, GenerationOperation,
        ProviderDefinition, ProviderId, SecretMaterial, TrustedOrigin,
    },
    semantic::task::generation::GenerationSemanticContract,
    topology::*,
};
use serde_json::json;

fn provider() -> ProviderDefinition {
    ProviderDefinition {
        id: ProviderId::new("synthetic").unwrap(),
        origin: TrustedOrigin::parse("https://synthetic.invalid").unwrap(),
        chat_completions: Some(EndpointPath::new("/chat/completions").unwrap()),
        responses: None,
        messages: Some(EndpointPath::new("/messages").unwrap()),
        auth: AuthScheme::Bearer,
    }
}
fn endpoint(protocol: ProtocolProfile) -> Endpoint {
    let p = provider();
    Endpoint {
        id: EndpointId::new(if protocol == ProtocolProfile::OpenAiChat {
            "chat"
        } else {
            "messages"
        })
        .unwrap(),
        provider: p.id.clone(),
        target: EndpointTarget {
            origin: p.origin,
            path: EndpointPath::new(if protocol == ProtocolProfile::OpenAiChat {
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
        representation: Adapter::new(Profile::Chat, Dialect::OpenCodeGo, None)
            .contract(&Contract::full()),
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
#[test]
fn wire_declaration_cannot_publish_unimplemented_native_capabilities() {
    let native = endpoint(ProtocolProfile::AnthropicMessages);
    let canonical = ModelId::new("synthetic-model").unwrap();
    let route = Route {
        id: RouteId::new("synthetic-route").unwrap(),
        task: TaskKind::Generation,
        endpoints: vec![native.id.clone()],
        policy: RoutePolicy::default(),
    };
    let model = PublicModel::new(
        ModelId::new("public-native").unwrap(),
        canonical.clone(),
        TaskKind::Generation,
        route.id.clone(),
        GenerationSemanticContract::text_images(),
    );
    assert_eq!(
        compile(
            vec![provider()],
            vec![native],
            vec![route],
            vec![model],
            vec![CanonicalModel {
                id: canonical,
                task: TaskKind::Generation,
                contract: GenerationSemanticContract::full()
            }]
        )
        .unwrap_err(),
        TopologyError::ContractUnsatisfiable,
    );
}
#[test]
fn typed_native_requests_use_the_same_admission_preflight_and_preparation() {
    use morphiecore::semantic::{task::generation::LocalScope, value::ReplayOrigin};
    let p = provider();
    let mut native = endpoint(ProtocolProfile::AnthropicMessages);
    native.representation.semantics = NativeProfile::AdaptiveTextTools.semantic_contract();
    native.representation.replay_origin = Some(ReplayOrigin::new("synthetic").unwrap());
    native.representation.standard_context = false;
    let route = Route {
        id: RouteId::new("native-route").unwrap(),
        task: TaskKind::Generation,
        endpoints: vec![native.id.clone()],
        policy: RoutePolicy::default(),
    };
    let mut model = PublicModel::new(
        ModelId::new("native-public").unwrap(),
        native.canonical_model.clone(),
        TaskKind::Generation,
        route.id.clone(),
        native.representation.semantics.clone(),
    );
    model.standard_context = false;
    compile(
        vec![p.clone()],
        vec![native.clone()],
        vec![route],
        vec![model],
        vec![CanonicalModel {
            id: native.canonical_model.clone(),
            task: TaskKind::Generation,
            contract: NativeProfile::AdaptiveTextTools.semantic_contract(),
        }],
    )
    .unwrap();
    let target =
        morphiecore::protocol::anthropic::ReplayTarget::new("synthetic", "synthetic-model")
            .unwrap();
    let decoded = NativeProfile::AdaptiveTextTools.decode_generation_request(
        br#"{"model":"synthetic-model","max_tokens":32,"messages":[{"role":"user","content":"hello"}]}"#,
        LocalScope::new(1),&target).unwrap();
    let mut request =
        morphiecore::adapter::Request::from_generation(decoded, "native-public").unwrap();
    request.cache_session = Some(CacheSession::new("synthetic-session").unwrap());
    representable(&native, &request).unwrap();
    let mut wrong_scope = native.clone();
    wrong_scope.representation.replay_origin = Some(ReplayOrigin::new("other-provider").unwrap());
    assert!(representable(&wrong_scope, &request).is_err());
    assert!(
        prepare(
            &wrong_scope,
            &p,
            &SecretMaterial::new("synthetic-secret").unwrap(),
            &request
        )
        .is_err()
    );
    let wire = prepare(
        &native,
        &p,
        &SecretMaterial::new("synthetic-secret").unwrap(),
        &request,
    )
    .unwrap();
    assert_eq!(
        wire.auth_header,
        ("x-api-key".into(), "synthetic-secret".into())
    );
    assert!(
        wire.safe_headers
            .contains(&("anthropic-version".into(), "2023-06-01".into()))
    );
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&wire.body).unwrap(),
        json!({"model":"synthetic-model","max_tokens":32,"messages":[{"role":"user","content":[{"type":"text","text":"hello"}]}]})
    );
    request.delivery.stream = morphiecore::semantic::value::Presence::Value(true);
    assert!(representable(&native, &request).is_err());
    request.delivery.stream = morphiecore::semantic::value::Presence::Null;
    assert!(representable(&native, &request).is_err());
}
#[test]
fn native_go_cache_projection_agrees_between_preflight_and_prepare() {
    use morphiecore::semantic::{
        context::{CacheMode, CacheOptions, CacheRetention, CacheTtl, ConversationContext},
        task::generation::LocalScope,
        value::{Presence, ReplayOrigin},
    };
    let p = provider();
    let mut native = endpoint(ProtocolProfile::AnthropicMessages);
    native.representation.semantics = NativeProfile::AdaptiveTextTools.semantic_contract();
    native.representation.replay_origin = Some(ReplayOrigin::new("synthetic").unwrap());
    let target =
        morphiecore::protocol::anthropic::ReplayTarget::new("synthetic", "synthetic-model")
            .unwrap();
    let decoded = NativeProfile::AdaptiveTextTools.decode_generation_request(
        br#"{"model":"synthetic-model","max_tokens":32,"messages":[{"role":"user","content":"hello"}]}"#,
        LocalScope::new(1), &target,
    ).unwrap();
    let mut request = morphiecore::adapter::Request::from_generation(decoded, "public").unwrap();
    request.conversation = Some(ConversationContext::conversation("conversation-one").unwrap());
    request.context.cache.prompt_cache_key = Presence::Value("shared-workspace".into());
    request.context.cache.prompt_cache_retention = Presence::Value(CacheRetention::InMemory);
    let original = request.clone();
    let secret = SecretMaterial::new("synthetic-key").unwrap();
    representable(&native, &request).unwrap();
    let prepared = prepare(&native, &p, &secret, &request).unwrap();
    assert!(prepared.safe_headers.contains(&(
        "x-opencode-session".into(),
        "427d18da2021fadacd82a4b187acd8b4459e929ad27d1f77025fff66310a176e".into()
    )));
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&prepared.body).unwrap(),
        json!({"model":"synthetic-model","max_tokens":32,"messages":[{"role":"user","content":[{"type":"text","text":"hello"}]}]})
    );
    assert_eq!(request, original);
    assert!(request.cache_session.is_none());
    let mut independent = request.clone();
    independent.conversation = Some(ConversationContext::conversation("conversation-two").unwrap());
    independent.context.cache.prompt_cache_key = Presence::Value("shared-workspace".into());
    representable(&native, &independent).unwrap();
    let prepared = prepare(&native, &p, &secret, &independent).unwrap();
    assert!(prepared.safe_headers.contains(&(
        "x-opencode-session".into(),
        "7577d45153ce9042751834fb6f264cc0fb919b44f4408bb37a6defc6d3256d9f".into()
    )));
    for key in [
        Presence::Absent,
        Presence::Null,
        Presence::Value(String::new()),
        Presence::Value("bad\nheader".into()),
        Presence::Value("中文".into()),
    ] {
        let mut edited = request.clone();
        edited.context.cache.prompt_cache_key = key;
        representable(&native, &edited).unwrap();
        prepare(&native, &p, &secret, &edited).unwrap();
    }
    let mut missing = request.clone();
    missing.conversation = None;
    assert!(representable(&native, &missing).is_err());
    assert!(prepare(&native, &p, &secret, &missing).is_err());
    let mut no_carrier = native.clone();
    no_carrier.representation.cache.session_id = false;
    assert!(representable(&no_carrier, &request).is_err());
    assert!(prepare(&no_carrier, &p, &secret, &request).is_err());
    request.context.cache.prompt_cache_options = Presence::Value(CacheOptions {
        mode: CacheMode::Explicit,
        ttl: CacheTtl::ThirtyMinutes,
        comparison_response_id: Presence::Absent,
        prewarm: Presence::Absent,
    });
    assert!(representable(&native, &request).is_err());
    assert!(prepare(&native, &p, &secret, &request).is_err());
}
#[test]
fn one_provider_keeps_operation_paths_auth_and_credential_domain_separate() {
    let p = provider();
    let chat = endpoint(ProtocolProfile::OpenAiChat);
    let native = endpoint(ProtocolProfile::AnthropicMessages);
    compile_endpoints(p.clone(), vec![chat.clone(), native.clone()]).unwrap();
    assert_eq!(
        p.generation_entry(GenerationOperation::ChatCompletions)
            .unwrap()
            .auth,
        AuthScheme::Bearer
    );
    assert_eq!(
        p.generation_entry(GenerationOperation::AnthropicMessages)
            .unwrap()
            .auth,
        AuthScheme::ApiKeyHeader("x-api-key")
    );
    let secret = SecretMaterial::new("synthetic-secret").unwrap();
    let mut request = Adapter::new(Profile::Chat, Dialect::Standard, None)
        .decode_request(br#"{"model":"public","messages":[{"role":"user","content":"hello"}]}"#)
        .unwrap();
    request.conversation = Some(
        morphiecore::semantic::context::ConversationContext::conversation("synthetic-conversation")
            .unwrap(),
    );
    assert_eq!(
        prepare(&chat, &p, &secret, &request).unwrap().auth_header,
        ("authorization".into(), "Bearer synthetic-secret".into())
    );
    let request = NativeProfile::AdaptiveTextTools.decode_request(
        br#"{"model":"synthetic-model","max_tokens":32,"messages":[{"role":"user","content":"hello"}]}"#).unwrap();
    let before = request.clone();
    let session = CacheSession::new("synthetic-session").unwrap();
    let wire = prepare_messages(&native, &p, &secret, &request, Some(&session)).unwrap();
    assert_eq!(wire.path, "/messages");
    assert_eq!(
        wire.auth_header,
        ("x-api-key".into(), "synthetic-secret".into())
    );
    assert!(
        wire.safe_headers
            .contains(&("anthropic-version".into(), "2023-06-01".into()))
    );
    assert!(
        wire.safe_headers
            .contains(&("x-opencode-session".into(), "synthetic-session".into()))
    );
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&wire.body).unwrap(),
        json!({"model":"synthetic-model","max_tokens":32,"messages":[{"role":"user","content":"hello"}]})
    );
    assert_eq!(request, before);
    assert!(!format!("{wire:?}").contains("synthetic-secret"));
    assert!(prepare_messages(&native, &p, &secret, &request, None).is_err());
    assert!(
        prepare_messages(
            &native,
            &p,
            &secret,
            &request,
            Some(&CacheSession::new("中文").unwrap())
        )
        .is_err()
    );
}
#[test]
fn native_binding_mismatches_and_unwired_semantic_paths_fail_closed() {
    let p = provider();
    let native = endpoint(ProtocolProfile::AnthropicMessages);
    assert!(matches!(
        native.adapter(),
        UpstreamAdapter::AnthropicMessages(_)
    ));
    let chat_request = Adapter::new(Profile::Chat, Dialect::Standard, None)
        .decode_request(br#"{"model":"public","messages":[{"role":"user","content":"hello"}]}"#)
        .unwrap();
    assert!(representable(&native, &chat_request).is_err());
    let mut attempt = Attempt::new(native.adapter(), 4096, SseLimits::default());
    assert!(attempt.begin(200, "application/json").is_err());
    assert!(attempt.finish().is_err());

    let target =
        morphiecore::protocol::anthropic::ReplayTarget::new("synthetic", "synthetic-model")
            .unwrap();
    let source = NativeProfile::AdaptiveTextTools.decode_generation_request(
        br#"{"model":"synthetic-model","max_tokens":32,"messages":[{"role":"user","content":"hello"}]}"#,
        morphiecore::semantic::task::generation::LocalScope::new(1),&target).unwrap();
    let body = br#"{"id":"r","type":"message","role":"assistant","model":"synthetic-model","content":[{"type":"text","text":"answer"}],"stop_reason":"end_turn","usage":{"input_tokens":1,"output_tokens":2}}"#;
    for fragment in [1, 7, body.len()] {
        let mut attempt = Attempt::native_messages(
            source.clone(),
            target.clone(),
            morphiecore::semantic::task::generation::LocalScope::new(2),
            4096,
        )
        .unwrap();
        attempt.begin(200, "application/json").unwrap();
        for chunk in body.chunks(fragment) {
            assert!(attempt.push(chunk).unwrap().1.is_empty());
        }
        assert!(attempt.response().is_err());
        assert_eq!(attempt.finish().unwrap().metadata.created, None);
    }
    let mut attempt = Attempt::native_messages(
        source.clone(),
        target.clone(),
        morphiecore::semantic::task::generation::LocalScope::new(2),
        4096,
    )
    .unwrap();
    attempt.begin(200, "application/json").unwrap();
    attempt.push(body).unwrap();
    attempt.push(b"x").unwrap();
    assert!(attempt.finish().is_err());
    assert!(attempt.begin(200, "application/json").is_err());
    let mut attempt = Attempt::native_messages(
        source,
        target,
        morphiecore::semantic::task::generation::LocalScope::new(2),
        4096,
    )
    .unwrap();
    assert!(attempt.begin(200, "text/event-stream").is_err());
    assert!(attempt.finish().is_err());
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
    let mut bad = p.clone();
    bad.messages = None;
    assert_eq!(
        compile_endpoints(bad, vec![native.clone()]).unwrap_err(),
        TopologyError::TargetMismatch
    );
    let secret = SecretMaterial::new("synthetic-secret").unwrap();
    let mut request=NativeProfile::AdaptiveTextTools.decode_request(
        br#"{"model":"synthetic-model","max_tokens":32,"messages":[{"role":"user","content":"hello"}]}"#).unwrap();
    let session = CacheSession::new("synthetic-session").unwrap();
    request.model = "other-model".into();
    assert!(prepare_messages(&native, &p, &secret, &request, Some(&session)).is_err());
    request.model = "synthetic-model".into();
    request.stream = morphiecore::semantic::value::Presence::Value(true);
    assert!(prepare_messages(&native, &p, &secret, &request, Some(&session)).is_err());
}
