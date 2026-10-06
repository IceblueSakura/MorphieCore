//! Explicit transcription identity, target and execution admission.
use super::{
    CompiledTopology, EndpointId, EndpointTarget, ExecutionContract, ModelId, RouteId,
    TopologyError,
};
use crate::provider::{CredentialBindingId, CredentialKind, EndpointPath, ProviderId};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderEntry {
    pub provider: ProviderId,
    pub path: EndpointPath,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TranscriptionEndpoint {
    pub id: EndpointId,
    pub provider: ProviderId,
    pub target: EndpointTarget,
    pub upstream_model: String,
    pub canonical_model: ModelId,
    pub profile: crate::adapter::transcription::Profile,
    pub credential: CredentialBindingId,
    pub execution: ExecutionContract,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TranscriptionRoute {
    pub id: RouteId,
    pub model: ModelId,
    pub canonical_model: ModelId,
    pub endpoint: TranscriptionEndpoint,
}
impl CompiledTopology {
    pub fn transcription_route(&self, model: &str) -> Option<&TranscriptionRoute> {
        self.transcription_routes.get(model)
    }
    pub fn with_transcriptions(
        mut self,
        entries: Vec<ProviderEntry>,
        routes: Vec<TranscriptionRoute>,
    ) -> Result<Self, TopologyError> {
        if !self.transcription_routes.is_empty() || entries.len() > 64 || routes.len() > 64 {
            return Err(TopologyError::InvalidRoutePolicy);
        }
        let mut paths = BTreeMap::new();
        for entry in entries {
            if self.provider(entry.provider.as_str()).is_none() {
                return Err(TopologyError::UnknownProvider);
            }
            if paths.insert(entry.provider, entry.path).is_some() {
                return Err(TopologyError::DuplicateProvider);
            }
        }
        let mut ids = BTreeSet::new();
        let mut endpoints = BTreeSet::new();
        for route in routes {
            let endpoint = &route.endpoint;
            let provider = self
                .provider(endpoint.provider.as_str())
                .ok_or(TopologyError::UnknownProvider)?;
            if endpoint.target.origin != provider.origin
                || paths.get(&endpoint.provider) != Some(&endpoint.target.path)
                || provider.auth.kind() != CredentialKind::ApiKey
                || endpoint.execution.credential_kind != CredentialKind::ApiKey
            {
                return Err(TopologyError::TargetMismatch);
            }
            if self.canonical_model(&route.canonical_model).is_some()
                || self
                    .image_routes
                    .values()
                    .any(|r| r.canonical_model == route.canonical_model)
                || self
                    .speech_routes
                    .values()
                    .any(|r| r.canonical_model == route.canonical_model)
            {
                return Err(TopologyError::TaskMismatch);
            }
            if endpoint.canonical_model != route.canonical_model {
                return Err(TopologyError::CanonicalModelMismatch);
            }
            if !crate::protocol::openai::speech::valid_model(&endpoint.upstream_model) {
                return Err(TopologyError::InvalidModelBinding);
            }
            if endpoint.execution.streaming
                || endpoint.execution.retry_before_commit
                || endpoint.execution.request_body_limit == 0
                || endpoint.execution.response_body_limit == 0
                || endpoint.execution.timeout_ms == 0
            {
                return Err(TopologyError::InvalidExecutionLimits);
            }
            if self.route(&route.id).is_some()
                || self.image_routes.values().any(|r| r.id == route.id)
                || self.speech_routes.values().any(|r| r.id == route.id)
                || !ids.insert(route.id.clone())
            {
                return Err(TopologyError::DuplicateRoute);
            }
            if self.endpoint(&endpoint.id).is_some()
                || self
                    .image_routes
                    .values()
                    .any(|r| r.endpoint.id == endpoint.id)
                || self
                    .speech_routes
                    .values()
                    .any(|r| r.endpoint.id == endpoint.id)
                || !endpoints.insert(endpoint.id.clone())
            {
                return Err(TopologyError::DuplicateEndpoint);
            }
            if self.model(route.model.as_str()).is_some()
                || self.image_route(route.model.as_str()).is_some()
                || self.speech_route(route.model.as_str()).is_some()
                || self
                    .transcription_routes
                    .insert(route.model.as_str().into(), route)
                    .is_some()
            {
                return Err(TopologyError::DuplicateModel);
            }
        }
        Ok(self)
    }
}
