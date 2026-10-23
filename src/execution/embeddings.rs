//! Prepare one immutable embedding attempt before resolving credentials. No I/O or retries.
use super::{AttemptError, UpstreamRequest};
use crate::{
    adapter::embeddings::Request,
    lowering::embeddings::{DeliveryPolicy, Expectation, Receipt},
    provider::{EndpointPath, ProviderDefinition, ProviderId, SecretMaterial, TrustedOrigin},
    topology::embeddings::EmbeddingRoute,
};

pub struct PreparedEmbedding {
    provider: ProviderId,
    origin: TrustedOrigin,
    path: EndpointPath,
    body: Vec<u8>,
    pub expectation: Expectation,
    pub delivery: DeliveryPolicy,
    pub receipts: Vec<Receipt>,
}
impl PreparedEmbedding {
    pub fn authorize(
        self,
        provider: &ProviderDefinition,
        secret: &SecretMaterial,
    ) -> Result<UpstreamRequest, AttemptError> {
        if provider.id != self.provider || provider.origin != self.origin {
            return Err(AttemptError::Protocol("embedding binding mismatch"));
        }
        Ok(UpstreamRequest {
            origin: self.origin.as_str().into(),
            path: self.path.as_str().into(),
            method: "POST",
            safe_headers: vec![
                ("content-type".into(), "application/json".into()),
                ("accept".into(), "application/json".into()),
            ],
            auth_header: provider
                .entry(crate::provider::ProviderOperation::Embeddings)
                .map_or(provider.auth, |e| e.auth)
                .auth_header(secret),
            body: self.body,
        })
    }
}
pub fn prepare(
    route: &EmbeddingRoute,
    provider: &ProviderDefinition,
    request: &Request,
) -> Result<PreparedEmbedding, AttemptError> {
    if request.model != route.model.as_str()
        || provider.id != route.endpoint.provider
        || provider.origin != route.endpoint.target.origin
    {
        return Err(AttemptError::Protocol("embedding binding mismatch"));
    }
    let projected = route
        .endpoint
        .contract
        .prepare(request)
        .map_err(|_| AttemptError::Protocol("embedding admission"))?;
    let value = route
        .endpoint
        .profile
        .encode_request(&projected.target, &route.endpoint.upstream_model)?;
    crate::semantic::value::json_size(&value, route.endpoint.execution.request_body_limit)
        .map_err(|_| AttemptError::Limit)?;
    let body = serde_json::to_vec(&value)
        .map_err(|_| AttemptError::Protocol("embedding serialization"))?;
    Ok(PreparedEmbedding {
        provider: provider.id.clone(),
        origin: provider.origin.clone(),
        path: route.endpoint.target.path.clone(),
        body,
        expectation: projected.expectation,
        delivery: projected.delivery,
        receipts: projected.receipts,
    })
}
