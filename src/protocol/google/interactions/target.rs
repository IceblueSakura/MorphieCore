//! Target capabilities and binding admission for stateless Interactions.
use super::{RequestRule, encode_generation_request};
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
    (Dispositions::empty())
        .with_native(
            crate::semantic::task::generation::Feature::Compaction,
            false,
        )
        .with_native(
            crate::semantic::task::generation::Feature::Instructions,
            true,
        )
        // A leading instruction prefix, never instructions after conversation content.
        .with_native(
            crate::semantic::task::generation::Feature::OrderedInstructions,
            false,
        )
        .with_native(
            crate::semantic::task::generation::Feature::Temperature,
            false,
        )
        .with_native(
            crate::semantic::task::generation::Feature::MaxOutputTokens,
            true,
        )
        .with_native(crate::semantic::task::generation::Feature::Tools, true)
        .with_native(
            crate::semantic::task::generation::Feature::CustomTools,
            false,
        )
        .with_native(
            crate::semantic::task::generation::Feature::StructuredArguments,
            true,
        )
        .with_native(
            crate::semantic::task::generation::Feature::StructuredToolResult,
            false,
        )
        .with_native(
            crate::semantic::task::generation::Feature::ToolExecutionReport,
            false,
        )
        .with_native(
            crate::semantic::task::generation::Feature::ToolResultError,
            true,
        )
        .with_native(
            crate::semantic::task::generation::Feature::ToolResultMedia,
            true,
        )
        .with_native(
            crate::semantic::task::generation::Feature::TextMetadata,
            false,
        )
        .with_native(crate::semantic::task::generation::Feature::TopP, false)
        .with_native(crate::semantic::task::generation::Feature::TopK, false)
        .with_native(
            crate::semantic::task::generation::Feature::StopSequences,
            false,
        )
        .with_native(
            crate::semantic::task::generation::Feature::PresencePenalty,
            false,
        )
        .with_native(
            crate::semantic::task::generation::Feature::FrequencyPenalty,
            false,
        )
        .with_native(crate::semantic::task::generation::Feature::Seed, false)
        .with_native(crate::semantic::task::generation::Feature::Logprobs, false)
        .with_native(crate::semantic::task::generation::Feature::Verbosity, false)
        .with_native(
            crate::semantic::task::generation::Feature::Truncation,
            false,
        )
        .with_native(
            crate::semantic::task::generation::Feature::StructuredOutput,
            false,
        )
        .with_native(crate::semantic::task::generation::Feature::Reasoning, true)
        .with_native(
            crate::semantic::task::generation::Feature::ResponseEffort,
            false,
        )
        .with_native(crate::semantic::task::generation::Feature::ImageInput, true)
        .with_native(crate::semantic::task::generation::Feature::AudioInput, true)
        .with_native(
            crate::semantic::task::generation::Feature::VideoInput,
            false,
        )
        .with_native(
            crate::semantic::task::generation::Feature::AudioOutput,
            false,
        )
        .with_native(
            crate::semantic::task::generation::Feature::AudioHistory,
            false,
        )
        .with_native(crate::semantic::task::generation::Feature::FileInput, true)
        .with_native(
            crate::semantic::task::generation::Feature::DocumentContext,
            false,
        )
        .with_native(
            crate::semantic::task::generation::Feature::ParallelTools,
            true,
        )
        .with_native(
            crate::semantic::task::generation::Feature::StrictTools,
            false,
        )
        .with_native(
            crate::semantic::task::generation::Feature::ToolExamples,
            false,
        )
        .with_native(
            crate::semantic::task::generation::Feature::ToolChanges,
            false,
        )
        .with_native(
            crate::semantic::task::generation::Feature::ConditionalToolStrictness,
            false,
        )
        .with_native(
            crate::semantic::task::generation::Feature::ExplicitCache,
            false,
        )
        .with_native(
            crate::semantic::task::generation::Feature::CachedContext,
            false,
        )
        .with_native(crate::semantic::task::generation::Feature::Safety, false)
        .with_native(
            crate::semantic::task::generation::Feature::AutomaticPrefixCache,
            false,
        )
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
            use crate::protocol::conversion::generation::Effect;
            let request_projection = crate::lowering::request::project(
                request,
                crate::lowering::request::Target::Interactions(self),
                target.model(),
                Some(target),
            )?;
            let request = request_projection.value.as_ref();
            let mut controls: Vec<_> = request_projection.evidence.into_effects().collect();
            self.check_context(request)?;
            let semantic = &request.task.semantic;
            let (wire, mut rules) =
                encode_generation_request(semantic, &request.task.fidelity, target)?;

            rules.sort();
            rules.dedup();
            for rule in &rules {
                use crate::semantic::task::generation::ReasoningDisplay;
                match rule {
                    RequestRule::OmitMessagePhase => {
                        controls.push(crate::protocol::phase::omission(rule.name()))
                    }
                    RequestRule::OmitVerbosity => controls.push(Effect::Verbosity {
                        rule: crate::protocol::conversion::generation::RuleId::new(rule.name()),
                        input: request.task.semantic.text_options().verbosity,
                        output: None,
                        owner: crate::protocol::conversion::generation::Owner::Root,
                    }),
                    RequestRule::SummarizeReasoning => controls.push(Effect::Summary {
                        rule: crate::protocol::conversion::generation::RuleId::new(rule.name()),
                        input: request
                            .task
                            .semantic
                            .reasoning()
                            .presentation
                            .summary
                            .clone(),
                        output: ReasoningDisplay::Summary,
                        owner: crate::protocol::conversion::generation::Owner::Root,
                    }),
                    _ => {}
                }
            }
            let mut evidence: crate::protocol::conversion::generation::EvidenceDelta =
                controls.into_iter().collect();
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
