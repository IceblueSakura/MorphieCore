//! Explicit Speech bindings; product admission never follows Chat activation.
use crate::{
    adapter::speech::UpstreamAdapter,
    protocol::speech::projection::SpeechCapabilities,
    provider::{CredentialBindingId, CredentialKind, ProviderOperation},
    semantic::value::AudioEncoding,
    topology::{
        EndpointId, EndpointTarget, ExecutionContract, ModelId, ModelMetadata, RouteId,
        speech::{ProviderEntry, SpeechEndpoint, SpeechRoute},
    },
};
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpeechBinding {
    pub(super) provider: &'static str,
    pub identity: &'static str,
    pub profile: UpstreamAdapter,
    pub model: &'static str,
    pub upstream: &'static str,
    pub voices: Box<[&'static str]>,
    pub released_at: u64,
    pub developer: &'static str,
}

impl SpeechBinding {
    pub fn provider(&self) -> crate::provider::ProviderDefinition {
        super::data::provider(self.provider).clone()
    }

    pub fn credential(&self) -> CredentialBindingId {
        CredentialBindingId::new(self.provider().id.as_str()).expect("catalog Provider identity")
    }
    pub fn metadata(&self) -> ModelMetadata {
        ModelMetadata::new(self.released_at, self.developer).expect("static publication")
    }
    pub fn operation(&self) -> ProviderEntry {
        let provider = self.provider();
        let path = provider
            .entry(ProviderOperation::Speech)
            .expect("speech entry")
            .path
            .clone();
        ProviderEntry {
            provider: provider.id,
            path,
        }
    }
    pub fn route(&self) -> SpeechRoute {
        let provider = self.provider();
        let canonical = ModelId::new(self.model).expect("static speech identity");
        SpeechRoute {
            id: RouteId::new(self.identity).expect("static route"),
            model: canonical.clone(),
            canonical_model: canonical.clone(),
            endpoint: SpeechEndpoint {
                id: EndpointId::new(self.identity).expect("static endpoint"),
                provider: provider.id,
                target: EndpointTarget {
                    origin: provider.origin,
                    path: self.operation().path,
                },
                upstream_model: self.upstream.into(),
                canonical_model: canonical,
                profile: self.profile,
                capabilities: SpeechCapabilities {
                    voices: self.voices.iter().map(|v| (*v).into()).collect(),
                    formats: if self.profile == UpstreamAdapter::Xiaomi {
                        crate::protocol::xiaomi_audio::FORMATS.to_vec()
                    } else {
                        vec![AudioEncoding::Mp3]
                    },
                    instructions: self.profile == UpstreamAdapter::Xiaomi,
                    speed: false,
                },
                credential: self.credential(),
                execution: ExecutionContract {
                    streaming: self.profile.upstream_sse(),
                    request_body_limit: 64 << 10,
                    response_body_limit: 8 << 20,
                    credential_kind: CredentialKind::ApiKey,
                    timeout_ms: 120_000,
                },
            },
        }
    }
}
