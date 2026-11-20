//! Preparation for one compiled static image route. No I/O or implicit target selection.
use super::{AttemptError, task::PreparedTaskRequest};
use crate::{provider::ProviderDefinition, topology::images::ImageRoute};
pub fn prepare(
    route: &ImageRoute,
    provider: &ProviderDefinition,
    request: &crate::protocol::image::Request,
) -> Result<PreparedTaskRequest, AttemptError> {
    if request.model != route.model.as_str()
        || provider.id != route.endpoint.provider
        || provider.origin != route.endpoint.target.origin
    {
        return Err(AttemptError::Protocol(
            crate::execution::attempt::ProtocolReason::ImageBindingMismatch,
        ));
    }
    let value = route
        .endpoint
        .profile
        .encode_request(request, &route.endpoint.upstream_model)?;
    crate::semantic::value::json_size(&value, route.endpoint.execution.request_body_limit)
        .map_err(|_| AttemptError::Limit)?;
    Ok(PreparedTaskRequest::new(
        provider,
        route.endpoint.target.path.clone(),
        crate::provider::ProviderOperation::Images,
        vec![
            ("content-type".into(), "application/json".into()),
            ("accept".into(), "application/json".into()),
        ],
        serde_json::to_vec(&value).map_err(|_| {
            AttemptError::Protocol(crate::execution::attempt::ProtocolReason::ImageSerialization)
        })?,
    ))
}
