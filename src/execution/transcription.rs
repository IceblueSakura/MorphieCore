//! Pure preparation of a single native transcription attempt.
use super::{AttemptError, UpstreamRequest};
use crate::{
    adapter::transcription::Request,
    provider::{CredentialKind, ProviderDefinition, SecretMaterial},
    topology::transcription::TranscriptionRoute,
};
pub fn prepare(
    route: &TranscriptionRoute,
    provider: &ProviderDefinition,
    secret: &SecretMaterial,
    request: &Request,
) -> Result<UpstreamRequest, AttemptError> {
    let endpoint = &route.endpoint;
    if request.model != route.model.as_str()
        || provider.id != endpoint.provider
        || provider.origin != endpoint.target.origin
        || provider.auth.kind() != CredentialKind::ApiKey
        || endpoint.execution.credential_kind != CredentialKind::ApiKey
        || endpoint.execution.streaming
        || endpoint.execution.retry_before_commit
    {
        return Err(AttemptError::Protocol("transcription binding"));
    }
    let value = endpoint
        .profile
        .encode_request(request, &endpoint.upstream_model)?;
    crate::semantic::value::json_size(&value, endpoint.execution.request_body_limit)
        .map_err(|_| AttemptError::Limit)?;
    Ok(UpstreamRequest {
        origin: endpoint.target.origin.as_str().into(),
        method: "POST",
        path: endpoint.target.path.as_str().into(),
        safe_headers: vec![
            ("content-type".into(), "application/json".into()),
            ("accept".into(), "application/json".into()),
            ("x-dashscope-sse".into(), "disable".into()),
        ],
        auth_header: provider.auth.auth_header(secret),
        body: serde_json::to_vec(&value)
            .map_err(|_| AttemptError::Protocol("transcription serialization"))?,
    })
}
