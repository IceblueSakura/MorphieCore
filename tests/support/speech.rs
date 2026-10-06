//! Explicit synthetic speech binding; no product registration or real credentials.
use morphiecore::{
    lowering::speech::SpeechCapabilities,
    provider::{
        AuthScheme, CredentialBindingId, CredentialKind, EndpointPath, ProviderDefinition,
        ProviderId, TrustedOrigin,
    },
    semantic::value::AudioEncoding,
    topology::{
        self, CompiledTopology, EndpointId, EndpointTarget, ExecutionContract, ModelId,
        ModelMetadata, RouteId,
        speech::{ProviderEntry, SpeechEndpoint, SpeechRoute},
    },
};
pub fn provider(origin: &str) -> ProviderDefinition {
    ProviderDefinition {
        id: ProviderId::new("speech-fixture").unwrap(),
        origin: TrustedOrigin::parse(origin).unwrap(),
        chat_completions: None,
        responses: None,
        auth: AuthScheme::Bearer,
    }
}
pub fn binding(origin: &str) -> (ProviderEntry, SpeechRoute) {
    let provider = provider(origin);
    let path = EndpointPath::new("/audio/speech").unwrap();
    let canonical = ModelId::new("canonical-speech").unwrap();
    (
        ProviderEntry {
            provider: provider.id.clone(),
            path: path.clone(),
        },
        SpeechRoute {
            id: RouteId::new("speech-route").unwrap(),
            model: ModelId::new("public-speech").unwrap(),
            canonical_model: canonical.clone(),
            endpoint: SpeechEndpoint {
                id: EndpointId::new("speech-endpoint").unwrap(),
                provider: provider.id,
                target: EndpointTarget {
                    origin: provider.origin,
                    path,
                },
                upstream_model: "private-speech".into(),
                canonical_model: canonical,
                profile: morphiecore::adapter::speech::Profile::Standard,
                capabilities: SpeechCapabilities {
                    voices: vec!["alloy".into(), "coral".into()],
                    formats: vec![AudioEncoding::Mp3, AudioEncoding::Wav],
                    instructions: true,
                    speed: true,
                },
                credential: CredentialBindingId::new("speech-key").unwrap(),
                execution: ExecutionContract {
                    streaming: false,
                    retry_before_commit: false,
                    request_body_limit: 64 << 10,
                    response_body_limit: 1 << 20,
                    credential_kind: CredentialKind::ApiKey,
                    timeout_ms: 2000,
                },
            },
        },
    )
}
pub fn topology(origin: &str) -> CompiledTopology {
    let (operation, route) = binding(origin);
    topology::compile(vec![provider(origin)], vec![], vec![], vec![], vec![])
        .unwrap()
        .with_speech(vec![operation], vec![route])
        .unwrap()
        .with_model_metadata([(
            ModelId::new("canonical-speech").unwrap(),
            ModelMetadata::new(9, "Synthetic Speech Developer").unwrap(),
        )])
        .unwrap()
}
