//! Native ASR with isolated Token Plan and metered credential domains.
//! Sources: https://help.aliyun.com/en/model-studio/token-plan-personal-overview
//! https://help.aliyun.com/en/model-studio/fun-asr-flash-recorded-speech-recognition-http-api
use crate::{
    adapter::transcription::Profile,
    provider::{CredentialBindingId, CredentialKind, EndpointPath, ProviderDefinition, catalog},
    topology::{
        EndpointId, EndpointTarget, ExecutionContract, ModelId, ModelMetadata, RouteId,
        transcription::{ProviderEntry, TranscriptionEndpoint, TranscriptionRoute},
    },
};
pub struct TranscriptionBinding {
    pub model: &'static str,
    pub canonical_model: &'static str,
    pub upstream: &'static str,
    pub credential: &'static str,
    pub provider: fn() -> ProviderDefinition,
    pub endpoint_id: &'static str,
    pub released_at: u64,
}
pub const TRANSCRIPTION_BINDINGS: &[TranscriptionBinding] = &[
    TranscriptionBinding {
        model: "qwen-audio-3.0-asr-flash",
        canonical_model: "qwen-audio-3.0-asr-flash",
        upstream: "qwen-audio-3.0-asr-flash",
        credential: "aliyun-tokenplan-cn-api-key",
        provider: catalog::aliyun_tokenplan_cn,
        endpoint_id: "tokenplan-qwen-asr",
        // Developer release notes, 2026-07-30 UTC midnight, not plan registration.
        // https://help.aliyun.com/zh/model-studio/newly-released-models
        released_at: 1_785_369_600,
    },
    TranscriptionBinding {
        model: "qwen-audio-3.0-asr-flash-dashscope",
        canonical_model: "qwen-audio-3.0-asr-flash",
        upstream: "qwen-audio-3.0-asr-flash",
        credential: "aliyun-dashscope-cn-api-key",
        provider: catalog::aliyun_dashscope_cn,
        endpoint_id: "dashscope-qwen-asr",
        released_at: 1_785_369_600,
    },
];
impl TranscriptionBinding {
    pub fn provider(&self) -> ProviderDefinition {
        (self.provider)()
    }
    pub fn metadata(&self) -> ModelMetadata {
        ModelMetadata::new(self.released_at, "Alibaba").expect("static publication")
    }
    pub fn operation(&self) -> ProviderEntry {
        ProviderEntry {
            provider: self.provider().id,
            path: EndpointPath::new("/api/v1/services/aigc/multimodal-generation/generation")
                .expect("static ASR path"),
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
                profile: Profile::AliyunFlash,
                credential: CredentialBindingId::new(self.credential)
                    .expect("static credential binding"),
                execution: ExecutionContract {
                    streaming: false,
                    retry_before_commit: false,
                    request_body_limit: 2 << 20,
                    response_body_limit: 2 << 20,
                    credential_kind: CredentialKind::ApiKey,
                    timeout_ms: 120_000,
                },
            },
        }
    }
}
