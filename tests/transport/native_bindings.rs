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
fn wire_declaration_cannot_publish_a_native_semantic_model_before_mapping_exists() {
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
    let request = Adapter::new(Profile::Chat, Dialect::Standard, None)
        .decode_request(br#"{"model":"public","messages":[{"role":"user","content":"hello"}]}"#)
        .unwrap();
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
