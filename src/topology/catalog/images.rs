//! Explicit image-only product binding, never inferred from Chat vision admission.
use crate::{
    adapter::images::Profile,
    lowering::images::AccountingPolicy,
    provider::{CredentialBindingId, CredentialKind, EndpointPath, catalog},
    topology::{
        EndpointId, EndpointTarget, ExecutionContract, ModelId, ModelMetadata, RouteId,
        images::{ImageEndpoint, ImageRoute, ProviderEntry},
    },
};
pub struct ImageBinding {
    pub model: &'static str,
    pub upstream: &'static str,
    pub credential: &'static str,
    pub released_at: u64,
    pub developer: &'static str,
}
/// Source: https://openrouter.ai/api/v1/images/models/openai/gpt-image-2.5-flare/endpoints
pub const IMAGE_BINDINGS: &[ImageBinding] = &[ImageBinding {
    model: "gpt-image-2.5-flare",
    upstream: "openai/gpt-image-2.5-flare",
    credential: "openrouter-api-key",
    // Source: https://openai.com/index/introducing-chatgpt-images-2-5/ (2026-09-08).
    released_at: 1_788_825_600,
    developer: "OpenAI",
}];
impl ImageBinding {
    pub fn metadata(&self) -> ModelMetadata {
        ModelMetadata::new(self.released_at, self.developer).expect("static publication")
    }
    pub fn provider(&self) -> crate::provider::ProviderDefinition {
        catalog::openrouter()
    }
    pub fn operation(&self) -> ProviderEntry {
        ProviderEntry {
            provider: self.provider().id,
            path: EndpointPath::new("/api/v1/images").expect("static Images path"),
        }
    }
    pub fn route(&self) -> ImageRoute {
        let provider = self.provider();
        let canonical = ModelId::new(self.model).expect("static image identity");
        ImageRoute {
            id: RouteId::new("openrouter-flare-images").expect("static route"),
            model: canonical.clone(),
            canonical_model: canonical.clone(),
            accounting: AccountingPolicy::OmitUnrepresentableAccounting,
            endpoint: ImageEndpoint {
                id: EndpointId::new("openrouter-flare-images").expect("static endpoint"),
                provider: provider.id,
                target: EndpointTarget {
                    origin: provider.origin,
                    path: self.operation().path,
                },
                upstream_model: self.upstream.into(),
                canonical_model: canonical,
                profile: Profile::OpenRouterFlare,
                credential: CredentialBindingId::new(self.credential)
                    .expect("static credential binding"),
                execution: ExecutionContract {
                    streaming: false,
                    retry_before_commit: false,
                    request_body_limit: 256 << 10,
                    response_body_limit: 4 << 20,
                    credential_kind: CredentialKind::ApiKey,
                    timeout_ms: 120_000,
                },
            },
        }
    }
}
