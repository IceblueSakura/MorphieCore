//! Pure preparation of a single native transcription attempt.
use super::{AttemptError, task::PreparedTaskRequest};
use crate::{
    protocol::transcription::Request,
    provider::{CredentialKind, ProviderDefinition},
    topology::transcription::TranscriptionRoute,
};
pub fn prepare(
    route: &TranscriptionRoute,
    provider: &ProviderDefinition,
    request: &Request,
) -> Result<PreparedTaskRequest, AttemptError> {
    let endpoint = &route.endpoint;
    if request.model != route.model.as_str()
        || provider.id != endpoint.provider
        || provider.origin != endpoint.target.origin
        || provider.auth.kind() != CredentialKind::ApiKey
        || endpoint.execution.credential_kind != CredentialKind::ApiKey
        || endpoint.execution.streaming
    {
        return Err(AttemptError::Protocol(
            crate::execution::attempt::ProtocolReason::TranscriptionBinding,
        ));
    }
    let encoded = endpoint.profile.encode_request(
        request,
        &endpoint.upstream_model,
        endpoint.execution.request_body_limit,
    )?;
    let mut headers = vec![
        ("content-type".into(), encoded.content_type),
        (
            "accept".into(),
            endpoint
                .profile
                .expected_response_media(request.task.response_format)
                .into(),
        ),
    ];
    if endpoint.profile == crate::adapter::transcription::UpstreamAdapter::AliyunFlash {
        headers.push(("x-dashscope-sse".into(), "disable".into()));
    }
    Ok(PreparedTaskRequest::new(
        provider,
        endpoint.target.path.clone(),
        crate::provider::ProviderOperation::Transcription,
        headers,
        encoded.body,
    ))
}
