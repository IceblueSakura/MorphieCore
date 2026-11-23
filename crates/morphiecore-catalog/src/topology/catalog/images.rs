//! Explicit image-only product binding, never inferred from Chat vision admission.
use crate::{
    adapter::images::UpstreamAdapter,
    protocol::image::projection::AccountingPolicy,
    provider::{CredentialBindingId, ProviderOperation},
    topology::{
        EndpointId, EndpointTarget, ExecutionContract, ModelId, ModelMetadata, RouteId,
        images::{ImageEndpoint, ImageRoute, ProviderEntry},
    },
};
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImageBinding {
    pub(super) providers: Box<[&'static str]>,
    pub model: &'static str,
    pub released_at: u64,
    pub developer: &'static str,
}

impl ImageBinding {
    pub(super) fn reviewed(&self) -> bool {
        match self.model {
            "gpt-image-2.6-flare" | "gpt-image-2.6-sunburst" => *self.providers == ["openai"],
            "qwen-image-3.0-pro" => {
                *self.providers == ["aliyun-tokenplan-cn", "aliyun-dashscope-cn"]
            }
            "gemini-nano-banana-2.1" => *self.providers == ["google"],
            "grok-imagine-image-2.0" => *self.providers == ["grok"],
            _ => false,
        }
    }
    pub fn metadata(&self) -> ModelMetadata {
        ModelMetadata::new(self.released_at, self.developer).expect("static publication")
    }
    pub(super) fn operation(provider: &str) -> ProviderEntry {
        let provider = super::data::provider(provider);
        let path = provider
            .entry(ProviderOperation::Images)
            .expect("image entry")
            .path
            .clone();
        ProviderEntry {
            provider: provider.id.clone(),
            path,
        }
    }
    pub(super) fn route(&self, provider: &str) -> ImageRoute {
        let provider = super::data::provider(provider);
        let canonical = ModelId::new(self.model).expect("static image identity");
        let identity = format!("{}-{}-images", provider.id.as_str(), self.model);
        let profile = match provider.id.as_str() {
            "openai" => UpstreamAdapter::GptImage,
            "google" => UpstreamAdapter::GeminiImage,
            "grok" => UpstreamAdapter::GrokImage,
            "aliyun-tokenplan-cn" | "aliyun-dashscope-cn" => UpstreamAdapter::QwenImage,
            _ => unreachable!("reviewed provider"),
        };
        ImageRoute {
            id: RouteId::new(&identity).expect("static route"),
            model: canonical.clone(),
            canonical_model: canonical.clone(),
            accounting: if profile.requires_native_static() {
                AccountingPolicy::NativeStaticArtifacts
            } else {
                AccountingPolicy::Strict
            },
            endpoint: ImageEndpoint {
                id: EndpointId::new(&identity).expect("static endpoint"),
                provider: provider.id.clone(),
                target: EndpointTarget {
                    origin: provider.origin.clone(),
                    path: Self::operation(provider.id.as_str()).path,
                },
                upstream_model: self.model.into(),
                canonical_model: canonical,
                profile,
                credential: CredentialBindingId::new(provider.id.as_str())
                    .expect("catalog Provider identity"),
                execution: ExecutionContract {
                    streaming: false,
                    request_body_limit: 256 << 10,
                    response_body_limit: 4 << 20,
                    credential_kind: provider.auth.kind(),
                    timeout_ms: 120_000,
                },
            },
        }
    }
}
