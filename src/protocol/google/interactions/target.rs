//! Target capabilities and binding admission for stateless Interactions.
use super::{RequestRule, encode_generation_request};
use crate::{
    adapter::{AdapterError, Request},
    protocol::{
        CodecError,
        cache::CacheProjection,
        native::{ReplayTarget, normalize_stateless_defaults},
    },
    semantic::{
        context::ExecutionHints, task::generation::GenerationSemanticContract, value::Presence,
    },
};

pub fn semantic_contract() -> GenerationSemanticContract {
    GenerationSemanticContract {
        compaction: false,
        instructions: true,
        // A leading instruction prefix, never instructions after conversation content.
        ordered_instructions: false,
        temperature: false,
        max_output_tokens: true,
        tools: true,
        custom_tools: false,
        structured_arguments: true,
        structured_tool_results: false,
        tool_execution_reports: false,
        tool_result_errors: true,
        tool_result_media: true,
        text_metadata: false,
        top_p: false,
        top_k: false,
        stop_sequences: false,
        presence_penalty: false,
        frequency_penalty: false,
        seed: false,
        logprobs: false,
        verbosity: false,
        truncation: false,
        structured_output: false,
        reasoning: true,
        response_effort: false,
        image_input: true,
        audio_input: true,
        video_input: false,
        audio_output: false,
        audio_history: false,
        file_input: true,
        document_context: false,
        parallel_tool_calls: true,
        strict_tools: false,
        tool_examples: false,
        tool_changes: false,
        normalization_default_tools: false,
        explicit_cache: false,
        cached_context: false,
        safety: false,
        automatic_prefix_cache: false,
    }
}

/// Pure codec support does not activate a model or widen a public promise.
pub fn representation_contract() -> GenerationSemanticContract {
    let mut contract = semantic_contract();
    contract.structured_tool_results = true;
    contract.structured_output = true;
    contract.stop_sequences = true;
    contract.seed = true;
    contract
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TargetCaps {
    pub semantics: GenerationSemanticContract,
    pub sampling: super::super::SamplingPolicy,
    pub model_limits: crate::protocol::model_limits::ModelLimits,
    pub cache: CacheProjection,
    pub reasoning_policy: Option<crate::protocol::reasoning::ReasoningRequestPolicy>,
    pub reasoning_as_text: bool,
    /// Selected downstream terminal projection; native source facts stay unchanged.
    pub messages_terminal_projection: bool,
}
impl Default for TargetCaps {
    fn default() -> Self {
        Self {
            semantics: semantic_contract(),
            sampling: super::super::SamplingPolicy::Reject,
            model_limits: Default::default(),
            cache: CacheProjection::default(),
            reasoning_policy: None,
            reasoning_as_text: false,
            messages_terminal_projection: false,
        }
    }
}
impl TargetCaps {
    fn check_context(&self, request: &Request) -> Result<bool, CodecError> {
        request.check_grouping()?;
        request.context.validate()?;
        request.delivery.validate()?;
        let mut context = request.context.clone();
        self.cache.project(&mut context)?;
        normalize_stateless_defaults(&mut context);
        // Recorded as `HintRule::OmitIdentityHints`; no native identity carrier is used.
        context.identity = Default::default();
        if context != ExecutionHints::default()
            || !request.service_policy.is_absent()
            || request.delivery.stream == Presence::Null
            || request.n.value().is_some_and(|n| *n != 1)
        {
            return Err(CodecError::Unsupported(
                "Interactions request context".into(),
            ));
        }
        Ok(request.cache_session.is_some())
    }
    /// The target labels come from the trusted endpoint binding, never the request.
    pub(crate) fn encode_request(
        &self,
        request: &Request,
        target: &ReplayTarget,
    ) -> Result<crate::protocol::RequestProjection, AdapterError> {
        use crate::lowering::generation::RequestControlLoss as Loss;
        let mut projected = std::borrow::Cow::Borrowed(request);
        let mut reasoning_rule = None;
        let mut controls = vec![];
        let sampling = super::super::sampling::project(
            &request.task.semantic,
            self.sampling,
            RequestRule::OmitSampling.name(),
        )?;
        let omitted_sampling = sampling.is_some();
        if let Some((semantic, loss)) = sampling {
            projected.to_mut().task.semantic = semantic;
            controls.push(loss);
        }
        if let Some(policy) = &self.reasoning_policy
            && policy.applies_to(&projected.task.semantic)
            && let Some(semantic) =
                policy.project_request(&projected.task.semantic, |settings| {
                    use crate::semantic::task::generation::*;
                    settings.reasoning.budget_tokens.is_none()
                        && settings.effort.value().is_some_and(|effort| {
                            effort.scope == EffortScope::Reasoning
                                && matches!(
                                    effort.level,
                                    EffortLevel::Minimal
                                        | EffortLevel::Low
                                        | EffortLevel::Medium
                                        | EffortLevel::High
                                )
                                && self.model_limits.check_values(None, [effort.level]).is_ok()
                        })
                })?
        {
            controls.push(policy.receipt(&request.task.semantic, &semantic)?);
            projected.to_mut().task.semantic = semantic;
            reasoning_rule = Some(policy.rule);
        }
        let request = projected.as_ref();
        let stripped = request.without_foreign_reasoning(Some(&target.origin()))?;
        let foreign = stripped.is_some();
        let request = stripped.as_ref().unwrap_or(request);
        let cache_key = self.check_context(request)?;
        let mut admission = request.clone();
        let mut settings = admission.task.semantic.settings().clone();
        settings.text.verbosity = Presence::Absent;
        if settings.text.format.is_absent() {
            settings.text.presence = Presence::Absent;
        }
        admission.task.semantic = admission
            .task
            .semantic
            .with_settings(settings)
            .map_err(CodecError::from)?;
        admission.check_semantic(&self.semantics)?;
        admission.check_semantic(&representation_contract())?;
        let semantic = &request.task.semantic;
        let mut settings = semantic.settings().clone();
        // Include selects the downstream opaque carrier, not a native control.
        settings.reasoning = settings.reasoning.with_encrypted_output(false);
        let semantic = semantic
            .clone()
            .with_settings(settings)
            .map_err(CodecError::from)?;
        self.model_limits.check_request(&semantic)?;
        let attribution = crate::protocol::attribution::project(
            request,
            crate::protocol::attribution::Carrier::None,
        )?;
        let (wire, mut rules) =
            encode_generation_request(&semantic, &request.task.fidelity, target)?;
        if !self.reasoning_as_text && rules.contains(&RequestRule::ReasoningAsText) {
            return Err(CodecError::Unsupported("reasoning as text disabled".into()).into());
        }
        if foreign {
            rules.push(RequestRule::TransferVisibleHistory);
        }
        if omitted_sampling {
            rules.push(RequestRule::OmitSampling);
        }
        if cache_key {
            rules.push(RequestRule::OmitCacheKey);
        }
        rules.extend(
            self.cache
                .losses(&request.context)
                .into_iter()
                .chain(attribution.losses)
                .map(RequestRule::Hint),
        );
        rules.sort();
        rules.dedup();
        for rule in &rules {
            use crate::semantic::task::generation::{InstructionAuthority, ReasoningDisplay};
            match rule {
                RequestRule::Hint(hint) => controls.extend(hint.evidence().map(Loss::Projection)),
                RequestRule::OmitMessagePhase => controls.push(Loss::Projection(
                    crate::protocol::phase::omission(rule.name()),
                )),
                RequestRule::OmitVerbosity => controls.push(Loss::Verbosity {
                    rule: rule.name(),
                    input: request.task.semantic.text_options().verbosity.clone(),
                    output: Presence::Absent,
                }),
                RequestRule::SummarizeReasoning => controls.push(Loss::Summary {
                    rule: rule.name(),
                    input: request
                        .task
                        .semantic
                        .reasoning()
                        .presentation
                        .summary
                        .clone(),
                    output: ReasoningDisplay::Summary,
                }),
                RequestRule::DeveloperAsSystem => controls.push(Loss::Role {
                    rule: rule.name(),
                    input: InstructionAuthority::Developer,
                    output: InstructionAuthority::System,
                }),
                _ => {}
            }
        }
        let mut rules: Vec<_> = rules.iter().map(|r| r.name()).collect();
        rules.extend(reasoning_rule);
        Ok(crate::protocol::RequestProjection {
            value: wire,
            rules,
            controls,
        })
    }
    pub(crate) fn request_headers(&self) -> Vec<(String, String)> {
        vec![(
            "user-agent".into(),
            concat!("MorphieCore/", env!("CARGO_PKG_VERSION")).into(),
        )]
    }
}
