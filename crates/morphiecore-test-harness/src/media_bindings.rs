//! Explicit protocol fixtures; never selected from the product catalog.
use crate::{
    morphiecore::{
        adapter::speech::UpstreamAdapter as SpeechProfile,
        provider::{
            AuthScheme, CredentialBindingId, EndpointPath, ProviderDefinition, ProviderId,
            TrustedOrigin,
        },
        semantic::value::AudioEncoding,
        topology::{EndpointId, EndpointTarget, ModelId, RouteId, speech, transcription},
    },
    speech as speech_support,
};

fn provider(origin: &str, id: &str) -> ProviderDefinition {
    ProviderDefinition {
        id: ProviderId::new(id).unwrap(),
        origin: TrustedOrigin::parse(origin).unwrap(),
        endpoints: [].into(),
        auth: AuthScheme::Bearer,
    }
}

pub fn native_speech(
    origin: &str,
) -> (
    ProviderDefinition,
    speech::ProviderEntry,
    speech::SpeechRoute,
) {
    let provider = provider(origin, "native-fixture");
    let (_, mut route) = speech_support::binding(origin);
    route.id = RouteId::new("native-speech").unwrap();
    route.model = ModelId::new("native-speech").unwrap();
    route.canonical_model = route.model.clone();
    route.endpoint.id = EndpointId::new("native-speech").unwrap();
    route.endpoint.provider = provider.id.clone();
    route.endpoint.canonical_model = route.canonical_model.clone();
    route.endpoint.target.path =
        EndpointPath::new("/api/v1/services/audio/tts/SpeechSynthesizer").unwrap();
    route.endpoint.upstream_model = "private-native-speech".into();
    route.endpoint.profile = SpeechProfile::AliyunMp3;
    route.endpoint.capabilities.voices = vec!["native-voice".into()];
    route.endpoint.capabilities.formats = vec![AudioEncoding::Mp3];
    route.endpoint.capabilities.instructions = false;
    route.endpoint.capabilities.speed = false;
    route.endpoint.credential = CredentialBindingId::new("native-key").unwrap();
    route.endpoint.execution.streaming = true;
    let operation = speech::ProviderEntry {
        provider: provider.id.clone(),
        path: route.endpoint.target.path.clone(),
    };
    (provider, operation, route)
}

pub fn native_transcription(
    origin: &str,
) -> (
    transcription::ProviderEntry,
    transcription::TranscriptionRoute,
) {
    let (provider, _, speech) = native_speech(origin);
    let model = ModelId::new("native-transcription").unwrap();
    let path = EndpointPath::new("/api/v1/services/aigc/multimodal-generation/generation").unwrap();
    (
        transcription::ProviderEntry {
            provider: provider.id.clone(),
            path: path.clone(),
        },
        transcription::TranscriptionRoute {
            id: RouteId::new("native-transcription").unwrap(),
            model: model.clone(),
            canonical_model: model.clone(),
            endpoint: transcription::TranscriptionEndpoint {
                id: EndpointId::new("native-transcription").unwrap(),
                provider: provider.id,
                target: EndpointTarget {
                    origin: provider.origin,
                    path,
                },
                upstream_model: "private-native-asr".into(),
                canonical_model: model,
                profile: crate::morphiecore::adapter::transcription::UpstreamAdapter::AliyunFlash,
                credential: CredentialBindingId::new("native-key").unwrap(),
                execution: crate::morphiecore::topology::ExecutionContract {
                    streaming: false,
                    request_body_limit: 2 << 20,
                    ..speech.endpoint.execution
                },
            },
        },
    )
}

#[allow(dead_code)]
pub fn standard_transcription(
    origin: &str,
) -> (
    ProviderDefinition,
    transcription::ProviderEntry,
    transcription::TranscriptionRoute,
) {
    let mut provider = provider(origin, "standard-asr-fixture");
    let path = EndpointPath::new("/audio/transcriptions").unwrap();
    provider.endpoints.insert(
        crate::morphiecore::provider::ProviderOperation::Transcription,
        crate::morphiecore::provider::ProviderEndpoint {
            suffix: path.clone(),
            path: path.clone(),
            auth: None,
        },
    );
    let (_, _, speech) = native_speech(origin);
    let model = ModelId::new("standard-transcription").unwrap();
    (
        provider.clone(),
        transcription::ProviderEntry {
            provider: provider.id.clone(),
            path: path.clone(),
        },
        transcription::TranscriptionRoute {
            id: RouteId::new("standard-transcription").unwrap(),
            model: model.clone(),
            canonical_model: model.clone(),
            endpoint: transcription::TranscriptionEndpoint {
                id: EndpointId::new("standard-transcription").unwrap(),
                provider: provider.id,
                target: EndpointTarget {
                    origin: provider.origin,
                    path,
                },
                upstream_model: "private-standard-asr".into(),
                canonical_model: model,
                profile: crate::morphiecore::adapter::transcription::UpstreamAdapter::Standard,
                credential: CredentialBindingId::new("asr-key").unwrap(),
                execution: crate::morphiecore::topology::ExecutionContract {
                    streaming: true,
                    request_body_limit: 6 << 20,
                    ..speech.endpoint.execution
                },
            },
        },
    )
}
