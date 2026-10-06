//! Small ingress fixture independent of product registration and publication data.
use crate::{
    adapter::{Adapter, Dialect},
    lowering::generation::GenerationRepresentationContract,
    protocol::openai::Profile,
    provider::{
        AuthScheme, CredentialBindingId, CredentialKind, EndpointPath, ProviderDefinition,
        ProviderId, TrustedOrigin,
    },
    semantic::value::ReplayOrigin,
    topology::{
        self, CanonicalModel, CompiledTopology, Endpoint, EndpointId, EndpointTarget,
        ExecutionContract, GenerationSemanticContract, ModelId, ModelMetadata, ProtocolProfile,
        PublicModel, Route, RouteId, TaskKind,
    },
};

pub(crate) fn topology() -> CompiledTopology {
    let provider = ProviderDefinition {
        id: ProviderId::new("fixture").unwrap(),
        origin: TrustedOrigin::parse("http://127.0.0.1:9").unwrap(),
        chat_completions: Some(EndpointPath::new("/chat/completions").unwrap()),
        responses: Some(EndpointPath::new("/responses").unwrap()),
        auth: AuthScheme::Bearer,
    };
    let canonical = ModelId::new("fixture-canonical").unwrap();
    let execution = ExecutionContract {
        streaming: true,
        retry_before_commit: false,
        request_body_limit: 256 << 10,
        response_body_limit: 1 << 20,
        timeout_ms: 1000,
        credential_kind: CredentialKind::ApiKey,
    };
    let endpoints = [
        (
            "fixture-chat",
            Profile::Chat,
            ProtocolProfile::OpenAiChat,
            false,
        ),
        (
            "fixture-responses",
            Profile::Responses,
            ProtocolProfile::OpenAiResponses,
            false,
        ),
        (
            "limited-responses",
            Profile::Responses,
            ProtocolProfile::OpenAiResponses,
            true,
        ),
    ]
    .into_iter()
    .map(|(id, profile, protocol, limited)| {
        let scope = ReplayOrigin::new("synthetic-source").unwrap();
        let mut representation = GenerationRepresentationContract {
            adaptation: Adapter::new(profile, Dialect::MorphieCore, Some(scope.clone())).adaptation,
            replay_origin: Some(scope),
            ..GenerationRepresentationContract::full()
        };
        representation.semantics.temperature = !limited;
        Endpoint {
            id: EndpointId::new(id).unwrap(),
            provider: provider.id.clone(),
            target: EndpointTarget {
                origin: provider.origin.clone(),
                path: if profile == Profile::Chat {
                    provider.chat_completions.clone().unwrap()
                } else {
                    provider.responses.clone().unwrap()
                },
            },
            task: TaskKind::Generation,
            protocol,
            upstream_model: "private-fixture".into(),
            canonical_model: canonical.clone(),
            representation,
            execution: execution.clone(),
            credential: CredentialBindingId::new("fixture-key").unwrap(),
        }
    })
    .collect();
    let routes = [
        ("fixture-route", vec!["fixture-responses", "fixture-chat"]),
        ("limited-route", vec!["limited-responses"]),
    ]
    .into_iter()
    .map(|(id, endpoints)| Route {
        id: RouteId::new(id).unwrap(),
        task: TaskKind::Generation,
        endpoints: endpoints
            .into_iter()
            .map(|id| EndpointId::new(id).unwrap())
            .collect(),
        policy: topology::RoutePolicy::default(),
    })
    .collect();
    let models = [
        ("fixture-model", "fixture-route"),
        ("limited-model", "limited-route"),
    ]
    .into_iter()
    .map(|(id, route)| {
        let mut contract = GenerationSemanticContract::full();
        contract.temperature = id != "limited-model";
        PublicModel::new(
            ModelId::new(id).unwrap(),
            canonical.clone(),
            TaskKind::Generation,
            RouteId::new(route).unwrap(),
            contract,
        )
    })
    .collect();
    let image = ModelId::new("fixture-image").unwrap();
    let image_path = EndpointPath::new("/images/generations").unwrap();
    topology::compile(
        vec![provider.clone()],
        endpoints,
        routes,
        models,
        vec![CanonicalModel {
            id: canonical.clone(),
            task: TaskKind::Generation,
            contract: GenerationSemanticContract::full(),
        }],
    )
    .unwrap()
    .with_images(
        vec![topology::images::ProviderEntry {
            provider: provider.id.clone(),
            path: image_path.clone(),
        }],
        vec![topology::images::ImageRoute {
            id: RouteId::new("fixture-images").unwrap(),
            model: image.clone(),
            canonical_model: image.clone(),
            accounting: crate::lowering::images::AccountingPolicy::Strict,
            endpoint: topology::images::ImageEndpoint {
                id: EndpointId::new("fixture-images").unwrap(),
                provider: provider.id,
                target: EndpointTarget {
                    origin: provider.origin,
                    path: image_path,
                },
                upstream_model: "private-image".into(),
                canonical_model: image.clone(),
                profile: crate::adapter::images::Profile::GptImage,
                credential: CredentialBindingId::new("fixture-key").unwrap(),
                execution: ExecutionContract {
                    streaming: false,
                    ..execution
                },
            },
        }],
    )
    .unwrap()
    .with_model_metadata([
        (
            canonical,
            ModelMetadata::new(7, "Synthetic Developer").unwrap(),
        ),
        (
            image,
            ModelMetadata::new(8, "Synthetic Image Developer").unwrap(),
        ),
    ])
    .unwrap()
}
