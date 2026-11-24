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
        || route.endpoint.execution.streaming != route.endpoint.profile.supports_streaming()
        || (request.delivery.streaming() && !route.endpoint.execution.streaming)
    {
        return Err(AttemptError::Protocol(
            crate::execution::attempt::ProtocolReason::ImageBindingMismatch,
        ));
    }
    let edit = request.task.is_edit();
    let encoded = route.endpoint.profile.encode_request(
        request,
        &route.endpoint.upstream_model,
        route.endpoint.execution.request_body_limit,
    )?;
    let accept = if request.delivery.streaming() {
        "text/event-stream"
    } else {
        "application/json"
    };
    Ok(PreparedTaskRequest::new(
        provider,
        route.endpoint.request_path(edit).map_err(|_| {
            AttemptError::Protocol(crate::execution::attempt::ProtocolReason::ImageBindingMismatch)
        })?,
        if edit {
            crate::provider::ProviderOperation::ImagesEdit
        } else {
            crate::provider::ProviderOperation::Images
        },
        vec![
            ("content-type".into(), encoded.content_type),
            ("accept".into(), accept.into()),
        ],
        encoded.body,
    ))
}
