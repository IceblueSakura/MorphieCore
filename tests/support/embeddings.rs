//! Independent fixed text-vector binding, never derived from the product catalog.
use morphiecore::{
    adapter::embeddings::{Contract, DimensionSupport, Profile},
    provider::{CredentialBindingId, CredentialKind, EndpointPath, ProviderDefinition},
    topology::{
        EndpointId, EndpointTarget, ExecutionContract, ModelId, RouteId,
        embeddings::{EmbeddingEndpoint, EmbeddingRoute, ProviderEntry},
    },
};
pub fn binding(provider: &ProviderDefinition) -> (ProviderEntry, EmbeddingRoute) {
    let path = EndpointPath::new("/embeddings").unwrap();
    let canonical = ModelId::new("canonical-vector").unwrap();
    (
        ProviderEntry {
            provider: provider.id.clone(),
            path: path.clone(),
        },
        EmbeddingRoute {
            id: RouteId::new("fixture-embeddings").unwrap(),
            model: ModelId::new("public-vector").unwrap(),
            canonical_model: canonical.clone(),
            endpoint: EmbeddingEndpoint {
                id: EndpointId::new("fixture-embeddings").unwrap(),
                provider: provider.id.clone(),
                target: EndpointTarget {
                    origin: provider.origin.clone(),
                    path,
                },
                upstream_model: "private-vector".into(),
                canonical_model: canonical,
                profile: Profile::DashScope,
                contract: Contract {
                    max_inputs: 4,
                    dimensions: DimensionSupport::UpTo(16),
                    user: false,
                },
                credential: CredentialBindingId::new("fixture-key").unwrap(),
                execution: ExecutionContract {
                    streaming: false,
                    retry_before_commit: false,
                    request_body_limit: 4096,
                    response_body_limit: 4096,
                    credential_kind: CredentialKind::ApiKey,
                    timeout_ms: 3000,
                },
            },
        },
    )
}
