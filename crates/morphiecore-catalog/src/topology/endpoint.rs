//! One fixed upstream target: trusted origin + relative entry for a task,
//! protocol profile and upstream model binding.
use crate::{
    lowering::target::TargetContract,
    provider::{
        EndpointPath, ProviderId, TrustedOrigin,
        auth::{CredentialBindingId, CredentialKind},
    },
    topology::{EndpointId, GenerationProtocol},
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
    pub request_body_limit: usize,
    pub response_body_limit: usize,
    pub credential_kind: CredentialKind,
    pub timeout_ms: u64,
}

/// Compiled endpoint relation. All fields come from trusted topology input;
/// business requests cannot create or modify them.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Endpoint {
    /// Set after account/target binding; clear before retargeting an owned copy.
    pub bound_replay_scope: Option<crate::semantic::value::ReplayOrigin>,
    pub id: EndpointId,
    pub provider: ProviderId,
    pub target: EndpointTarget,
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
        if let Some(scope) = &self.bound_replay_scope {
            return scope.clone();
        }
        crate::protocol::replay_envelope::deployment_issuer(
            self.provider.as_str(),
            &self.upstream_model,
            self.target.origin.as_str(),
            self.target.path.as_str(),
            self.protocol.identity_tag(),
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
                    dialect: caps.dialect,
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
    pub fn encode_request_with_evidence(
        &self,
        request: &crate::protocol::GenerationRequestEnvelope,
    ) -> Result<
        crate::protocol::conversion::generation::Conversion<serde_json::Value>,
        crate::protocol::ConversionFailure,
    > {
        let result = (|| {
            use crate::adapter::UpstreamAdapter;
            match (self.adapter()?, &self.representation) {
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
            }
        })();
        result.map_err(|error: crate::protocol::ConversionFailure| {
            error.at(crate::protocol::conversion::generation::ConversionBoundary::RequestEncode)
        })
    }
    /// Closed Provider header projection; no inbound header map or runtime target selection.
    pub fn request_headers(
        &self,
        request: &crate::protocol::GenerationRequestEnvelope,
        body: &serde_json::Value,
    ) -> Result<Vec<(String, String)>, crate::protocol::CodecError> {
        match &self.representation {
            TargetContract::OpenAi(caps) => self.adapter()?.openai()?.request_headers(
                request,
                caps.cache,
                body["prompt_cache_key"].as_str(),
            ),
            TargetContract::AnthropicMessages(caps) => caps.request_headers(request, body),
            TargetContract::GoogleGenerateContent(caps) => Ok(caps.request_headers()),
            TargetContract::GoogleInteractions(caps) => Ok(caps.request_headers()),
        }
    }
    /// The target path the Provider's declared operation entry implies for this binding:
    /// the entry itself, or for generateContent the model inside its collection.
    pub fn declared_path(
        &self,
        entry: &EndpointPath,
    ) -> Result<EndpointPath, crate::provider::ProviderError> {
        self.protocol
            .operation()
            .resolve_path(entry, &self.upstream_model, true)
    }
    /// Relative request target. generateContent names the model in its path and
    /// selects SSE framing with a fixed query, never business input.
    pub fn request_path(&self) -> String {
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
        let native = match &self.representation {
            TargetContract::AnthropicMessages(caps) => Some(caps.profile.representation_contract()),
            TargetContract::GoogleGenerateContent(_) => {
                Some(crate::protocol::google::generate_content::semantic_contract())
            }
            TargetContract::GoogleInteractions(_) => {
                Some(crate::protocol::google::interactions::semantic_contract())
            }
            TargetContract::OpenAi(_) => None,
        };
        if native
            .is_some_and(|ceiling| !ceiling.supports_native(self.representation.dispositions()))
        {
            return Err(TopologyError::ContractUnsatisfiable);
        }
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

#[cfg(test)]
mod tests {
    use crate::protocol::model_limits::ModelLimits;
    use crate::semantic::task::generation::EffortLevel;

    #[test]
    fn declarations_are_validated_when_compiling_embedded_endpoints() {
        for limits in [
            ModelLimits {
                allowed_effort: Some(vec![].into()),
                ..Default::default()
            },
            ModelLimits {
                allowed_effort: Some(vec![EffortLevel::High; 2].into()),
                ..Default::default()
            },
            ModelLimits {
                maximum_output_tokens: Some(0),
                ..Default::default()
            },
        ] {
            let mut endpoint = crate::topology::catalog::google_bindings()[0].endpoint();
            *endpoint.representation.model_limits_mut() = limits;
            assert_eq!(
                endpoint.validate(),
                Err(crate::topology::TopologyError::ContractUnsatisfiable)
            );
        }
    }

    #[test]
    fn readable_chat_history_transfers_uncarriable_reasoning_shapes() {
        use crate::{
            protocol::openai::{Dialect, GenerationApi, GenerationCodec, TargetContract},
            semantic::task::generation::ReasoningPresence,
            topology::{EndpointId, catalog::default_topology},
        };
        use serde_json::json;
        let topology = default_topology().unwrap();
        let endpoint = topology
            .endpoint(&EndpointId::new("deepseek-chat").unwrap())
            .unwrap();
        let source_codec = GenerationCodec::new(GenerationApi::Responses, Dialect::Standard, None);
        for summary in [
            json!([]),
            json!([
                {"type":"summary_text","text":"first"},
                {"type":"summary_text","text":"second"}
            ]),
        ] {
            let source = source_codec
                .decode_client_request(
                    json!({
                        "model":"deepseek-flash","input":[
                            {"role":"user","content":"go"},
                            {"type":"reasoning","id":"rs_1","summary":summary},
                            {"type":"message","id":"msg_1","role":"assistant","status":"completed",
                                "content":[{"type":"output_text","text":"visible","annotations":[]}]},
                            {"type":"function_call","call_id":"call_1","name":"lookup","arguments":"{}"},
                            {"type":"function_call_output","call_id":"call_1","output":"feedback"},
                            {"role":"user","content":"next"}
                        ]})
                    .to_string()
                    .as_bytes(),
                )
                .unwrap();
            let before = source.clone();
            let projected = endpoint.encode_request_with_evidence(&source).unwrap();
            assert!(
                projected
                    .evidence
                    .contains_rule("chat.transfer-visible-history.v1")
            );
            assert_eq!(projected.value["messages"][1]["content"], "visible");
            assert_eq!(
                projected.value["messages"][1]["tool_calls"][0]["id"],
                "call_1"
            );
            assert_eq!(projected.value["messages"][2]["tool_call_id"], "call_1");
            assert_eq!(projected.value["messages"][2]["content"], "feedback");
            assert!(
                projected.value["messages"][1]
                    .get("reasoning_content")
                    .is_none()
            );
            assert_eq!(source, before);
            let mut explicit = source;
            let mut settings = explicit.task.semantic.settings().clone();
            settings.reasoning.presence = ReasoningPresence::Present;
            settings.reasoning.read_history = Some(true);
            explicit.task.semantic = explicit.task.semantic.with_settings(settings).unwrap();
            assert!(endpoint.encode_request(&explicit).is_err());
        }
        let source = source_codec
            .decode_client_request(
                br#"{
            "model":"deepseek-flash","input":[
                {"type":"reasoning","id":"rs_keep","summary":[
                    {"type":"summary_text","text":"retain this"}]},
                {"type":"reasoning","id":"rs_drop","summary":[],"encrypted_content":"opaque"},
                {"type":"message","id":"msg_1","role":"assistant","status":"completed",
                    "content":[{"type":"output_text","text":"visible","annotations":[]}]}
            ]}"#,
            )
            .unwrap();
        let before = source.clone();
        let projected = endpoint.encode_request_with_evidence(&source).unwrap();
        assert_eq!(
            projected.value["messages"][0]["reasoning_content"],
            "retain this"
        );
        assert_eq!(projected.value["messages"][0]["content"], "visible");
        assert!(
            projected
                .evidence
                .contains_rule("chat.transfer-visible-history.v1")
        );
        assert_eq!(source, before);

        // This target carries history-read intent, so the omission itself must
        // reject rather than relying on an unsupported-control admission error.
        let chat = GenerationCodec::new(GenerationApi::Chat, Dialect::MorphieCore, None);
        for (summary, accepted) in [
            (json!([]), false),
            (json!([{"type":"summary_text","text":"retained"}]), true),
        ] {
            let mut source = source_codec
                .decode_client_request(
                    json!({
                    "model":"m","input":[
                        {"type":"reasoning","id":"rs_1","summary":summary},
                        {"role":"assistant","content":"visible"}
                    ]})
                    .to_string()
                    .as_bytes(),
                )
                .unwrap();
            let mut settings = source.task.semantic.settings().clone();
            settings.reasoning.presence = ReasoningPresence::Present;
            settings.reasoning.read_history = Some(true);
            source.task.semantic = source.task.semantic.with_settings(settings).unwrap();
            let before = source.clone();
            let result = chat.encode_request_with_evidence(&source, "m", &TargetContract::full());
            assert_eq!(result.is_ok(), accepted);
            assert_eq!(source, before);
        }
    }
}
