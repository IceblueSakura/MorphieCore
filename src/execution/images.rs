//! Preparation for one compiled static image route. No I/O or implicit target selection.
use super::{AttemptError, UpstreamRequest};
use crate::{
    provider::{ProviderDefinition, SecretMaterial},
    topology::images::ImageRoute,
};
pub fn prepare(
    route: &ImageRoute,
    provider: &ProviderDefinition,
    secret: &SecretMaterial,
    request: &crate::adapter::images::Request,
) -> Result<UpstreamRequest, AttemptError> {
    if request.model != route.model.as_str()
        || provider.id != route.endpoint.provider
        || provider.origin != route.endpoint.target.origin
    {
        return Err(AttemptError::Protocol("image binding mismatch"));
    }
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
            .map_err(|_| AttemptError::Protocol("image serialization"))?,
    })
}
