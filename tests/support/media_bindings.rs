//! Explicit protocol fixtures; never selected from the product catalog.
use super::{image_support, speech_support};
use morphiecore::{
    adapter::{images::Profile as ImageProfile, speech::Profile as SpeechProfile},
    lowering::images::AccountingPolicy,
    provider::{
        AuthScheme, CredentialBindingId, EndpointPath, ProviderDefinition, ProviderId,
        TrustedOrigin,
    },
    semantic::value::AudioEncoding,
    topology::{EndpointId, EndpointTarget, ModelId, RouteId, images, speech, transcription},
};

fn provider(origin: &str, id: &str) -> ProviderDefinition {
    ProviderDefinition {
        id: ProviderId::new(id).unwrap(),
        origin: TrustedOrigin::parse(origin).unwrap(),
        chat_completions: None,
        responses: None,
        messages: None,
        auth: AuthScheme::Bearer,
    }
}

pub fn router_images(
    origin: &str,
) -> (
    ProviderDefinition,
    images::ProviderEntry,
    images::ImageRoute,
) {
    let provider = provider(origin, "router-fixture");
    let (_, mut route) = image_support::binding(origin);
    route.id = RouteId::new("router-image").unwrap();
    route.model = ModelId::new("router-image").unwrap();
    route.canonical_model = route.model.clone();
    route.accounting = AccountingPolicy::OmitUnrepresentableAccounting;
    route.endpoint.id = EndpointId::new("router-image").unwrap();
    route.endpoint.provider = provider.id.clone();
    route.endpoint.canonical_model = route.canonical_model.clone();
    route.endpoint.target.path = EndpointPath::new("/api/v1/images").unwrap();
    // This named codec pins an upstream operation; the public label is synthetic.
    route.endpoint.upstream_model = "openai/gpt-image-2.5-flare".into();
    route.endpoint.profile = ImageProfile::OpenRouterFlare;
    route.endpoint.credential = CredentialBindingId::new("router-key").unwrap();
    let operation = images::ProviderEntry {
        provider: provider.id.clone(),
        path: route.endpoint.target.path.clone(),
    };
    (provider, operation, route)
}

pub fn router_speech(
    origin: &str,
) -> (
    ProviderDefinition,
    speech::ProviderEntry,
    speech::SpeechRoute,
) {
    let provider = provider(origin, "router-fixture");
    let (_, mut route) = speech_support::binding(origin);
    route.id = RouteId::new("router-speech").unwrap();
    route.model = ModelId::new("router-speech").unwrap();
    route.canonical_model = route.model.clone();
    route.endpoint.id = EndpointId::new("router-speech").unwrap();
    route.endpoint.provider = provider.id.clone();
    route.endpoint.canonical_model = route.canonical_model.clone();
    route.endpoint.target.path = EndpointPath::new("/api/v1/audio/speech").unwrap();
    route.endpoint.upstream_model = "private-router-speech".into();
    route.endpoint.profile = SpeechProfile::OpenRouterMp3;
    route.endpoint.capabilities.voices = vec!["router-voice".into()];
    route.endpoint.capabilities.formats = vec![AudioEncoding::Mp3];
    route.endpoint.capabilities.instructions = false;
    route.endpoint.capabilities.speed = false;
    route.endpoint.credential = CredentialBindingId::new("router-key").unwrap();
    let operation = speech::ProviderEntry {
        provider: provider.id.clone(),
        path: route.endpoint.target.path.clone(),
    };
    (provider, operation, route)
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
                profile: morphiecore::adapter::transcription::Profile::AliyunFlash,
                credential: CredentialBindingId::new("native-key").unwrap(),
                execution: morphiecore::topology::ExecutionContract {
                    streaming: false,
                    request_body_limit: 2 << 20,
                    ..speech.endpoint.execution
                },
            },
        },
    )
}
