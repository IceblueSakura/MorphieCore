//! Prepare one trusted Speech operation. No I/O, retry or ambient authority.
use super::{AttemptError, UpstreamRequest};
use crate::{
    adapter::speech::Request,
    provider::{ProviderDefinition, SecretMaterial},
    topology::speech::SpeechRoute,
};
pub fn prepare(
    route: &SpeechRoute,
    provider: &ProviderDefinition,
    secret: &SecretMaterial,
    request: &Request,
) -> Result<UpstreamRequest, AttemptError> {
    if request.model != route.model.as_str()
        || provider.id != route.endpoint.provider
        || provider.origin != route.endpoint.target.origin
        || route.endpoint.execution.streaming != route.endpoint.profile.upstream_sse()
    {
        return Err(AttemptError::Protocol("speech binding mismatch"));
    }
    crate::lowering::speech::check_request(&request.task, &route.endpoint.capabilities)
        .map_err(|_| AttemptError::Protocol("speech admission"))?;
    let value = route
        .endpoint
        .profile
        .encode_request(request, &route.endpoint.upstream_model)?;
    crate::semantic::value::json_size(&value, route.endpoint.execution.request_body_limit)
        .map_err(|_| AttemptError::Limit)?;
    let mut headers = vec![("content-type".into(), "application/json".into())];
    if route.endpoint.profile.upstream_sse() {
        headers.push(("accept".into(), "text/event-stream".into()));
        headers.push(("x-dashscope-sse".into(), "enable".into()));
    } else {
        headers.push(("accept".into(), "application/octet-stream".into()));
    }
    Ok(UpstreamRequest {
        origin: route.endpoint.target.origin.as_str().into(),
        method: "POST",
        path: route.endpoint.target.path.as_str().into(),
        safe_headers: headers,
        auth_header: provider.auth.auth_header(secret),
        body: serde_json::to_vec(&value)
            .map_err(|_| AttemptError::Protocol("speech serialization"))?,
    })
}
