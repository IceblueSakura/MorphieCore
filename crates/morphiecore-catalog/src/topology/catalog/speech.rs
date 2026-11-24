//! Explicit Speech bindings; product admission never follows Chat activation.
use crate::{
    adapter::speech::UpstreamAdapter,
    protocol::speech::projection::SpeechCapabilities,
    provider::{CredentialBindingId, CredentialKind, ProviderOperation},
    semantic::value::AudioEncoding,
    topology::{
        EndpointId, EndpointTarget, ExecutionContract, ModelId, ModelMetadata, RouteId,
        TopologyError,
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
    voices: Option<Box<[&'static str]>>,
    pub released_at: u64,
    pub developer: &'static str,
}

impl SpeechBinding {
    pub(super) fn voices(&self) -> Result<&[&str], TopologyError> {
        match (self.profile, self.voices.as_deref()) {
            (UpstreamAdapter::Standard, None) => Ok(crate::protocol::openai::speech::VOICES),
            (UpstreamAdapter::Xiaomi, None) => Ok(crate::protocol::xiaomi_audio::VOICES),
            (UpstreamAdapter::Standard | UpstreamAdapter::Xiaomi, Some(_)) | (_, None) => {
                Err(TopologyError::InvalidModelBinding)
            }
            (_, Some(voices)) => Ok(voices),
        }
    }

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
                    voices: self
                        .voices()
                        .expect("validated speech voice source")
                        .iter()
                        .map(|v| (*v).into())
                        .collect(),
                    formats: match self.profile {
                        UpstreamAdapter::Standard => {
                            crate::protocol::openai::speech::FORMATS.to_vec()
                        }
                        UpstreamAdapter::Xiaomi => crate::protocol::xiaomi_audio::FORMATS.to_vec(),
                        UpstreamAdapter::AliyunMp3 => vec![AudioEncoding::Mp3],
                    },
                    instructions: matches!(
                        self.profile,
                        UpstreamAdapter::Standard | UpstreamAdapter::Xiaomi
                    ),
                    speed: self.profile == UpstreamAdapter::Standard,
                },
                credential: self.credential(),
                execution: ExecutionContract {
                    streaming: self.profile.supports_streaming(),
                    request_body_limit: 64 << 10,
                    response_body_limit: 16 << 20,
                    credential_kind: CredentialKind::ApiKey,
                    timeout_ms: 120_000,
                },
            },
        }
    }
}
