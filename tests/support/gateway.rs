//! Trusted synthetic topology shared by Router and explicit SDK gates.
use morphiecore::{
    gateway::{Entry, Gateway, Limits},
    lowering::generation::GenerationRepresentationContract as Contract,
    protocol::openai::Profile,
    provider::{
        AuthScheme, CredentialBindingId, CredentialKind, EndpointPath, ProviderDefinition,
        ProviderId, SecretMaterial, TrustedOrigin,
    },
    semantic::value::ReplayOrigin,
    topology::{
        CanonicalModel, Endpoint, EndpointId, EndpointTarget, ExecutionContract,
        GenerationSemanticContract, ModelId, ProtocolProfile, PublicModel, Route, RouteId,
        TaskKind, compile,
    },
};
use std::{collections::BTreeMap, sync::Arc};
#[path = "image_generation.rs"]
mod image_support;
#[path = "media_bindings.rs"]
mod media;
#[allow(dead_code)]
#[path = "speech.rs"]
mod speech_support;
pub const CLIENT_KEY: &str = "synthetic-gateway-client-token-0001";
pub fn gateway(origin: &str, limits: Limits) -> Gateway {
    let provider = ProviderDefinition {
        id: ProviderId::new("fixture").unwrap(),
        origin: TrustedOrigin::parse(origin).unwrap(),
        chat_completions: Some(EndpointPath::new("/chat/completions").unwrap()),
        responses: Some(EndpointPath::new("/responses").unwrap()),
        auth: AuthScheme::Bearer,
    };
    let endpoints = [
        ("chat", ProtocolProfile::OpenAiChat),
        ("responses", ProtocolProfile::OpenAiResponses),
    ]
    .into_iter()
    .map(|(id, protocol)| Endpoint {
        id: EndpointId::new(id).unwrap(),
        provider: provider.id.clone(),
        target: EndpointTarget {
            origin: provider.origin.clone(),
            path: if protocol == ProtocolProfile::OpenAiChat {
                provider.chat_completions.clone().expect("test Chat entry")
            } else {
                provider.responses.clone().expect("test Responses entry")
            },
        },
        task: TaskKind::Generation,
        protocol,
        upstream_model: "private-model".into(),
        canonical_model: ModelId::new("canonical-fixture").unwrap(),
        representation: Contract {
            // A synthetic upstream-only carrier must not leak through the
            // independently selected standard downstream adapter.
            adaptation: morphiecore::protocol::adaptation::Adaptation {
                rules: morphiecore::protocol::adaptation::WireRules {
                    responses_image_usage: protocol == ProtocolProfile::OpenAiResponses,
                    ..Default::default()
                },
                ..Default::default()
            },
            replay_origin: if protocol == ProtocolProfile::OpenAiResponses {
                Some(ReplayOrigin::new("fixture").unwrap())
            } else {
                None
            },
            ..Contract::full()
        },
        execution: ExecutionContract {
            streaming: true,
            retry_before_commit: false,
            request_body_limit: 256 << 10,
            response_body_limit: 8 << 20,
            timeout_ms: 3000,
            credential_kind: CredentialKind::ApiKey,
        },
        credential: CredentialBindingId::new("fixture-key").unwrap(),
    })
    .collect();
    let route = Route {
        policy: morphiecore::topology::RoutePolicy::default(),
        id: RouteId::new("fixture-route").unwrap(),
        task: TaskKind::Generation,
        endpoints: vec![
            EndpointId::new("responses").unwrap(),
            EndpointId::new("chat").unwrap(),
        ],
    };
    let models = ["public-model", "cross-model", "no-files-model"]
        .into_iter()
        .map(|label| PublicModel {
            id: ModelId::new(label).unwrap(),
            canonical_model: ModelId::new("canonical-fixture").unwrap(),
            task: TaskKind::Generation,
            route: route.id.clone(),
            contract: GenerationSemanticContract {
                file_input: label != "no-files-model",
                ..GenerationSemanticContract::full()
            },
            standard_context: true,
            reported_facts: morphiecore::lowering::generation::ReportedFactPolicy::Faithful,
        })
        .collect();
    let canonical = CanonicalModel {
        id: ModelId::new("canonical-fixture").unwrap(),
        task: TaskKind::Generation,
        contract: GenerationSemanticContract::full(),
    };
    let (image_provider, image_route) = image_support::binding(origin);
    let (router_provider, router_image_operation, router_route) = media::router_images(origin);
    let (_, router_speech_operation, router_speech) = media::router_speech(origin);
    let (native_provider, native_speech_operation, native_speech) = media::native_speech(origin);
    let (asr_operation, asr) = media::native_transcription(origin);
    let topology = compile(
        vec![
            provider,
            router_provider,
            speech_support::provider(origin),
            native_provider,
        ],
        endpoints,
        vec![route],
        models,
        vec![canonical],
    )
    .unwrap()
    .with_images(
        vec![image_provider, router_image_operation],
        vec![image_route, router_route],
    )
    .unwrap()
    .with_speech(
        vec![
            speech_support::binding(origin).0,
            router_speech_operation,
            native_speech_operation,
        ],
        vec![
            speech_support::binding(origin).1,
            router_speech,
            native_speech,
        ],
    )
    .unwrap()
    .with_transcriptions(vec![asr_operation], vec![asr])
    .unwrap()
    .with_model_metadata([
        (
            ModelId::new("canonical-fixture").unwrap(),
            morphiecore::topology::ModelMetadata::new(7, "Synthetic Developer").unwrap(),
        ),
        (
            ModelId::new("canonical-image").unwrap(),
            morphiecore::topology::ModelMetadata::new(8, "Synthetic Image Developer").unwrap(),
        ),
        (
            ModelId::new("router-image").unwrap(),
            morphiecore::topology::ModelMetadata::new(10, "Synthetic Router").unwrap(),
        ),
        (
            ModelId::new("canonical-speech").unwrap(),
            morphiecore::topology::ModelMetadata::new(9, "Synthetic Speech Developer").unwrap(),
        ),
        (
            ModelId::new("router-speech").unwrap(),
            morphiecore::topology::ModelMetadata::new(11, "Synthetic Router").unwrap(),
        ),
        (
            ModelId::new("native-speech").unwrap(),
            morphiecore::topology::ModelMetadata::new(12, "Synthetic Native").unwrap(),
        ),
        (
            ModelId::new("native-transcription").unwrap(),
            morphiecore::topology::ModelMetadata::new(13, "Synthetic Native").unwrap(),
        ),
    ])
    .unwrap();
    let entries = vec![
        Entry {
            model: "public-model".into(),
            protocol: Profile::Chat,
            endpoint: EndpointId::new("chat").unwrap(),
        },
        Entry {
            model: "public-model".into(),
            protocol: Profile::Responses,
            endpoint: EndpointId::new("responses").unwrap(),
        },
        Entry {
            model: "cross-model".into(),
            protocol: Profile::Responses,
            endpoint: EndpointId::new("chat").unwrap(),
        },
        Entry {
            model: "cross-model".into(),
            protocol: Profile::Chat,
            endpoint: EndpointId::new("responses").unwrap(),
        },
    ];
    let mut entries = entries;
    entries.push(Entry {
        model: "no-files-model".into(),
        protocol: Profile::Responses,
        endpoint: EndpointId::new("responses").unwrap(),
    });
    let credentials = BTreeMap::from([
        (
            CredentialBindingId::new("fixture-key").unwrap(),
            Arc::new(SecretMaterial::new("synthetic-upstream-credential-0001").unwrap()),
        ),
        (
            CredentialBindingId::new("router-key").unwrap(),
            Arc::new(SecretMaterial::new("synthetic-router-credential-0001").unwrap()),
        ),
        (
            CredentialBindingId::new("speech-key").unwrap(),
            Arc::new(SecretMaterial::new("synthetic-speech-credential-0001").unwrap()),
        ),
        (
            CredentialBindingId::new("native-key").unwrap(),
            Arc::new(SecretMaterial::new("synthetic-tokenplan-credential-0001").unwrap()),
        ),
    ]);
    Gateway::new_with_media(
        topology,
        entries,
        vec![
            morphiecore::gateway::ImageEntry {
                model: "public-image".into(),
            },
            morphiecore::gateway::ImageEntry {
                model: "router-image".into(),
            },
        ],
        vec![
            morphiecore::gateway::SpeechEntry {
                model: "public-speech".into(),
            },
            morphiecore::gateway::SpeechEntry {
                model: "router-speech".into(),
            },
            morphiecore::gateway::SpeechEntry {
                model: "native-speech".into(),
            },
        ],
        vec![morphiecore::gateway::TranscriptionEntry {
            model: "native-transcription".into(),
        }],
        credentials,
        SecretMaterial::new(CLIENT_KEY).unwrap(),
        limits,
        None,
    )
    .unwrap()
}
