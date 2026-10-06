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
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn openrouter_flare_is_an_image_binding_not_a_conversation_alias() {
        let topology = super::super::default_topology().unwrap();
        let binding = &IMAGE_BINDINGS[0];
        let route = topology.image_route(binding.model).unwrap();
        assert!(topology.model(binding.model).is_none());
        assert_eq!(route.endpoint.upstream_model, "openai/gpt-image-2.5-flare");
        assert_eq!(
            route.endpoint.target.origin.as_str(),
            "https://openrouter.ai"
        );
        assert_eq!(route.endpoint.target.path.as_str(), "/api/v1/images");
        assert_eq!(route.endpoint.profile, Profile::OpenRouterFlare);
        assert_eq!(
            route.accounting,
            AccountingPolicy::OmitUnrepresentableAccounting
        );
        assert!(!route.endpoint.execution.streaming);
    }
}
