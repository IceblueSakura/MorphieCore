//! Small trusted image binding reused by offline topology and HTTP tests.
use morphiecore::{
    provider::{CredentialBindingId, CredentialKind, EndpointPath, ProviderId, TrustedOrigin},
    topology::{
        EndpointId, EndpointTarget, ExecutionContract, ModelId, RouteId,
        images::{ImageEndpoint, ImageRoute, ProviderEntry},
    },
};
pub fn binding(origin: &str) -> (ProviderEntry, ImageRoute) {
    let provider = ProviderId::new("fixture").unwrap();
    let path = EndpointPath::new("/images/generations").unwrap();
    let canonical = ModelId::new("canonical-image").unwrap();
    (
        ProviderEntry {
            provider: provider.clone(),
            path: path.clone(),
        },
        ImageRoute {
            id: RouteId::new("image-route").unwrap(),
            model: ModelId::new("public-image").unwrap(),
            canonical_model: canonical.clone(),
            accounting: morphiecore::lowering::images::AccountingPolicy::Strict,
            endpoint: ImageEndpoint {
                id: EndpointId::new("image-endpoint").unwrap(),
                provider,
                target: EndpointTarget {
                    origin: TrustedOrigin::parse(origin).unwrap(),
                    path,
                },
                upstream_model: "private-image".into(),
                canonical_model: canonical,
                profile: morphiecore::adapter::images::Profile::GptImage,
                credential: CredentialBindingId::new("fixture-key").unwrap(),
                execution: ExecutionContract {
                    streaming: false,
                    retry_before_commit: false,
                    request_body_limit: 256 << 10,
                    response_body_limit: 4 << 20,
                    credential_kind: CredentialKind::ApiKey,
                    timeout_ms: 1000,
                },
            },
        },
    )
}
