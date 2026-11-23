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
/// Models reviewed under the one direct, static GPT Image contract.
pub(super) const REVIEWED: &[&str] = &["gpt-image-2.6-flare", "gpt-image-2.6-sunburst"];

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
        let identity = format!("openai-{}-images", self.model);
        ImageRoute {
            id: RouteId::new(&identity).expect("static route"),
            model: canonical.clone(),
            canonical_model: canonical.clone(),
            accounting: AccountingPolicy::Strict,
            endpoint: ImageEndpoint {
                id: EndpointId::new(&identity).expect("static endpoint"),
                provider: provider.id,
                target: EndpointTarget {
                    origin: provider.origin,
                    path: self.operation().path,
                },
                upstream_model: self.upstream.into(),
                canonical_model: canonical,
                profile: UpstreamAdapter::GptImage,
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
