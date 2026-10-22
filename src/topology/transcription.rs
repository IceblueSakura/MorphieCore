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
    /// All bindings of a public label must share this canonical identity.
    pub fn transcription_model(&self, model: &str) -> Option<&ModelId> {
        self.transcription_routes
            .values()
            .find(|route| route.model.as_str() == model)
            .map(|route| &route.canonical_model)
    }
    /// Implicit selection is valid only for a single-binding model.
    pub fn transcription_route(&self, model: &str) -> Option<&TranscriptionRoute> {
        let mut routes = self
            .transcription_routes
            .values()
            .filter(|r| r.model.as_str() == model);
        let route = routes.next()?;
        routes.next().is_none().then_some(route)
    }
    /// Select one trusted binding without introducing runtime fallback.
    pub fn transcription_binding(
        &self,
        model: &str,
        binding: &EndpointId,
    ) -> Option<&TranscriptionRoute> {
        self.transcription_routes
            .get(binding.as_str())
            .filter(|route| route.model.as_str() == model)
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
                || provider
                    .entry(crate::provider::ProviderOperation::Transcription)
                    .is_some_and(|entry| {
                        entry.path != &endpoint.target.path
                            || entry.auth.kind() != CredentialKind::ApiKey
                    })
                || provider.auth.kind() != CredentialKind::ApiKey
                || endpoint.execution.credential_kind != CredentialKind::ApiKey
            {
                return Err(TopologyError::TargetMismatch);
            }
            if self.canonical_model(&route.canonical_model).is_some()
                || self
                    .embedding_routes
                    .values()
                    .any(|r| r.canonical_model == route.canonical_model)
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
                || self.embedding_routes.values().any(|r| r.id == route.id)
                || self.image_routes.values().any(|r| r.id == route.id)
                || self.speech_routes.values().any(|r| r.id == route.id)
                || !ids.insert(route.id.clone())
            {
                return Err(TopologyError::DuplicateRoute);
            }
            if self.endpoint(&endpoint.id).is_some()
                || self
                    .embedding_routes
                    .values()
                    .any(|r| r.endpoint.id == endpoint.id)
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
                || self.embedding_route(route.model.as_str()).is_some()
                || self.image_route(route.model.as_str()).is_some()
                || self.speech_route(route.model.as_str()).is_some()
                || self.transcription_routes.values().any(|existing| {
                    existing.model == route.model
                        && existing.canonical_model != route.canonical_model
                })
            {
                return Err(TopologyError::DuplicateModel);
            }
            self.transcription_routes
                .insert(endpoint.id.as_str().into(), route);
        }
        Ok(self)
    }
}
