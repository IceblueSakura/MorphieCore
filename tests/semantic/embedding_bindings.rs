//! Target admission and product wiring are separate from the pure vector codec.
use morphiecore::{
    adapter::embeddings::{Contract, DimensionSupport, Request},
    provider::{
        AuthScheme, CredentialKind, EndpointPath, ProviderDefinition, ProviderId, TrustedOrigin,
    },
    semantic::{task::embedding::EmbeddingRequest, value::Presence},
    topology::{ModelId, TopologyError, catalog, compile},
};
#[path = "../support/embeddings.rs"]
mod fixed;

#[test]
fn target_contract_rechecks_final_inputs_dimensions_and_identity_without_clipping() {
    let contract = Contract {
        max_inputs: 2,
        dimensions: DimensionSupport::Selected(vec![2, 4]),
        user: false,
    };
    let mut request = Request::new(
        "synthetic",
        EmbeddingRequest::new(vec!["a".into()]).unwrap(),
    );
    assert!(contract.admit(&request).is_ok());
    request.task.dimensions = Presence::Value(3);
    assert!(contract.admit(&request).is_err());
    assert_eq!(request.task.dimensions, Presence::Value(3));
    request.task.dimensions = Presence::Value(2);
    assert!(contract.admit(&request).is_ok());
    request.task = EmbeddingRequest::new(vec!["a".into(); 3]).unwrap();
    assert!(contract.admit(&request).is_err());
    request.task = EmbeddingRequest::new(vec!["a".into()]).unwrap();
    request.identity.user = Presence::Value("synthetic-user".into());
    assert!(contract.admit(&request).is_err());
    for dimensions in [
        DimensionSupport::Selected(vec![]),
        DimensionSupport::Selected(vec![2, 2]),
        DimensionSupport::UpTo(0),
    ] {
        assert!(
            Contract {
                dimensions,
                ..contract.clone()
            }
            .validate()
            .is_err()
        );
    }
    let capacity = Contract {
        max_inputs: 128,
        dimensions: DimensionSupport::UpTo(8192),
        user: false,
    };
    let mut wide = Request::new(
        "synthetic",
        EmbeddingRequest::new(vec!["x".into(); 8]).unwrap(),
    );
    wide.task.dimensions = Presence::Value(8192);
    assert!(
        capacity.admit(&wide).is_err(),
        "explicit batch width cannot exceed the whole vector budget"
    );
    wide.task.dimensions = Presence::Value(256);
    assert!(capacity.admit(&wide).is_ok());
}

#[test]
fn registered_embedding_targets_are_task_specific_and_keep_trusted_domains() {
    // Product declarations are checked here, never used as a codec oracle.
    let topology = catalog::default_topology().unwrap();
    for binding in catalog::EMBEDDING_BINDINGS {
        let route = topology.embedding_route(binding.model).unwrap();
        assert!(topology.model(binding.model).is_none());
        assert!(topology.image_route(binding.model).is_none());
        assert!(topology.model_metadata(&route.canonical_model).is_some());
        assert!(!route.endpoint.execution.streaming);
        assert!(!route.endpoint.execution.retry_before_commit);
        assert_eq!(route.endpoint.upstream_model, binding.upstream);
        assert_eq!(route.endpoint.credential.as_str(), binding.credential);
    }
    let qwen = topology.embedding_route("qwen3.7-text-embedding").unwrap();
    assert_eq!(
        qwen.endpoint.profile,
        morphiecore::adapter::embeddings::Profile::DashScope
    );
    assert_eq!(qwen.endpoint.provider.as_str(), "aliyun-dashscope-cn");
    assert_eq!(
        qwen.endpoint.target.origin.as_str(),
        "https://dashscope.aliyuncs.com"
    );
    assert_eq!(
        qwen.endpoint.target.path.as_str(),
        "/compatible-mode/v1/embeddings"
    );
    let openai = topology.embedding_route("text-embedding-3-small").unwrap();
    assert_eq!(
        openai.endpoint.profile,
        morphiecore::adapter::embeddings::Profile::OpenRouter
    );
    assert_eq!(
        openai.endpoint.target.origin.as_str(),
        "https://openrouter.ai"
    );
    assert_eq!(openai.endpoint.target.path.as_str(), "/api/v1/embeddings");
    assert_eq!(
        openai.endpoint.upstream_model,
        "openai/text-embedding-3-small"
    );
}

#[test]
fn embedding_topology_rejects_untrusted_targets_wrong_tasks_and_execution_contracts() {
    // All rejection fixtures are local declarations; they never issue HTTP requests.
    let provider = ProviderDefinition {
        id: ProviderId::new("fixture").unwrap(),
        origin: TrustedOrigin::parse("http://127.0.0.1:9").unwrap(),
        chat_completions: None,
        responses: None,
        auth: AuthScheme::Bearer,
    };
    let (operation, route) = fixed::binding(&provider);
    let topology = || compile(vec![provider.clone()], vec![], vec![], vec![], vec![]).unwrap();
    assert!(
        topology()
            .with_embeddings(vec![operation.clone()], vec![route.clone()])
            .is_ok()
    );
    for (case, expected) in [
        (0, TopologyError::TargetMismatch),
        (1, TopologyError::TargetMismatch),
        (2, TopologyError::TargetMismatch),
        (3, TopologyError::InvalidExecutionLimits),
        (4, TopologyError::InvalidExecutionLimits),
        (5, TopologyError::InvalidModelBinding),
        (6, TopologyError::CanonicalModelMismatch),
        (7, TopologyError::ContractUnsatisfiable),
    ] {
        let mut changed = route.clone();
        match case {
            0 => {
                changed.endpoint.target.origin =
                    TrustedOrigin::parse("https://example.invalid").unwrap()
            }
            1 => changed.endpoint.target.path = EndpointPath::new("/untrusted").unwrap(),
            2 => changed.endpoint.execution.credential_kind = CredentialKind::OAuth("synthetic"),
            3 => changed.endpoint.execution.streaming = true,
            4 => changed.endpoint.execution.retry_before_commit = true,
            5 => changed.endpoint.upstream_model = "wrong model".into(),
            6 => changed.endpoint.canonical_model = ModelId::new("other").unwrap(),
            7 => changed.endpoint.contract.max_inputs = 0,
            _ => unreachable!(),
        }
        assert_eq!(
            topology()
                .with_embeddings(vec![operation.clone()], vec![changed])
                .unwrap_err(),
            expected
        );
    }
    assert_eq!(
        topology()
            .with_embeddings(vec![operation.clone()], vec![route.clone(), route])
            .unwrap_err(),
        TopologyError::DuplicateRoute
    );
}

#[test]
fn preparation_uses_final_typed_encoding_without_mutating_source_or_injecting_generation_controls()
{
    let provider = ProviderDefinition {
        id: ProviderId::new("fixture").unwrap(),
        origin: TrustedOrigin::parse("http://127.0.0.1:9").unwrap(),
        chat_completions: None,
        responses: None,
        auth: AuthScheme::Bearer,
    };
    let (_, route) = fixed::binding(&provider);
    let request = Request::new(
        "public-vector",
        EmbeddingRequest::new(vec!["synthetic text".into()]).unwrap(),
    );
    let original = request.clone();
    let secret = morphiecore::provider::SecretMaterial::new("synthetic-key").unwrap();
    let prepared =
        morphiecore::execution::embeddings::prepare(&route, &provider, &secret, &request).unwrap();
    assert_eq!(request, original);
    assert_eq!(prepared.method, "POST");
    assert_eq!(prepared.path, "/embeddings");
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&prepared.body).unwrap(),
        serde_json::json!({"model":"private-vector","input":["synthetic text"],"encoding_format":"float"})
    );
}
