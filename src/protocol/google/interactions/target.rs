//! Target capabilities and binding admission for stateless Interactions.
use super::encode_generation_request;
use crate::lowering::disposition::Dispositions;
use crate::{
    protocol::{
        CodecError, ConversionFailure, GenerationRequestEnvelope as Request,
        cache::CacheTargetPolicy,
        failure::CarrierReason,
        native::{ReplayTarget, normalize_stateless_defaults},
    },
    semantic::{context::ExecutionHints, value::Presence},
};

pub fn semantic_contract() -> Dispositions {
    use crate::semantic::task::generation::Feature::*;
    // Ordered instructions need model-level binding evidence.
    let mut contract = Dispositions::empty();
    for feature in [
        Instructions,
        MaxOutputTokens,
        Tools,
        StructuredArguments,
        ToolResultError,
        ToolResultMedia,
        Reasoning,
        ImageInput,
        AudioInput,
        FileInput,
        ParallelTools,
    ] {
        contract.set_native(feature, true);
    }
    contract
}

/// Pure codec support does not activate a model or widen a public promise.
pub fn representation_contract() -> Dispositions {
    let mut contract = semantic_contract();
    contract.set_native(
        crate::semantic::task::generation::Feature::StructuredToolResult,
        true,
    );
    contract.set_native(
        crate::semantic::task::generation::Feature::StructuredOutput,
        true,
    );
    contract.set_native(
        crate::semantic::task::generation::Feature::StopSequences,
        true,
    );
    contract.set_native(crate::semantic::task::generation::Feature::Seed, true);
    contract
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TargetContract {
    pub dispositions: Dispositions,
    pub reasoning_history: Option<crate::protocol::reasoning::ReasoningHistory>,
    pub sampling: super::super::SamplingPolicy,
    pub model_limits: crate::protocol::model_limits::ModelLimits,
    pub cache: CacheTargetPolicy,
    pub reasoning_policy: Option<crate::protocol::reasoning::ReasoningRequestPolicy>,
    pub reasoning_as_text: bool,
    /// Selected downstream terminal projection; native source facts stay unchanged.
    pub messages_terminal_projection: bool,
}
impl Default for TargetContract {
    fn default() -> Self {
        Self {
            dispositions: semantic_contract(),
            reasoning_history: None,
            sampling: super::super::SamplingPolicy::Reject,
            model_limits: Default::default(),
            cache: CacheTargetPolicy::default(),
            reasoning_policy: None,
            reasoning_as_text: false,
            messages_terminal_projection: false,
        }
    }
}
impl TargetContract {
    fn check_context(&self, request: &Request) -> Result<(), CodecError> {
        request.check_grouping()?;
        request.context.validate()?;
        request.delivery.validate()?;
        let mut context = request.context.clone();
        normalize_stateless_defaults(&mut context);
        context.identity = Default::default();
        if context != ExecutionHints::default()
            || !request.service_policy.is_none()
            || request.delivery.stream == Presence::Null
            || request.n.value().is_some_and(|n| *n != 1)
        {
            return Err(CodecError::Unsupported(CarrierReason::Unavailable));
        }
        Ok(())
    }
    /// The target labels come from the trusted endpoint binding, never the request.
    pub(crate) fn encode_request(
        &self,
        request: &Request,
        target: &ReplayTarget,
    ) -> Result<
        crate::protocol::conversion::generation::Conversion<serde_json::Value>,
        ConversionFailure,
    > {
        let result = (|| {
            let request_projection = crate::lowering::request::project(
                request,
                crate::lowering::request::Target::Interactions(self),
                target.model(),
                Some(target),
            )?;
            let request = request_projection.value.as_ref();
            let mut evidence = request_projection.evidence;
            self.check_context(request)?;
            let semantic = &request.task.semantic;
            let (wire, rules) =
                encode_generation_request(semantic, &request.task.fidelity, target)?;

            for rule in rules {
                if !evidence.contains_rule(rule.name()) {
                    evidence.push(rule.effect(&request.task.semantic));
                }
            }
            Ok(crate::protocol::conversion::generation::Conversion {
                value: wire,
                evidence,
            })
        })();
        result.map_err(|error: crate::protocol::ConversionFailure| {
            error.at(crate::protocol::conversion::generation::ConversionBoundary::RequestEncode)
        })
    }
    pub(crate) fn request_headers(&self) -> Vec<(String, String)> {
        vec![(
            "user-agent".into(),
            concat!("MorphieCore/", env!("CARGO_PKG_VERSION")).into(),
        )]
    }
}
