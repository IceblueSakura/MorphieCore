//! Fixed single-target embedding routes, independent of Generation capability placeholders.
use super::{
    CompiledTopology, EndpointId, EndpointTarget, ExecutionContract, ModelId, RouteId,
    TopologyError,
};
use crate::{
    adapter::embeddings::{Contract, Profile},
    provider::{CredentialBindingId, CredentialKind, EndpointPath, ProviderId},
};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderEntry {
    pub provider: ProviderId,
    pub path: EndpointPath,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EmbeddingEndpoint {
    pub id: EndpointId,
    pub provider: ProviderId,
    pub target: EndpointTarget,
    pub upstream_model: String,
    pub canonical_model: ModelId,
    pub profile: Profile,
    pub contract: Contract,
    pub credential: CredentialBindingId,
    pub execution: ExecutionContract,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EmbeddingRoute {
    pub id: RouteId,
    pub model: ModelId,
    pub canonical_model: ModelId,
    pub endpoint: EmbeddingEndpoint,
}
impl CompiledTopology {
    pub fn embedding_route(&self, model: &str) -> Option<&EmbeddingRoute> {
        self.embedding_routes.get(model)
    }
    pub fn with_embeddings(
        mut self,
        entries: Vec<ProviderEntry>,
        routes: Vec<EmbeddingRoute>,
    ) -> Result<Self, TopologyError> {
        if !self.embedding_routes.is_empty() || entries.len() > 64 || routes.len() > 64 {
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
                || self
                    .transcription_routes
                    .values()
                    .any(|r| r.canonical_model == route.canonical_model)
            {
                return Err(TopologyError::TaskMismatch);
            }
            if endpoint.canonical_model != route.canonical_model {
                return Err(TopologyError::CanonicalModelMismatch);
            }
            if endpoint.upstream_model.is_empty()
                || endpoint.upstream_model.len() > 256
                || endpoint
                    .upstream_model
                    .chars()
                    .any(|c| c.is_control() || c.is_whitespace())
            {
                return Err(TopologyError::InvalidModelBinding);
            }
            endpoint
                .contract
                .validate()
                .map_err(|_| TopologyError::ContractUnsatisfiable)?;
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
                || self.transcription_routes.values().any(|r| r.id == route.id)
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
                || self
                    .transcription_routes
                    .values()
                    .any(|r| r.endpoint.id == endpoint.id)
                || !endpoints.insert(endpoint.id.clone())
            {
                return Err(TopologyError::DuplicateEndpoint);
            }
            if self.model(route.model.as_str()).is_some()
                || self.image_route(route.model.as_str()).is_some()
                || self.speech_route(route.model.as_str()).is_some()
                || self.transcription_route(route.model.as_str()).is_some()
                || self
                    .embedding_routes
                    .insert(route.model.as_str().into(), route)
                    .is_some()
            {
                return Err(TopologyError::DuplicateModel);
            }
        }
        Ok(self)
    }
}
