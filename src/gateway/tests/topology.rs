//! Small ingress fixture independent of product registration and publication data.
use crate::{
    protocol::openai::{Dialect, GenerationApi, GenerationCodec, TargetContract},
    provider::{
        AuthScheme, CredentialBindingId, CredentialKind, EndpointPath, ProviderDefinition,
        ProviderId, TrustedOrigin,
    },
    semantic::value::ReplayOrigin,
    topology::{
        self, CanonicalModel, CompiledTopology, Endpoint, EndpointId, EndpointTarget,
        ExecutionContract, GenerationFeatureSet, GenerationProtocol, ModelId, ModelMetadata,
        PublicModel, Route, RouteId, TaskKind,
    },
};

pub(crate) fn topology() -> CompiledTopology {
    topology_at("http://127.0.0.1:9")
}
pub(crate) fn topology_at(origin: &str) -> CompiledTopology {
    let provider = ProviderDefinition {
        id: ProviderId::new("fixture").unwrap(),
        origin: TrustedOrigin::parse(origin).unwrap(),
        endpoints: [
            (
                crate::provider::ProviderOperation::ChatCompletions,
                crate::provider::ProviderEndpoint {
                    path: EndpointPath::new("/chat/completions").unwrap(),
                    auth: None,
                },
            ),
            (
                crate::provider::ProviderOperation::Responses,
                crate::provider::ProviderEndpoint {
                    path: EndpointPath::new("/responses").unwrap(),
                    auth: None,
                },
            ),
        ]
        .into(),
        credential_pool: None,
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
            GenerationApi::Chat,
            GenerationProtocol::OpenAiChat,
            false,
        ),
        (
            "fixture-responses",
            GenerationApi::Responses,
            GenerationProtocol::OpenAiResponses,
            false,
        ),
        (
            "limited-responses",
            GenerationApi::Responses,
            GenerationProtocol::OpenAiResponses,
            true,
        ),
    ]
    .into_iter()
    .map(|(id, profile, protocol, limited)| {
        let scope = ReplayOrigin::new("synthetic-source").unwrap();
        let mut representation = TargetContract {
            adaptation: GenerationCodec::new(profile, Dialect::MorphieCore, Some(scope.clone()))
                .adaptation,
            replay_origin: Some(scope),
            ..TargetContract::full()
        };
        representation.semantics.temperature = !limited;
        Endpoint {
            id: EndpointId::new(id).unwrap(),
            provider: provider.id.clone(),
            target: EndpointTarget {
                origin: provider.origin.clone(),
                path: if profile == GenerationApi::Chat {
                    provider
                        .entry(crate::provider::ProviderOperation::ChatCompletions)
                        .unwrap()
                        .path
                        .clone()
                } else {
                    provider
                        .entry(crate::provider::ProviderOperation::Responses)
                        .unwrap()
                        .path
                        .clone()
                },
            },
            task: TaskKind::Generation,
            protocol,
            upstream_model: "private-fixture".into(),
            canonical_model: canonical.clone(),
            representation: representation.into(),
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
        let mut contract = GenerationFeatureSet::full();
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
            contract: GenerationFeatureSet::full(),
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
            accounting: crate::protocol::image::projection::AccountingPolicy::Strict,
            endpoint: topology::images::ImageEndpoint {
                id: EndpointId::new("fixture-images").unwrap(),
                provider: provider.id,
                target: EndpointTarget {
                    origin: provider.origin,
                    path: image_path,
                },
                upstream_model: "private-image".into(),
                canonical_model: image.clone(),
                profile: crate::adapter::images::UpstreamAdapter::GptImage,
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
