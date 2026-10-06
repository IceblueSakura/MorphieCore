//! One fixed upstream target: trusted origin + relative entry for a task,
//! protocol profile and upstream model binding.

use crate::{
    lowering::generation::GenerationRepresentationContract,
    provider::{
        EndpointPath, ProviderId, TrustedOrigin, auth::CredentialBindingId, auth::CredentialKind,
    },
    topology::{EndpointId, ProtocolProfile, TaskKind},
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EndpointTarget {
    pub origin: TrustedOrigin,
    pub path: EndpointPath,
}

/// Operational limits of one endpoint. Execution capability never substitutes
/// for semantic or representation support.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExecutionContract {
    pub streaming: bool,
    /// Retries are forbidden once any semantic output is visible; this flag
    /// additionally declares whether pre-commit retry is eligible at all.
    pub retry_before_commit: bool,
    pub request_body_limit: usize,
    pub response_body_limit: usize,
    pub credential_kind: CredentialKind,
    pub timeout_ms: u64,
}

/// Compiled endpoint relation. All fields come from trusted topology input;
/// business requests cannot create or modify them.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Endpoint {
    pub id: EndpointId,
    pub provider: ProviderId,
    pub target: EndpointTarget,
    pub task: TaskKind,
    pub protocol: ProtocolProfile,
    /// Exact Provider spelling, bound at compile time and injected at attempt time.
    /// Different Providers may spell the same canonical model differently.
    pub upstream_model: String,
    /// Shared semantic identity; neither a public label nor a Provider selector.
    pub canonical_model: super::ModelId,
    pub representation: GenerationRepresentationContract,
    pub execution: ExecutionContract,
    pub credential: CredentialBindingId,
}

impl Endpoint {
    pub fn adapter(&self) -> crate::adapter::Adapter {
        crate::adapter::Adapter {
            protocol: match self.protocol {
                ProtocolProfile::OpenAiChat => crate::protocol::openai::Profile::Chat,
                ProtocolProfile::OpenAiResponses => crate::protocol::openai::Profile::Responses,
            },
            adaptation: self.representation.adaptation.clone(),
        }
    }
    pub fn validate(&self) -> Result<(), crate::topology::TopologyError> {
        use crate::topology::TopologyError;
        if self.upstream_model.is_empty()
            || self.upstream_model.len() > 256
            || self
                .upstream_model
                .chars()
                .any(|c| c.is_control() || c == ' ')
        {
            return Err(TopologyError::InvalidModelBinding);
        }
        if self.execution.request_body_limit == 0
            || self.execution.response_body_limit == 0
            || self.execution.timeout_ms == 0
        {
            return Err(TopologyError::InvalidExecutionLimits);
        }
        Ok(())
    }
}
