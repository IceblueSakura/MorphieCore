//! Explicit, native-only Decision routes. A label may also have a Generation route.
use super::{CompiledTopology, ExecutionContract, ModelId, TopologyError};
use crate::{
    protocol::decision::Native,
    provider::{CredentialBindingId, CredentialKind, ProviderId, ProviderOperation},
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DecisionRoute {
    pub model: ModelId,
    pub upstream_model: String,
    pub provider: ProviderId,
    pub native: Native,
    pub credential: CredentialBindingId,
    pub execution: ExecutionContract,
}

impl DecisionRoute {
    pub fn operation(&self) -> ProviderOperation {
        match self.native {
            Native::OpenAi => ProviderOperation::OpenaiDecisions,
            Native::SystemOne => ProviderOperation::SystemOne,
        }
    }
}

impl CompiledTopology {
    pub fn decision_route(&self, model: &str) -> Option<&DecisionRoute> {
        self.decision_routes.get(model)
    }

    /// In-memory bindings for the native text profile; no implicit catalog activation.
    pub fn with_decisions(mut self, routes: Vec<DecisionRoute>) -> Result<Self, TopologyError> {
        if !self.decision_routes.is_empty() || routes.len() > 64 {
            return Err(TopologyError::InvalidRoutePolicy);
        }
        for route in routes {
            let provider = self
                .provider(route.provider.as_str())
                .ok_or(TopologyError::UnknownProvider)?;
            if provider.auth.kind() != CredentialKind::ApiKey
                || provider
                    .entry(route.operation())
                    .is_none_or(|entry| entry.auth.kind() != CredentialKind::ApiKey)
                || route.execution.credential_kind != CredentialKind::ApiKey
            {
                return Err(TopologyError::TargetMismatch);
            }
            if route.execution.streaming
                || route.execution.timeout_ms == 0
                || route.execution.request_body_limit == 0
                || route.execution.response_body_limit == 0
            {
                return Err(TopologyError::InvalidExecutionLimits);
            }
            if route.upstream_model.is_empty()
                || route.upstream_model.len() > 256
                || route
                    .upstream_model
                    .chars()
                    .any(|c| c.is_control() || c.is_whitespace())
            {
                return Err(TopologyError::InvalidModelBinding);
            }
            if self
                .decision_routes
                .insert(route.model.as_str().into(), route)
                .is_some()
            {
                return Err(TopologyError::DuplicateRoute);
            }
        }
        Ok(self)
    }
}
