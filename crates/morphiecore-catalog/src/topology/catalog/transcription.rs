//! Native ASR with isolated Token Plan and metered credential domains.
//! Sources: https://help.aliyun.com/en/model-studio/token-plan-personal-overview
//! https://help.aliyun.com/en/model-studio/fun-asr-flash-recorded-speech-recognition-http-api
use crate::{
    adapter::transcription::UpstreamAdapter,
    provider::{CredentialBindingId, CredentialKind, ProviderOperation},
    topology::{
        EndpointId, EndpointTarget, ExecutionContract, ModelId, RouteId,
        transcription::{ProviderEntry, TranscriptionEndpoint, TranscriptionRoute},
    },
};
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TranscriptionBinding {
    pub model: &'static str,
    pub canonical_model: &'static str,
    pub upstream: &'static str,
    pub(super) provider: &'static str,
    pub endpoint_id: &'static str,
    pub profile: UpstreamAdapter,
}

impl TranscriptionBinding {
    pub fn provider(&self) -> crate::provider::ProviderDefinition {
        super::data::provider(self.provider).clone()
    }

    pub fn credential(&self) -> CredentialBindingId {
        CredentialBindingId::new(self.provider().id.as_str()).expect("catalog Provider identity")
    }
    pub fn operation(&self) -> ProviderEntry {
        let provider = self.provider();
        let path = provider
            .entry(ProviderOperation::Transcription)
            .expect("transcription entry")
            .path
            .clone();
        ProviderEntry {
            provider: provider.id,
            path,
        }
    }
    pub fn route(&self) -> TranscriptionRoute {
        let provider = self.provider();
        let canonical = ModelId::new(self.canonical_model).expect("static ASR identity");
        TranscriptionRoute {
            id: RouteId::new(self.endpoint_id).expect("static route"),
            model: ModelId::new(self.model).expect("static public ASR label"),
            canonical_model: canonical.clone(),
            endpoint: TranscriptionEndpoint {
                id: EndpointId::new(self.endpoint_id).expect("static endpoint"),
                provider: provider.id,
                target: EndpointTarget {
                    origin: provider.origin,
                    path: self.operation().path,
                },
                upstream_model: self.upstream.into(),
                canonical_model: canonical,
                profile: self.profile,
                credential: self.credential(),
                execution: ExecutionContract {
                    streaming: self.profile.supports_streaming(),
                    request_body_limit: 6 << 20,
                    response_body_limit: 2 << 20,
                    credential_kind: CredentialKind::ApiKey,
                    timeout_ms: 120_000,
                },
            },
        }
    }
}
