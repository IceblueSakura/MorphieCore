//! Explicit Decision routes. A label may also have a Generation route.
use super::{CompiledTopology, ExecutionContract, ModelId, TopologyError};
use crate::{
    protocol::decision::{Native, Target},
    provider::{CredentialBindingId, CredentialKind, ProviderId, ProviderOperation},
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DecisionRoute {
    pub model: ModelId,
    pub upstream_model: String,
    pub provider: ProviderId,
    pub target: Target,
    pub client: Option<Native>,
    pub credential: CredentialBindingId,
    pub execution: ExecutionContract,
    pub system_one_metadata: Option<SystemOneMetadata>,
}

/// Explicit versioned publication facts; never copied from a serving alias.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct SystemOneMetadata {
    pub(super) description: &'static str,
    pub(super) release_date: &'static str,
}

impl DecisionRoute {
    pub fn operation(&self) -> ProviderOperation {
        match self.target {
            Target::Native(Native::OpenAi) => ProviderOperation::OpenaiDecisions,
            Target::Native(Native::SystemOne) => ProviderOperation::SystemOne,
            Target::Gemini => ProviderOperation::GoogleDecisions,
        }
    }
}

impl CompiledTopology {
    pub fn decision_route(&self, model: &str) -> Option<&DecisionRoute> {
        self.decision_routes.get(model)
    }

    /// Bind only selected client/target pairs; no implicit catalog activation.
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
            let valid_profile = match (route.client, route.target) {
                (Some(client), Target::Native(target)) => {
                    client == target
                        && (client == Native::SystemOne || route.system_one_metadata.is_none())
                }
                (None, Target::Gemini) => route.system_one_metadata.is_none(),
                (Some(Native::SystemOne), Target::Gemini) => {
                    route.provider.as_str() == "google"
                        && route.model.as_str() == "gemini-decision-1"
                        && route.upstream_model == "gemini-decision-1"
                        && route.system_one_metadata.is_some()
                }
                _ => false,
            };
            if !valid_profile {
                return Err(TopologyError::TaskMismatch);
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
