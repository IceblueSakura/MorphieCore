//! Preparation of one fixed embedding attempt; no I/O, output-token cap or retry.
use super::{AttemptError, UpstreamRequest};
use crate::{
    adapter::embeddings::Request,
    provider::{ProviderDefinition, SecretMaterial},
    topology::embeddings::EmbeddingRoute,
};

pub fn prepare(
    route: &EmbeddingRoute,
    provider: &ProviderDefinition,
    secret: &SecretMaterial,
    request: &Request,
) -> Result<UpstreamRequest, AttemptError> {
    if request.model != route.model.as_str()
        || provider.id != route.endpoint.provider
        || provider.origin != route.endpoint.target.origin
    {
        return Err(AttemptError::Protocol("embedding binding mismatch"));
    }
    route
        .endpoint
        .contract
        .admit(request)
        .map_err(|_| AttemptError::Protocol("embedding admission"))?;
    let value = route
        .endpoint
        .profile
        .encode_request(request, &route.endpoint.upstream_model)?;
    crate::semantic::value::json_size(&value, route.endpoint.execution.request_body_limit)
        .map_err(|_| AttemptError::Limit)?;
    Ok(UpstreamRequest {
        origin: route.endpoint.target.origin.as_str().into(),
        method: "POST",
        path: route.endpoint.target.path.as_str().into(),
        safe_headers: vec![
            ("content-type".into(), "application/json".into()),
            ("accept".into(), "application/json".into()),
        ],
        auth_header: provider.auth.auth_header(secret),
        body: serde_json::to_vec(&value)
            .map_err(|_| AttemptError::Protocol("embedding serialization"))?,
    })
}
