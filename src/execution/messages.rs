//! Pure native request preparation. No loader, I/O, retries or semantic mapping.
use super::{AttemptError, UpstreamRequest};
use crate::{
    protocol::{anthropic, cache::CacheSession},
    provider::{ProviderDefinition, SecretMaterial},
    semantic::value::Presence,
    topology::Endpoint,
};
pub fn prepare_messages(
    endpoint: &Endpoint,
    provider: &ProviderDefinition,
    secret: &SecretMaterial,
    request: &anthropic::Request,
    session: Option<&CacheSession>,
) -> Result<UpstreamRequest, AttemptError> {
    let auth = super::attempt::operation_auth(endpoint, provider)?;
    let profile = endpoint.adapter().messages()?;
    if request.model != endpoint.upstream_model {
        return Err(AttemptError::Protocol("native model binding mismatch"));
    }
    if request.stream == Presence::Value(true) {
        return Err(AttemptError::Delivery(
            "native streaming intake is not wired",
        ));
    }
    let value = profile.encode_request(request)?;
    crate::semantic::value::json_size(&value, endpoint.execution.request_body_limit)
        .map_err(|_| AttemptError::Limit)?;
    let body =
        serde_json::to_vec(&value).map_err(|_| AttemptError::Protocol("native serialization"))?;
    let mut safe_headers = vec![
        ("content-type".into(), "application/json".into()),
        ("accept".into(), "application/json".into()),
    ];
    safe_headers.extend(native_headers(endpoint, session.map(CacheSession::as_str))?);
    Ok(UpstreamRequest {
        origin: endpoint.target.origin.as_str().into(),
        method: "POST",
        path: endpoint.target.path.as_str().into(),
        safe_headers,
        auth_header: auth.auth_header(secret),
        body,
    })
}
pub(super) fn native_headers(
    endpoint: &Endpoint,
    session: Option<&str>,
) -> Result<Vec<(String, String)>, AttemptError> {
    let go = endpoint.representation.adaptation.rules.opencode_go_headers;
    if go && session.is_none() || !go && session.is_some() {
        return Err(AttemptError::Protocol("native session carrier"));
    }
    let mut safe_headers = vec![
        ("anthropic-version".into(), "2023-06-01".into()),
        (
            "user-agent".into(),
            concat!("MorphieCore/", env!("CARGO_PKG_VERSION")).into(),
        ),
    ];
    if let Some(session) = session {
        if !endpoint.representation.cache.session_id {
            return Err(AttemptError::Protocol("native session header"));
        }
        let session = CacheSession::new(session)?;
        safe_headers.push(("x-opencode-session".into(), session.header_value()?.into()));
    }
    Ok(safe_headers)
}
