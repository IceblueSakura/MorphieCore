//! Frozen standalone request bytes and trusted authorization, without I/O.
use super::{AttemptError, UpstreamRequest, attempt::ProtocolReason};
use crate::provider::{
    AuthScheme, EndpointPath, ProviderDefinition, ProviderId, ProviderOperation, SecretMaterial,
    TrustedOrigin,
};

pub struct PreparedTaskRequest {
    provider: ProviderId,
    origin: TrustedOrigin,
    path: EndpointPath,
    auth: AuthScheme,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}
impl PreparedTaskRequest {
    pub(crate) fn new(
        provider: &ProviderDefinition,
        path: EndpointPath,
        operation: ProviderOperation,
        headers: Vec<(String, String)>,
        body: Vec<u8>,
    ) -> Self {
        Self {
            provider: provider.id.clone(),
            origin: provider.origin.clone(),
            path,
            auth: provider
                .entry(operation)
                .map_or(provider.auth, |entry| entry.auth),
            headers,
            body,
        }
    }
    /// Resolving a secret never re-encodes the task or changes its trusted target.
    pub fn authorize(
        self,
        provider: &ProviderDefinition,
        secret: &SecretMaterial,
    ) -> Result<UpstreamRequest, AttemptError> {
        if provider.id != self.provider || provider.origin != self.origin {
            return Err(AttemptError::Protocol(
                ProtocolReason::ProviderEndpointMismatch,
            ));
        }
        Ok(UpstreamRequest {
            origin: self.origin.as_str().into(),
            path: self.path.as_str().into(),
            method: "POST",
            safe_headers: self.headers,
            auth_header: self.auth.auth_header(secret),
            body: self.body,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preparation_freezes_wire_and_authorization_rejects_foreign_targets() {
        let topology = crate::topology::catalog::default_topology().unwrap();
        let mut route = topology
            .speech_route("qwen-audio-3.0-tts-plus")
            .unwrap()
            .clone();
        let provider = topology.provider("aliyun-tokenplan-cn").unwrap();
        let mut request = crate::protocol::speech::Request::new(
            route.model.as_str(),
            crate::semantic::task::speech_synthesis::SpeechRequest::new(
                "original",
                "longanlingxin",
            )
            .unwrap(),
        );
        let prepare = || super::super::speech::prepare(&route, provider, &request).unwrap();
        for foreign in [
            ProviderDefinition {
                id: ProviderId::new("foreign").unwrap(),
                ..provider.clone()
            },
            ProviderDefinition {
                origin: TrustedOrigin::parse("https://foreign.invalid").unwrap(),
                ..provider.clone()
            },
        ] {
            assert!(
                prepare()
                    .authorize(&foreign, &SecretMaterial::new("synthetic").unwrap())
                    .is_err()
            );
        }
        let prepared = prepare();
        request.task =
            crate::semantic::task::speech_synthesis::SpeechRequest::new("changed", "longanlingxin")
                .unwrap();
        route.endpoint.target.path = EndpointPath::new("/changed").unwrap();
        let wire = prepared
            .authorize(provider, &SecretMaterial::new("synthetic").unwrap())
            .unwrap();
        assert_ne!(wire.path, route.endpoint.target.path.as_str());
        let json: serde_json::Value = serde_json::from_slice(&wire.body).unwrap();
        assert_eq!(json["input"]["text"], "original");
        assert_ne!(json["input"]["text"], request.task.input());
    }
}
