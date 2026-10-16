//! One fixed upstream target: trusted origin + relative entry for a task,
//! protocol profile and upstream model binding.

use crate::{
    lowering::target::TargetCaps,
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
    pub representation: TargetCaps,
    pub execution: ExecutionContract,
    pub credential: CredentialBindingId,
}

impl Endpoint {
    pub fn replay_origin(&self) -> crate::semantic::value::ReplayOrigin {
        crate::protocol::replay_envelope::deployment_issuer(
            self.provider.as_str(),
            &self.upstream_model,
            self.target.origin.as_str(),
            self.target.path.as_str(),
        )
    }

    pub fn adapter(&self) -> Result<crate::adapter::UpstreamAdapter, crate::protocol::CodecError> {
        use crate::{
            adapter::UpstreamAdapter,
            protocol::{
                CodecError,
                openai::{Adapter, Profile},
            },
        };
        let reasoning_policy = match &self.representation {
            TargetCaps::OpenAi(caps) => caps.adaptation.reasoning_policy.as_ref(),
            TargetCaps::AnthropicMessages(caps) => caps.reasoning_policy.as_ref(),
            TargetCaps::GoogleGenerateContent(caps) => caps.reasoning_policy.as_ref(),
            TargetCaps::GoogleInteractions(caps) => caps.reasoning_policy.as_ref(),
        };
        if reasoning_policy
            .is_some_and(|policy| policy.canonical_model != self.canonical_model.as_str())
        {
            return Err(CodecError::Invalid("reasoning policy model"));
        }
        Ok(match (&self.protocol, &self.representation) {
            (ProtocolProfile::AnthropicMessages, TargetCaps::AnthropicMessages(caps)) => {
                UpstreamAdapter::AnthropicMessages {
                    profile: caps.profile,
                    target: crate::protocol::native::ReplayTarget::new(
                        self.provider.as_str(),
                        &self.upstream_model,
                    )?
                    .with_origin(self.replay_origin()),
                }
            }
            (ProtocolProfile::GoogleGenerateContent, TargetCaps::GoogleGenerateContent(_)) => {
                UpstreamAdapter::GoogleGenerateContent {
                    target: crate::protocol::native::ReplayTarget::new(
                        self.provider.as_str(),
                        &self.upstream_model,
                    )?
                    .with_origin(self.replay_origin()),
                }
            }
            (ProtocolProfile::GoogleInteractions, TargetCaps::GoogleInteractions(_)) => {
                UpstreamAdapter::GoogleInteractions {
                    target: crate::protocol::native::ReplayTarget::new(
                        self.provider.as_str(),
                        &self.upstream_model,
                    )?
                    .with_origin(self.replay_origin()),
                }
            }
            (
                ProtocolProfile::OpenAiChat | ProtocolProfile::OpenAiResponses,
                TargetCaps::OpenAi(caps),
            ) => {
                if let Some(mapping) = caps.adaptation.effort_mapping {
                    mapping.validate()?;
                    if mapping.canonical_model != self.canonical_model.as_str() {
                        return Err(CodecError::Invalid("reasoning mapping model"));
                    }
                }
                UpstreamAdapter::OpenAi(Box::new(Adapter {
                    protocol: if self.protocol == ProtocolProfile::OpenAiChat {
                        Profile::Chat
                    } else {
                        Profile::Responses
                    },
                    adaptation: crate::protocol::openai::dialect::adaptation::Adaptation {
                        scope: Some(self.replay_origin()),
                        ..caps.adaptation.clone()
                    },
                }))
            }
            _ => return Err(CodecError::ProfileMismatch),
        })
    }
    /// Candidate-private upstream body for this binding's protocol and model.
    pub fn encode_request(
        &self,
        request: &crate::adapter::Request,
    ) -> Result<serde_json::Value, crate::adapter::AdapterError> {
        self.encode_request_with_rules(request)
            .map(|(value, _)| value)
    }
    /// The body plus the names of the degradable rules applied to it.
    pub fn encode_request_with_rules(
        &self,
        request: &crate::adapter::Request,
    ) -> Result<(serde_json::Value, Vec<&'static str>), crate::adapter::AdapterError> {
        self.encode_request_with_evidence(request)
            .map(|projection| (projection.value, projection.rules))
    }
    pub(crate) fn encode_request_with_evidence(
        &self,
        request: &crate::adapter::Request,
    ) -> Result<crate::protocol::RequestProjection, crate::adapter::AdapterError> {
        use crate::adapter::UpstreamAdapter;
        match (self.adapter()?, &self.representation) {
            (UpstreamAdapter::OpenAi(adapter), TargetCaps::OpenAi(caps)) => {
                let mut contract = caps.as_ref().clone();
                contract.replay_origin = contract.replay_origin.map(|_| self.replay_origin());
                adapter.encode_request_with_evidence(request, &self.upstream_model, &contract)
            }
            (
                UpstreamAdapter::AnthropicMessages { target, .. },
                TargetCaps::AnthropicMessages(caps),
            ) => caps.encode_request(request, &target).map(|projection| {
                crate::protocol::RequestProjection {
                    value: projection.value,
                    rules: projection.rules.iter().map(|rule| rule.name()).collect(),
                    controls: projection.control_losses,
                }
            }),
            (
                UpstreamAdapter::GoogleGenerateContent { target },
                TargetCaps::GoogleGenerateContent(caps),
            ) => caps.encode_request(request, &target),
            (
                UpstreamAdapter::GoogleInteractions { target },
                TargetCaps::GoogleInteractions(caps),
            ) => caps.encode_request(request, &target),
            _ => Err(crate::protocol::CodecError::ProfileMismatch.into()),
        }
    }
    /// Closed Provider header projection; no inbound header map or runtime target selection.
    pub(crate) fn request_headers(
        &self,
        request: &crate::adapter::Request,
    ) -> Result<Vec<(String, String)>, crate::protocol::CodecError> {
        match &self.representation {
            TargetCaps::OpenAi(caps) => self
                .adapter()?
                .openai()?
                .request_headers(request, caps.cache),
            TargetCaps::AnthropicMessages(caps) => caps.request_headers(request),
            TargetCaps::GoogleGenerateContent(caps) => Ok(caps.request_headers()),
            TargetCaps::GoogleInteractions(caps) => Ok(caps.request_headers()),
        }
    }
    /// The target path the Provider's declared operation entry implies for this binding:
    /// the entry itself, or for generateContent the model inside its collection.
    pub(crate) fn declared_path(&self, entry: &EndpointPath) -> String {
        match self.protocol {
            ProtocolProfile::GoogleGenerateContent => format!(
                "{}/{}:streamGenerateContent",
                entry.as_str(),
                self.upstream_model
            ),
            _ => entry.as_str().into(),
        }
    }
    /// Relative request target. generateContent names the model in its path and
    /// selects SSE framing with a fixed query, never business input.
    pub(crate) fn request_path(&self) -> String {
        match self.protocol {
            ProtocolProfile::GoogleGenerateContent => {
                format!("{}?alt=sse", self.target.path.as_str())
            }
            _ => self.target.path.as_str().into(),
        }
    }
    pub fn validate(&self) -> Result<(), crate::topology::TopologyError> {
        use crate::topology::TopologyError;
        self.adapter()
            .map_err(|_| TopologyError::ContractUnsatisfiable)?;
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
