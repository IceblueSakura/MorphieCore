//! One fixed upstream target: trusted origin + relative entry for a task,
//! protocol profile and upstream model binding.
use crate::{
    lowering::target::TargetContract,
    protocol::failure::CarrierReason,
    provider::{
        EndpointPath, ProviderId, TrustedOrigin,
        auth::{CredentialBindingId, CredentialKind},
    },
    topology::{EndpointId, GenerationProtocol, TaskKind},
};

/// Messages tool and replay controls on another protocol's target. Eager argument
/// streaming only changes delivery granularity; tolerant `drop_block` only relaxes a
/// check other targets do not make. Explicit enforcement has no carrier and rejects.
fn omit_messages_controls(
    request: &crate::protocol::GenerationRequestEnvelope,
) -> Result<
    (
        std::borrow::Cow<'_, crate::protocol::GenerationRequestEnvelope>,
        Vec<&'static str>,
    ),
    crate::protocol::CodecError,
> {
    use crate::semantic::task::generation::{ReplayMismatch, ToolDefinition};
    let semantic = &request.task.semantic;
    let mut settings = semantic.settings().clone();
    let mut rules = vec![];
    match settings.reasoning.replay_mismatch.take() {
        Some(ReplayMismatch::Drop) => rules.push("messages.omit-block-binding.v1"),
        Some(ReplayMismatch::Reject) => {
            return Err(crate::protocol::CodecError::Unsupported(
                CarrierReason::Reasoning,
            ));
        }
        None => {}
    }
    let mut eager = false;
    for tool in settings.tools.iter_mut().flatten() {
        if let ToolDefinition::Function(tool) = tool {
            eager |= tool.eager_input.take().is_some();
        }
    }
    if eager {
        rules.push("generation.omit-eager-tool-input.v1");
    }
    if rules.is_empty() {
        return Ok((std::borrow::Cow::Borrowed(request), rules));
    }
    let mut copy = request.clone();
    copy.task.semantic = semantic.clone().with_settings(settings)?;
    Ok((std::borrow::Cow::Owned(copy), rules))
}

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
    pub protocol: GenerationProtocol,
    /// Exact Provider spelling, bound at compile time and injected at attempt time.
    /// Different Providers may spell the same canonical model differently.
    pub upstream_model: String,
    /// Shared semantic identity; neither a public label nor a Provider selector.
    pub canonical_model: super::ModelId,
    pub representation: TargetContract,
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
                openai::{GenerationApi, GenerationCodec},
            },
        };
        let reasoning_policy = match &self.representation {
            TargetContract::OpenAi(caps) => caps.adaptation.reasoning_policy.as_ref(),
            TargetContract::AnthropicMessages(caps) => caps.reasoning_policy.as_ref(),
            TargetContract::GoogleGenerateContent(caps) => caps.reasoning_policy.as_ref(),
            TargetContract::GoogleInteractions(caps) => caps.reasoning_policy.as_ref(),
        };
        if let Some(policy) = reasoning_policy {
            policy.validate()?;
            if policy.canonical_model != self.canonical_model.as_str() {
                return Err(CodecError::invalid());
            }
        }
        Ok(match (&self.protocol, &self.representation) {
            (GenerationProtocol::AnthropicMessages, TargetContract::AnthropicMessages(caps)) => {
                UpstreamAdapter::AnthropicMessages {
                    profile: caps.profile,
                    target: crate::protocol::native::ReplayTarget::new(
                        self.provider.as_str(),
                        &self.upstream_model,
                    )?
                    .with_origin(self.replay_origin()),
                }
            }
            (
                GenerationProtocol::GoogleGenerateContent,
                TargetContract::GoogleGenerateContent(_),
            ) => UpstreamAdapter::GoogleGenerateContent {
                target: crate::protocol::native::ReplayTarget::new(
                    self.provider.as_str(),
                    &self.upstream_model,
                )?
                .with_origin(self.replay_origin()),
            },
            (GenerationProtocol::GoogleInteractions, TargetContract::GoogleInteractions(_)) => {
                UpstreamAdapter::GoogleInteractions {
                    target: crate::protocol::native::ReplayTarget::new(
                        self.provider.as_str(),
                        &self.upstream_model,
                    )?
                    .with_origin(self.replay_origin()),
                }
            }
            (
                GenerationProtocol::OpenAiChat | GenerationProtocol::OpenAiResponses,
                TargetContract::OpenAi(caps),
            ) => {
                if let Some(mapping) = caps.adaptation.effort_mapping {
                    mapping.validate()?;
                    if mapping.canonical_model != self.canonical_model.as_str() {
                        return Err(CodecError::invalid());
                    }
                }
                UpstreamAdapter::OpenAi(Box::new(GenerationCodec {
                    protocol: if self.protocol == GenerationProtocol::OpenAiChat {
                        GenerationApi::Chat
                    } else {
                        GenerationApi::Responses
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
        request: &crate::protocol::GenerationRequestEnvelope,
    ) -> Result<serde_json::Value, crate::protocol::ConversionFailure> {
        self.encode_request_with_evidence(request)
            .map(|projection| projection.value)
    }
    /// The body plus the names of the degradable rules applied to it.
    pub fn encode_request_with_rules(
        &self,
        request: &crate::protocol::GenerationRequestEnvelope,
    ) -> Result<(serde_json::Value, Vec<&'static str>), crate::protocol::ConversionFailure> {
        self.encode_request_with_evidence(request)
            .map(|projection| {
                let rules = projection.evidence.rule_names();
                (projection.value, rules)
            })
    }
    pub(crate) fn encode_request_with_evidence(
        &self,
        request: &crate::protocol::GenerationRequestEnvelope,
    ) -> Result<
        crate::protocol::conversion::generation::Conversion<serde_json::Value>,
        crate::protocol::ConversionFailure,
    > {
        let result = (|| {
            use crate::adapter::UpstreamAdapter;
            let (request, omitted) = match &self.representation {
                TargetContract::AnthropicMessages(_) => {
                    (std::borrow::Cow::Borrowed(request), vec![])
                }
                _ => omit_messages_controls(request)?,
            };
            let request = request.as_ref();
            let mut projection = match (self.adapter()?, &self.representation) {
                (UpstreamAdapter::OpenAi(adapter), TargetContract::OpenAi(caps)) => {
                    let mut contract = caps.as_ref().clone();
                    contract.replay_origin = contract.replay_origin.map(|_| self.replay_origin());
                    adapter.encode_request_with_evidence(request, &self.upstream_model, &contract)
                }
                (
                    UpstreamAdapter::AnthropicMessages { target, .. },
                    TargetContract::AnthropicMessages(caps),
                ) => caps.encode_request(request, &target),
                (
                    UpstreamAdapter::GoogleGenerateContent { target },
                    TargetContract::GoogleGenerateContent(caps),
                ) => caps.encode_request(request, &target),
                (
                    UpstreamAdapter::GoogleInteractions { target },
                    TargetContract::GoogleInteractions(caps),
                ) => caps.encode_request(request, &target),
                _ => Err(crate::protocol::CodecError::ProfileMismatch.into()),
            }?;
            for rule in omitted {
                projection
                    .evidence
                    .push(crate::protocol::conversion::generation::Effect::new(
                        rule,
                        crate::protocol::conversion::generation::Field::ToolResultError,
                        crate::protocol::conversion::generation::ConversionBoundary::RequestProject,
                        crate::protocol::conversion::generation::Change::Approximate,
                    ));
            }
            Ok(projection)
        })();
        result.map_err(|error: crate::protocol::ConversionFailure| {
            error.at(crate::protocol::conversion::generation::ConversionBoundary::RequestEncode)
        })
    }
    /// Closed Provider header projection; no inbound header map or runtime target selection.
    pub(crate) fn request_headers(
        &self,
        request: &crate::protocol::GenerationRequestEnvelope,
        body: &serde_json::Value,
    ) -> Result<Vec<(String, String)>, crate::protocol::CodecError> {
        match &self.representation {
            TargetContract::OpenAi(caps) => self
                .adapter()?
                .openai()?
                .request_headers(request, caps.cache),
            TargetContract::AnthropicMessages(caps) => caps.request_headers(request, body),
            TargetContract::GoogleGenerateContent(caps) => Ok(caps.request_headers()),
            TargetContract::GoogleInteractions(caps) => Ok(caps.request_headers()),
        }
    }
    /// The target path the Provider's declared operation entry implies for this binding:
    /// the entry itself, or for generateContent the model inside its collection.
    pub(crate) fn declared_path(
        &self,
        entry: &EndpointPath,
    ) -> Result<EndpointPath, crate::provider::ProviderError> {
        self.protocol
            .operation()
            .resolve_path(entry, &self.upstream_model, true)
    }
    /// Relative request target. generateContent names the model in its path and
    /// selects SSE framing with a fixed query, never business input.
    pub(crate) fn request_path(&self) -> String {
        match self.protocol {
            GenerationProtocol::GoogleGenerateContent => {
                format!("{}?alt=sse", self.target.path.as_str())
            }
            _ => self.target.path.as_str().into(),
        }
    }
    pub fn validate(&self) -> Result<(), crate::topology::TopologyError> {
        use crate::topology::TopologyError;
        self.adapter()
            .map_err(|_| TopologyError::ContractUnsatisfiable)?;
        self.representation
            .model_limits()
            .validate()
            .map_err(|_| TopologyError::ContractUnsatisfiable)?;
        if let Some(caps) = self.representation.messages() {
            caps.validate()
                .map_err(|_| TopologyError::ContractUnsatisfiable)?;
        }
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
