//! Explicit image-only product binding, never inferred from Chat vision admission.
use crate::{
    adapter::images::UpstreamAdapter,
    protocol::image::projection::AccountingPolicy,
    provider::{CredentialBindingId, CredentialKind, ProviderOperation},
    topology::{
        EndpointId, EndpointTarget, ExecutionContract, ModelId, ModelMetadata, RouteId,
        images::{ImageEndpoint, ImageRoute, ProviderEntry},
    },
};
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImageBinding {
    pub(super) provider: &'static str,
    pub model: &'static str,
    pub upstream: &'static str,
    pub released_at: u64,
    pub developer: &'static str,
}

impl ImageBinding {
    pub fn credential(&self) -> CredentialBindingId {
        CredentialBindingId::new(self.provider().id.as_str()).expect("catalog Provider identity")
    }
    pub fn metadata(&self) -> ModelMetadata {
        ModelMetadata::new(self.released_at, self.developer).expect("static publication")
    }
    pub fn provider(&self) -> crate::provider::ProviderDefinition {
        super::data::provider(self.provider).clone()
    }
    pub fn operation(&self) -> ProviderEntry {
        let provider = self.provider();
        let path = provider
            .entry(ProviderOperation::Images)
            .expect("image entry")
            .path
            .clone();
        ProviderEntry {
            provider: provider.id,
            path,
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
                profile: UpstreamAdapter::OpenRouterFlare,
                credential: self.credential(),
                execution: ExecutionContract {
                    streaming: false,
                    request_body_limit: 256 << 10,
                    response_body_limit: 4 << 20,
                    credential_kind: CredentialKind::ApiKey,
                    timeout_ms: 120_000,
                },
            },
        }
    }
}
