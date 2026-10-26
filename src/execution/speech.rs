//! Prepare one trusted Speech operation. No I/O, retry or ambient authority.
use super::{AttemptError, task::PreparedTaskRequest};
use crate::{
    protocol::speech::Request, provider::ProviderDefinition, topology::speech::SpeechRoute,
};
pub fn prepare(
    route: &SpeechRoute,
    provider: &ProviderDefinition,
    request: &Request,
) -> Result<PreparedTaskRequest, AttemptError> {
    if request.model != route.model.as_str()
        || provider.id != route.endpoint.provider
        || provider.origin != route.endpoint.target.origin
        || route.endpoint.execution.streaming != route.endpoint.profile.upstream_sse()
    {
        return Err(AttemptError::Protocol(
            crate::execution::attempt::ProtocolReason::SpeechBindingMismatch,
        ));
    }
    crate::protocol::speech::projection::check_request(&request.task, &route.endpoint.capabilities)
        .map_err(|_| {
            AttemptError::Protocol(crate::execution::attempt::ProtocolReason::SpeechAdmission)
        })?;
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
    } else if route.endpoint.profile == crate::adapter::speech::UpstreamAdapter::Xiaomi {
        headers.push(("accept".into(), "application/json".into()));
    } else {
        headers.push(("accept".into(), "application/octet-stream".into()));
    }
    Ok(PreparedTaskRequest::new(
        provider,
        route.endpoint.target.path.clone(),
        crate::provider::ProviderOperation::Speech,
        headers,
        serde_json::to_vec(&value).map_err(|_| {
            AttemptError::Protocol(crate::execution::attempt::ProtocolReason::SpeechSerialization)
        })?,
    ))
}
