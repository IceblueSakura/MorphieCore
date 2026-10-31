//! One candidate-local request projection. Codecs render its output; they do not select losses.
use std::borrow::Cow;

use crate::{
    lowering::disposition::{Disposition, Dispositions},
    protocol::{
        CodecError, ConversionFailure, GenerationRequestEnvelope as Request,
        conversion::generation::{Conversion, ConversionBoundary, EvidenceDelta, Owner},
        failure::CarrierReason,
        openai::{GenerationApi, TargetContract as OpenAiContract},
    },
    semantic::{task::generation::*, value::Presence},
};

/// Borrow the existing target declaration rather than maintaining another capability object.
#[derive(Clone, Copy)]
pub(crate) enum Target<'a> {
    OpenAi(GenerationApi, &'a OpenAiContract),
    Messages(&'a crate::protocol::anthropic::messages::TargetContract),
    GenerateContent(&'a crate::protocol::google::generate_content::TargetContract),
    Interactions(&'a crate::protocol::google::interactions::TargetContract),
}
pub(crate) fn admit_source(source: &Request) -> Result<(), ConversionFailure> {
    source.check_grouping()?;
    source.task.semantic.validate().map_err(CodecError::from)?;
    source.context.validate().map_err(CodecError::from)?;
    source.delivery.validate().map_err(CodecError::from)?;
    // Candidate projections own target capabilities; source admission only
    // applies union-wide exclusions after validating grammar and bounds.
    Dispositions::full()
        .check(&source.task.semantic)
        .map_err(crate::protocol::conversion::generation::RepresentationError::Admission)?;
    Ok(())
}
impl<'a> Target<'a> {
    fn declared(self) -> &'a Dispositions {
        match self {
            Self::OpenAi(_, c) => &c.dispositions,
            Self::Messages(c) => &c.dispositions,
            Self::GenerateContent(c) => &c.dispositions,
            Self::Interactions(c) => &c.dispositions,
        }
    }
    fn limits(self) -> &'a crate::protocol::model_limits::ModelLimits {
        match self {
            Self::OpenAi(_, c) => &c.model_limits,
            Self::Messages(c) => &c.model_limits,
            Self::GenerateContent(c) => &c.model_limits,
            Self::Interactions(c) => &c.model_limits,
        }
    }
    fn effort_scope(self) -> EffortScope {
        if matches!(self, Self::Messages(_)) {
            EffortScope::Response
        } else {
            EffortScope::Reasoning
        }
    }
    fn effort_levels(self) -> &'static [EffortLevel] {
        use EffortLevel::*;
        match self {
            Self::Messages(_) => &[Low, Medium, High, XHigh, Max],
            Self::GenerateContent(_) | Self::Interactions(_) => &[Minimal, Low, Medium, High],
            _ => &[Minimal, Low, Medium, High, XHigh, Max],
        }
    }
    fn has_effort(self) -> bool {
        match self {
            Self::OpenAi(_, c) => {
                c.dispositions.native(Feature::Reasoning)
                    && !c.adaptation.rules.reject_reasoning_controls
            }
            Self::Messages(c) => c.dispositions.native(Feature::ResponseEffort),
            Self::GenerateContent(c) => c.dispositions.native(Feature::Reasoning),
            Self::Interactions(c) => c.dispositions.native(Feature::Reasoning),
        }
    }
    fn has_budget(self) -> bool {
        match self {
            Self::OpenAi(GenerationApi::Chat, c) => {
                c.adaptation.rules.thinking_controls
                    && !c.adaptation.effort_mapping.is_some_and(|mapping| {
                        matches!(
                            mapping.canonical_model,
                            "glm-5.3" | "glm-5.3-prime" | "kimi-k3"
                        )
                    })
            }
            Self::Messages(c) => c.dispositions.native(Feature::ReasoningBudget),
            Self::GenerateContent(c) => {
                c.dispositions.native(Feature::ReasoningBudget)
                    || c.reasoning_policy.as_ref().is_some_and(|p| {
                        matches!(
                            p.conversion,
                            crate::protocol::reasoning::ReasoningConversion::EffortToBudget { .. }
                        )
                    })
            }
            _ => false,
        }
    }
    fn policy(self) -> Option<&'a crate::protocol::reasoning::ReasoningRequestPolicy> {
        match self {
            Self::OpenAi(_, c) => c.adaptation.reasoning_policy.as_ref(),
            Self::Messages(c) => c.reasoning_policy.as_ref(),
            Self::GenerateContent(c) => c.reasoning_policy.as_ref(),
            Self::Interactions(c) => c.reasoning_policy.as_ref(),
        }
    }
    fn budget_fits(self, settings: &GenerationSettings) -> bool {
        if !self.has_budget() {
            return false;
        }
        let Some(budget) = settings.reasoning.budget_tokens else {
            return true;
        };
        let output = settings.controls.max_output_tokens.or(match self {
            Self::Messages(c) => c.default_output_tokens,
            _ => None,
        });
        let ceiling = match (output, self.limits().maximum_output_tokens) {
            (Some(output), Some(maximum)) => Some(output.min(maximum)),
            (output, maximum) => output.or(maximum),
        };
        ceiling.is_none_or(|ceiling| {
            budget < ceiling || !matches!(self, Self::Messages(_)) && budget == ceiling
        })
    }
    fn cache(self) -> crate::protocol::cache::CacheTargetPolicy {
        match self {
            Self::OpenAi(_, c) => c.cache,
            Self::Messages(c) => c.cache,
            Self::GenerateContent(c) => c.cache,
            Self::Interactions(c) => c.cache,
        }
    }
    fn levels(self) -> Vec<EffortLevel> {
        let native = if let Self::OpenAi(_, c) = self
            && let Some(mapping) = c.adaptation.effort_mapping
        {
            mapping.native_levels
        } else {
            self.effort_levels()
        };
        native
            .iter()
            .copied()
            .filter(|level| {
                self.limits()
                    .allowed_effort
                    .as_ref()
                    .is_none_or(|allowed| allowed.contains(level))
            })
            .collect()
    }
    fn map_effort(
        self,
        mut effort: EffortControl,
        levels: &[EffortLevel],
    ) -> Result<(EffortControl, &'static str), CodecError> {
        effort.scope = self.effort_scope();
        let mapping = match self {
            Self::OpenAi(_, c) => c.adaptation.effort_mapping,
            _ => None,
        };
        if let Some(mapping) = mapping {
            effort.level = mapping.encode(effort.level)?;
        }
        effort.level = crate::protocol::reasoning::nearest_effort(effort.level, levels)?;
        Ok((
            effort,
            mapping.map_or(super::disposition::EFFORT.id, |mapping| mapping.rule),
        ))
    }
    fn table(self, source: &GenerationRequest, model: &str) -> Dispositions {
        use Feature::*;
        let mut table = self.declared().clone();
        table.set(
            ReasoningContext,
            Disposition::Reject(
                "the selected carrier cannot enforce the explicit reasoning context",
            ),
        );
        table.set(
            ReasoningHistoryRead,
            Disposition::Reject(
                "the selected carrier cannot enforce the explicit history-read policy",
            ),
        );
        table.set(
            ImageDetail,
            Disposition::Approximate(super::disposition::OMIT_IMAGE_DETAIL),
        );
        let controls = source.controls();
        let omit = |table: &mut Dispositions, feature| table.set_native(feature, false);
        table.set_native(ToolChoice, self.declared().native(Tools));
        match self {
            Self::OpenAi(api, c) => {
                let rules = &c.adaptation.rules;
                if rules.thinking_controls || rules.bailian_requests {
                    table.set(ReasoningActivation, Disposition::Native);
                }
                if self.has_effort()
                    && source.reasoning().activation.value()
                        == Some(&crate::semantic::task::generation::ReasoningActivation::Disabled)
                {
                    table.set(ReasoningActivation, Disposition::Native);
                }
                if api == GenerationApi::Chat && rules.chat_preserve_thinking {
                    table.set(ReasoningHistoryRead, Disposition::Native);
                }
                for feature in [
                    Temperature,
                    TopP,
                    PresencePenalty,
                    FrequencyPenalty,
                    Seed,
                    Logprobs,
                    Verbosity,
                    Truncation,
                ] {
                    table.set_native(feature, c.dispositions.native(feature));
                }
                table.set_native(TopK, api == GenerationApi::Chat && rules.thinking_controls);
                table.set_native(
                    ToolBudget,
                    api == GenerationApi::Responses
                        && c.dispositions.native(ToolBudget)
                        && !rules.bailian_requests
                        && !rules.responses_siwc,
                );
                table.set(
                    CachedContext,
                    Disposition::Reject("cached context has no OpenAI input carrier"),
                );
                table.set(
                    Safety,
                    Disposition::Reject("explicit safety settings have no OpenAI request carrier"),
                );
                table.set(
                    ToolResultError,
                    Disposition::Approximate(super::disposition::TOOL_ERROR),
                );
                table.set(
                    ToolExecutionReport,
                    Disposition::Reject("tool execution states have no OpenAI input carrier"),
                );
                if api == GenerationApi::Responses {
                    for feature in [TopK, PresencePenalty, FrequencyPenalty, Seed] {
                        omit(&mut table, feature);
                    }
                } else {
                    omit(&mut table, Truncation);
                }
                if rules.responses_siwc {
                    for feature in [
                        MaxOutputTokens,
                        Temperature,
                        TopP,
                        Metadata,
                        ServicePolicy,
                        Identity,
                    ] {
                        omit(&mut table, feature);
                    }
                }
                if rules.bailian_requests {
                    if matches!(model, "glm-5.3" | "glm-5.3-prime")
                        && source.reasoning().activation.value()
                            == Some(
                                &crate::semantic::task::generation::ReasoningActivation::Disabled,
                            )
                    {
                        table.set(
                            ReasoningActivation,
                            Disposition::Reject("this model has no thinking-off carrier"),
                        );
                    }
                    if api == GenerationApi::Responses && controls.max_output_tokens.is_some_and(|n|
                        n < 16 || !model.is_empty() && !matches!(model, "qwen3.8-flash" | "qwen3.8-max")
                            && source.reasoning().activation.value() != Some(&crate::semantic::task::generation::ReasoningActivation::Disabled)
                    ) { omit(&mut table, MaxOutputTokens); }
                    for feature in [Metadata, ServicePolicy, Verbosity] {
                        omit(&mut table, feature);
                    }
                    if controls.temperature().is_some_and(|v| {
                        !(0.0..2.0).contains(&v)
                            || matches!(model, "qwen3.8-flash" | "qwen3.8-max")
                                && source.reasoning().activation.value()
                                    != Some(&crate::semantic::task::generation::ReasoningActivation::Disabled)
                                && v < 0.6
                    }) {
                        omit(&mut table, Temperature);
                    }
                    if controls.top_p().is_some_and(|v| v <= 0.0) {
                        omit(&mut table, TopP);
                    }
                    if controls.top_k.is_some_and(|v| v > 100) || model == "kimi-k3" {
                        omit(&mut table, TopK);
                    }
                    if controls
                        .seed
                        .is_some_and(|v| !(0..=2147483647).contains(&v))
                    {
                        omit(&mut table, Seed);
                    }
                    if controls.logprobs == Some(true) || controls.top_logprobs.is_some() {
                        omit(&mut table, Logprobs);
                    }
                    if api == GenerationApi::Chat {
                        omit(&mut table, FrequencyPenalty);
                    }
                    if api == GenerationApi::Responses {
                        omit(&mut table, Truncation);
                    }
                }
                if api == GenerationApi::Chat
                    && controls
                        .presence_penalty()
                        .is_some_and(|v| !(-2.0..=2.0).contains(&v))
                {
                    omit(&mut table, PresencePenalty);
                }
                if api == GenerationApi::Chat
                    && controls
                        .frequency_penalty()
                        .is_some_and(|v| !(-2.0..=2.0).contains(&v))
                {
                    omit(&mut table, FrequencyPenalty);
                }
                if api == GenerationApi::Chat || rules.bailian_requests {
                    table.set(
                        MessagePhase,
                        Disposition::Omit(super::disposition::OMIT_PHASE),
                    );
                    table.set(
                        ConditionalToolStrictness,
                        Disposition::Approximate(super::disposition::CONDITIONAL_STRICTNESS),
                    );
                } else {
                    table.set(MessagePhase, Disposition::Native);
                }
                table.set_native(
                    RequestLifetimeCache,
                    api == GenerationApi::Responses
                        && !rules.bailian_requests
                        && !rules.responses_siwc,
                );
                omit(&mut table, ExplicitCache);
                omit(&mut table, AutomaticPrefixCache);
                if rules.parallel_tools_always_enabled
                    || api == GenerationApi::Responses && rules.bailian_requests
                {
                    table.set(ParallelTools, Disposition::Equivalent);
                    table.set(
                        SerialTools,
                        Disposition::Reject(
                            "serial tool execution cannot be guaranteed by this target",
                        ),
                    );
                }
                table.set_native(
                    Prediction,
                    api == GenerationApi::Chat && c.dispositions.native(Prediction),
                );
            }
            Self::Messages(c) => {
                table.set_native(ReasoningMode, true);
                table.set_native(ReasoningActivation, true);
                table.set_native(ServicePolicy, true);
                table.set(
                    ToolExecutionReport,
                    Disposition::Omit(super::disposition::Rule::new(
                        "messages.omit-tool-execution.v1",
                        crate::protocol::conversion::generation::Field::ToolExecution,
                        crate::protocol::conversion::generation::Change::Omit,
                    )),
                );
                for feature in [
                    Temperature,
                    TopP,
                    TopK,
                    PresencePenalty,
                    FrequencyPenalty,
                    Seed,
                    Logprobs,
                    Verbosity,
                    Truncation,
                    Metadata,
                    Prediction,
                    RequestLifetimeCache,
                ] {
                    omit(&mut table, feature);
                }
                for feature in [ExplicitCache, AutomaticPrefixCache, ToolChoice, SerialTools] {
                    table.set_native(feature, true);
                }
                table.set(
                    MessagePhase,
                    Disposition::Omit(super::disposition::OMIT_PHASE),
                );
                table.set(
                    ConditionalToolStrictness,
                    Disposition::Approximate(super::disposition::CONDITIONAL_STRICTNESS),
                );
                table.set(
                    ToolChanges,
                    if c.dispositions.native(ToolChanges) {
                        Disposition::Native
                    } else if source.features().contains_key(&ToolExamples) {
                        Disposition::Reject(
                            "tool-history collapse cannot preserve example-dependent tool versions",
                        )
                    } else {
                        Disposition::Approximate(super::disposition::TOOL_CHANGES)
                    },
                );
                if !c.dispositions.native(MessageEffort) {
                    table.set(
                        MessageEffort,
                        Disposition::Approximate(super::disposition::MESSAGE_EFFORT),
                    );
                    if source.items().iter().any(|(_, item)| {
                        matches!(item, Item::ConfigurationUpdate(update)
                        if update.activation.is_some())
                    }) {
                        table.set(ReasoningActivation, Disposition::Reject(
                            "the selected native message carrier cannot preserve ordered activation changes"));
                    }
                }
            }
            Self::GenerateContent(c) => {
                table.set_native(ReasoningActivation, true);
                table.set_native(ReasoningMode, true);
                table.set(ParallelTools, Disposition::Equivalent);
                table.set(
                    SerialTools,
                    Disposition::Reject("Google has no exact serial-tool guarantee carrier"),
                );
                for feature in [
                    Verbosity,
                    Metadata,
                    ServicePolicy,
                    Prediction,
                    ExplicitCache,
                    RequestLifetimeCache,
                    AutomaticPrefixCache,
                    Truncation,
                ] {
                    omit(&mut table, feature);
                }
                for feature in [TopK, PresencePenalty, FrequencyPenalty, Seed] {
                    table.set_native(feature, true);
                }
                if c.sampling != crate::protocol::google::SamplingPolicy::Native {
                    for feature in [Temperature, TopP, TopK] {
                        omit(&mut table, feature);
                    }
                }
                if controls.seed.is_some_and(|v| i32::try_from(v).is_err()) {
                    omit(&mut table, Seed);
                }
                table.set(
                    MessagePhase,
                    Disposition::Omit(super::disposition::OMIT_PHASE),
                );
            }
            Self::Interactions(_) => {
                table.set_native(ReasoningActivation, true);
                table.set_native(ReasoningMode, true);
                table.set(ParallelTools, Disposition::Equivalent);
                table.set(
                    SerialTools,
                    Disposition::Reject("Google has no exact serial-tool guarantee carrier"),
                );
                for feature in [
                    Temperature,
                    TopP,
                    TopK,
                    PresencePenalty,
                    FrequencyPenalty,
                    Verbosity,
                    Metadata,
                    ServicePolicy,
                    Prediction,
                    ExplicitCache,
                    RequestLifetimeCache,
                    AutomaticPrefixCache,
                    Truncation,
                ] {
                    omit(&mut table, feature);
                }
                table.set_native(Seed, true);
                table.set(
                    MessagePhase,
                    Disposition::Omit(super::disposition::OMIT_PHASE),
                );
            }
        }
        self.presentation_rows(source.settings(), &mut table);
        table.set(
            Effort,
            if self.has_effort() {
                Disposition::Approximate(super::disposition::EFFORT)
            } else {
                super::disposition::unsupported(Effort)
            },
        );
        if matches!(table.get(ConditionalToolStrictness), Disposition::Omit(_)) {
            table.set(
                ConditionalToolStrictness,
                Disposition::Approximate(super::disposition::CONDITIONAL_STRICTNESS),
            );
        }
        if !matches!(self, Target::Messages(_)) || !table.native(EagerToolInput) {
            table.set(
                EagerToolInput,
                Disposition::Omit(super::disposition::OMIT_EAGER_INPUT),
            );
        }
        if matches!(self, Target::OpenAi(GenerationApi::Chat, contract)
            if contract.adaptation.rules.thinking_controls)
            && source.reasoning().mode.value()
                == Some(&crate::semantic::task::generation::ReasoningMode::Adaptive)
            && source.reasoning().budget_tokens.is_none()
        {
            table.set(ReasoningMode, Disposition::Equivalent);
        }
        if !matches!(self, Target::Messages(_)) || !table.native(ReplayMismatch) {
            table.set(
                ReplayMismatch,
                match source.reasoning().replay_mismatch {
                    Some(crate::semantic::task::generation::ReplayMismatch::Reject) => {
                        Disposition::Reject(
                            "the target cannot enforce the requested replay mismatch failure",
                        )
                    }
                    _ => Disposition::Omit(super::disposition::OMIT_REPLAY_MISMATCH),
                },
            );
        }
        if source.items().iter().any(|(_, item)| {
            matches!(item,
            Item::Reasoning(reasoning) if reasoning.replay.is_none() && !reasoning.parts.is_empty())
        }) {
            let native = match self {
                Target::Messages(contract) => Some((
                    contract.reasoning_as_text,
                    super::disposition::MESSAGES_REASONING_AS_TEXT,
                )),
                Target::GenerateContent(contract) => Some((
                    contract.reasoning_as_text,
                    super::disposition::GENERATE_CONTENT_REASONING_AS_TEXT,
                )),
                Target::Interactions(contract) => Some((
                    contract.reasoning_as_text,
                    super::disposition::INTERACTIONS_REASONING_AS_TEXT,
                )),
                Target::OpenAi(_, _) => None,
            };
            if let Some((allowed, rule)) = native {
                table.set(ReasoningHistory, if allowed { Disposition::Approximate(rule) }
                    else { Disposition::Reject("readable reasoning cannot be reclassified as assistant text without the selected policy") });
            }
        }
        if let Target::OpenAi(api, contract) = self {
            table.set(SchemaName, Disposition::Native);
            if api == GenerationApi::Responses && !contract.adaptation.rules.bailian_requests {
                table.set(ReasoningContext, Disposition::Native);
            }
            if !contract.adaptation.rules.thinking_controls
                && !contract.adaptation.rules.bailian_requests
                && source.reasoning().activation.value()
                    == Some(&crate::semantic::task::generation::ReasoningActivation::Enabled)
                && (!self.has_effort()
                    || source.settings().effort.is_none()
                        && source.reasoning().budget_tokens.is_none())
            {
                omit(&mut table, ReasoningActivation);
            }
            if !self.has_budget()
                && matches!(
                    source.reasoning().mode.value(),
                    Some(
                        crate::semantic::task::generation::ReasoningMode::Adaptive
                            | crate::semantic::task::generation::ReasoningMode::Budgeted
                    )
                )
            {
                omit(&mut table, ReasoningMode);
            } else if self.has_budget()
                && source.reasoning().mode.value()
                    == Some(&crate::semantic::task::generation::ReasoningMode::Budgeted)
            {
                table.set(ReasoningMode, Disposition::Equivalent);
            }
        }
        if (matches!(self, Target::OpenAi(_, contract) if contract.adaptation.rules.reject_disabled_reasoning)
            || matches!(self, Target::GenerateContent(_) | Target::Interactions(_)))
            && (source.reasoning().activation.value() == Some(&crate::semantic::task::generation::ReasoningActivation::Disabled)
                || source.items().iter().any(|(_, item)| matches!(item, Item::ConfigurationUpdate(update)
                    if update.activation == Some(crate::semantic::task::generation::ReasoningActivation::Disabled))))
        {
            table.set(ReasoningActivation, Disposition::Reject("the selected always-thinking model cannot disable reasoning"));
        }
        table
    }
    fn projected_rows(
        self,
        source: &GenerationRequest,
        model: &str,
        streaming: bool,
        table: &mut Dispositions,
    ) {
        let Target::OpenAi(api, contract) = self else {
            return;
        };
        let settings = source.settings();
        if streaming
            && settings
                .audio
                .value()
                .is_some_and(|audio| audio.format != AudioFormat::Pcm16)
        {
            table.set(
                Feature::AudioOutput,
                Disposition::Reject("streamed audio requires the PCM16 carrier"),
            );
        }
        let custom_output = match source.tool_choice() {
            Some(ToolChoice::None | ToolChoice::Specific(_)) => false,
            Some(ToolChoice::Allowed { tools, .. }) => {
                tools.iter().any(|tool| tool.kind == ToolKind::Custom)
            }
            _ => true,
        };
        if api == GenerationApi::Chat
            && (streaming || contract.adaptation.rules.responses_forced_stream)
            && custom_output
            && source
                .tools()
                .iter()
                .flat_map(ToolDefinition::leaves)
                .any(|tool| matches!(tool, ToolDefinition::Custom(_)))
        {
            table.set(
                Feature::CustomTools,
                Disposition::Reject("Chat streaming has no selected custom-call output carrier"),
            );
        }
        if contract.adaptation.rules.reject_reasoning_controls
            && (source.reasoning().presence() == ReasoningPresence::Present
                || source.reasoning().encrypted_output()
                || settings.effort.is_some())
        {
            for feature in [
                Feature::ReasoningActivation,
                Feature::ReasoningMode,
                Feature::ReasoningBudget,
                Feature::ReasoningDisplay,
                Feature::ReasoningSummary,
                Feature::EncryptedReasoning,
                Feature::Effort,
            ] {
                table.set(
                    feature,
                    Disposition::Reject(
                        "the selected target does not execute active reasoning controls",
                    ),
                );
            }
        }
        if contract.adaptation.rules.responses_siwc
            && (source.tools().iter().any(|tool| {
                !matches!(tool, ToolDefinition::Namespace(_)) || !tool.dispatch_inactive()
            }) || source.items().iter().any(|(_, item)| match item {
                Item::ToolCall(call) => {
                    call.context.namespace.is_none()
                        || call.context.async_call
                        || call.context.caller.is_some()
                }
                Item::CustomCall(call) => {
                    call.context.namespace.is_none()
                        || call.context.async_call
                        || call.context.caller.is_some()
                }
                Item::Program(_) | Item::ProgramOutput(_) => true,
                _ => false,
            }))
        {
            table.set(Feature::Tools, Disposition::Reject("the selected native tool contract requires inactive namespaces and namespace-qualified direct calls"));
        }
        if contract.adaptation.rules.bailian_requests {
            let qwen = matches!(model, "qwen3.8-flash" | "qwen3.8-max");
            let glm = matches!(model, "glm-5.3" | "glm-5.3-prime");
            let disabled =
                settings.reasoning.activation.value() == Some(&ReasoningActivation::Disabled);
            if api == GenerationApi::Chat
                && (glm && !matches!(source.output(), OutputConstraint::Text)
                    || model == "kimi-k3"
                        && matches!(source.output(), OutputConstraint::JsonSchema { .. }))
            {
                table.set(
                    Feature::StructuredOutput,
                    Disposition::Reject(
                        "the selected model cannot enforce the requested output constraint",
                    ),
                );
            }
            if !source.reasoning().context.is_absent()
                || api == GenerationApi::Responses && !source.reasoning().read_history.is_absent()
            {
                table.set(
                    Feature::ReasoningContext,
                    Disposition::Reject(
                        "the selected carrier cannot enforce the explicit history-read scope",
                    ),
                );
            }
            if !source.reasoning().mode.is_absent()
                && (api == GenerationApi::Responses
                    || source.reasoning().mode.value() != Some(&ReasoningMode::Budgeted))
            {
                table.set(
                    Feature::ReasoningMode,
                    Disposition::Reject("the selected carrier cannot enforce this allocation mode"),
                );
            }
            if source.tools().iter().any(|tool| match tool {
                ToolDefinition::Function(function) => {
                    function.output_schema.is_some() || !function.dispatch.is_inactive()
                }
                _ => true,
            }) {
                table.set(Feature::Tools, Disposition::Reject("the selected native tool grammar has no carrier for this dispatch, output-schema or namespace contract"));
            }
            let choice = source.tool_choice();
            if api == GenerationApi::Chat
                && (matches!(
                    choice,
                    Some(
                        ToolChoice::Allowed { .. }
                            | ToolChoice::Custom(_)
                            | ToolChoice::Qualified(_)
                    )
                ) || qwen && matches!(choice, Some(ToolChoice::Required))
                    || !disabled && matches!(choice, Some(ToolChoice::Specific(_))))
                || api == GenerationApi::Responses
                    && matches!(
                        choice,
                        Some(
                            ToolChoice::Required
                                | ToolChoice::Specific(_)
                                | ToolChoice::Allowed { required: true, .. }
                        )
                    )
                    && source.tools().len() != 1
            {
                table.set(
                    Feature::ToolChoice,
                    Disposition::Reject(
                        "the selected native grammar cannot enforce the requested tool selection",
                    ),
                );
            }
            if api == GenerationApi::Responses {
                for (_, item) in source.items() {
                    match item {
                        Item::ToolCall(call) if !call.context.is_direct() => {
                            table.set(Feature::Tools, Disposition::Reject("the selected tool-call carrier cannot preserve this call context"));
                        }
                        Item::ToolResult(result) if !matches!(result.output, ToolOutput::Text(_)) => {
                            table.set(Feature::Tools, Disposition::Reject("the selected function-output carrier is text-only; omitting feedback would change the tool result"));
                        }
                        Item::Reasoning(reasoning) if !crate::protocol::openai::dialect::bailian::responses_reasoning_carrier(reasoning) => {
                            table.set(Feature::ReasoningHistory, Disposition::Reject("the selected history carrier cannot preserve the retained reasoning owner"));
                        }
                        Item::CustomCall(_) | Item::CustomResult(_) | Item::Program(_) | Item::ProgramOutput(_) => {
                            table.set(Feature::Tools, Disposition::Reject("the selected tool grammar cannot preserve this executable owner"));
                        }
                        Item::ConfigurationUpdate(_) => {
                            for feature in [Feature::Effort, Feature::ReasoningActivation] {
                                table.set(feature, Disposition::Reject("the selected history grammar has no configuration-update carrier"));
                            }
                        }
                        Item::Compaction(_) => {
                            table.set(Feature::Compaction, Disposition::Reject("the selected history grammar has no compaction-state carrier"));
                        }
                        _ => {}
                    }
                }
            }
        }
    }
    fn presentation_rows(self, settings: &GenerationSettings, table: &mut Dispositions) {
        let presentation = &settings.reasoning.presentation;
        let auto = presentation.summary.value() == Some(&ReasoningSummary::Auto);
        let summary_carrier = matches!(self, Self::OpenAi(GenerationApi::Responses, c)
            if !c.adaptation.rules.bailian_requests);
        let native_summary = match self {
            Self::Messages(_) => Some(super::disposition::MESSAGES_SUMMARY),
            Self::GenerateContent(_) => Some(super::disposition::GENERATE_CONTENT_SUMMARY),
            Self::Interactions(_) => Some(super::disposition::INTERACTIONS_SUMMARY),
            Self::OpenAi(_, _) => None,
        };
        let omission = Disposition::Omit(super::disposition::OMIT_REASONING_SUMMARY);
        let display = if matches!(self, Self::OpenAi(_, _))
            && presentation.visibility.value() == Some(&ReasoningDisplay::Omitted)
        {
            Disposition::Reject("the selected carrier cannot guarantee hidden reasoning display")
        } else if summary_carrier || native_summary.is_some() {
            Disposition::Native
        } else if auto {
            Disposition::Equivalent
        } else {
            omission
        };
        table.set(Feature::ReasoningDisplay, display);
        let summary = if let Some(rule) = native_summary {
            match &presentation.summary {
                Presence::Null => Disposition::Reject(
                    "the selected native summary carrier has no explicit null value",
                ),
                Presence::Value(ReasoningSummary::Auto) => {
                    if matches!(self, Self::Messages(_))
                        && presentation.visibility.value() == Some(&ReasoningDisplay::Summary)
                    {
                        table.set(Feature::ReasoningDisplay, Disposition::Equivalent);
                    }
                    Disposition::Equivalent
                }
                Presence::Value(ReasoningSummary::Concise | ReasoningSummary::Detailed) => {
                    let no_thinking = matches!(self, Self::Messages(_))
                        && settings.reasoning.activation.value()
                            != Some(&ReasoningActivation::Enabled)
                        && settings.reasoning.mode.value() != Some(&ReasoningMode::Adaptive)
                        && settings.reasoning.budget_tokens.is_none();
                    if no_thinking {
                        table.set(Feature::ReasoningDisplay, omission);
                        omission
                    } else {
                        Disposition::Approximate(rule)
                    }
                }
                Presence::Absent => Disposition::Native,
            }
        } else if summary_carrier {
            Disposition::Native
        } else if auto {
            Disposition::Equivalent
        } else {
            omission
        };
        table.set(Feature::ReasoningSummary, summary);
        table.set(
            Feature::EncryptedReasoning,
            if summary_carrier {
                Disposition::Native
            } else if settings.reasoning.activation.value() == Some(&ReasoningActivation::Disabled)
            {
                Disposition::Equivalent
            } else {
                Disposition::Omit(super::disposition::OMIT_REASONING_INCLUDE)
            },
        );
    }
    fn context_rows(self, source: &Request, table: &mut Dispositions) -> Result<(), CodecError> {
        use crate::semantic::context::{CacheOptions, CacheRetention};
        let cache = self.cache();
        table.set(
            Feature::SessionAffinity,
            match self {
                Target::GenerateContent(_) => {
                    Disposition::Omit(super::disposition::OMIT_GENERATE_CONTENT_SESSION)
                }
                Target::Interactions(_) => {
                    Disposition::Omit(super::disposition::OMIT_INTERACTIONS_SESSION)
                }
                _ if cache.session_id => Disposition::Native,
                _ => Disposition::Reject(
                    "the explicit session has no selected upstream grouping carrier",
                ),
            },
        );
        table.set_native(Feature::CacheKey, cache.key);
        table.set_native(Feature::CacheRetention, cache.retention);
        if !cache.retention
            && let Some(CacheRetention::InMemory) = source.context.cache.prompt_cache_retention
        {
            table.set(
                Feature::CacheRetention,
                Disposition::Reject(
                    "in-memory retention cannot be replaced by an unknown storage policy",
                ),
            );
        }
        if source.context.cache.prompt_cache_options == Presence::Null {
            return Err(CodecError::invalid());
        }
        table.set(
            Feature::CacheOptions,
            if cache.options {
                Disposition::Native
            } else if source
                .context
                .cache
                .prompt_cache_options
                .value()
                .is_none_or(|v| v == &CacheOptions::default())
            {
                Disposition::Equivalent
            } else {
                Disposition::Reject("explicit cache options require an exact carrier")
            },
        );
        let service = source.service_policy.as_ref();
        let carries_service = table.native(Feature::ServicePolicy)
            && match self {
                Target::OpenAi(api, _) => service.is_none_or(|value| {
                    crate::protocol::openai::service::policy_wire(*value, api).is_ok()
                }),
                Target::Messages(_) => service.is_none_or(|value| {
                    crate::protocol::anthropic::messages::service::policy_wire(*value).is_ok()
                }),
                _ => false,
            };
        if !carries_service {
            table.set(
                Feature::ServicePolicy,
                Disposition::Omit(super::disposition::OMIT_SERVICE),
            );
        }
        Ok(())
    }
}

fn omit_option<T>(
    value: &mut Option<T>,
    feature: Feature,
    table: &Dispositions,
    evidence: &mut EvidenceDelta,
) {
    if let Disposition::Omit(rule) = table.get(feature)
        && value.take().is_some()
    {
        evidence.push(rule.effect(ConversionBoundary::RequestProject));
    }
}
fn apply_presence<T>(
    value: &mut Presence<T>,
    feature: Feature,
    table: &Dispositions,
    evidence: &mut EvidenceDelta,
) -> Result<(), ConversionFailure> {
    if value.is_absent() {
        return Ok(());
    }
    match table.get(feature) {
        Disposition::Omit(rule) => {
            if value.value().is_some() {
                evidence.push(rule.effect(ConversionBoundary::RequestProject));
            }
            *value = Presence::Absent;
        }
        Disposition::Equivalent => *value = Presence::Absent,
        Disposition::Reject(_) => {
            return Err(
                crate::protocol::conversion::generation::RepresentationError::Admission(feature)
                    .into(),
            );
        }
        _ => {}
    }
    Ok(())
}

pub(crate) fn project_semantic<'a>(
    source: &'a GenerationRequest,
    target: Target<'_>,
    model: &str,
) -> Result<Conversion<Cow<'a, GenerationRequest>>, ConversionFailure> {
    let mut table = target.table(source, model);
    let projected = project_semantic_with_table(source, target, &mut table)?;
    target.projected_rows(projected.value.as_ref(), model, false, &mut table);
    table
        .check(projected.value.as_ref())
        .map_err(crate::protocol::conversion::generation::RepresentationError::Admission)?;
    Ok(projected)
}

fn project_semantic_with_table<'a>(
    source: &'a GenerationRequest,
    target: Target<'_>,
    table: &mut Dispositions,
) -> Result<Conversion<Cow<'a, GenerationRequest>>, ConversionFailure> {
    source.validate().map_err(CodecError::from)?;
    target.limits().validate()?;
    if let Target::OpenAi(_, contract) = target
        && contract.adaptation.reasoning_policy.is_some()
        && contract.adaptation.effort_mapping.is_some()
    {
        return Err(CodecError::invalid().into());
    }
    let mut evidence = EvidenceDelta::default();
    let mut semantic = Cow::Borrowed(source);
    let mut initial = source.settings().clone();
    if !table.accepts(Feature::ReasoningActivation)
        && source
            .features()
            .contains_key(&Feature::ReasoningActivation)
    {
        return Err(
            crate::protocol::conversion::generation::RepresentationError::Admission(
                Feature::ReasoningActivation,
            )
            .into(),
        );
    }
    if initial.reasoning.replay_mismatch.is_some() && !table.accepts(Feature::ReplayMismatch) {
        return Err(
            crate::protocol::conversion::generation::RepresentationError::Admission(
                Feature::ReplayMismatch,
            )
            .into(),
        );
    }
    omit_option(
        &mut initial.reasoning.replay_mismatch,
        Feature::ReplayMismatch,
        table,
        &mut evidence,
    );
    if initial.reasoning.budget_tokens.is_some()
        && initial.effort.is_some()
        && (!matches!(target, Target::Messages(_)) || !target.budget_fits(&initial))
    {
        initial.reasoning.budget_tokens = None;
        if initial.reasoning.mode.value() == Some(&ReasoningMode::Budgeted) {
            initial.reasoning.mode = if matches!(target, Target::Messages(_)) {
                Presence::Value(ReasoningMode::Adaptive)
            } else {
                Presence::Absent
            };
        }
        evidence.push(
            super::disposition::BUDGET_WITH_EFFORT.effect(ConversionBoundary::RequestProject),
        );
    }
    if initial.reasoning.activation.value() == Some(&ReasoningActivation::Disabled)
        && initial.effort.is_some()
        && target.effort_scope() == EffortScope::Reasoning
    {
        initial.effort = None;
        evidence.push(
            super::disposition::unsupported(Feature::Effort)
                .rule()
                .expect("effort omission rule")
                .effect(ConversionBoundary::RequestProject),
        );
    }
    if &initial != source.settings() {
        semantic = Cow::Owned(
            source
                .clone()
                .with_settings(initial)
                .map_err(CodecError::from)?,
        );
    }
    let mut applied_policy = None;
    if let Some(policy) = target.policy()
        && let Some(copy) = policy.project_request(&semantic, |s| {
            if s.reasoning.budget_tokens.is_some() {
                target.budget_fits(s)
                    || s.effort.is_some()
                        && matches!(
                            policy.conversion,
                            crate::protocol::reasoning::ReasoningConversion::BudgetToEffort { .. }
                        )
            } else {
                s.effort
                    .as_ref()
                    .is_some_and(|e| target.has_effort() && e.scope == target.effort_scope())
            }
        })?
    {
        applied_policy = Some(policy);
        semantic = Cow::Owned(copy);
    }
    let mut settings = semantic.settings().clone();
    if matches!(target, Target::OpenAi(_, contract) if contract.adaptation.rules.bailian_requests)
        && settings.text.format == Presence::Value(OutputConstraint::Text)
    {
        settings.text.format = Presence::Absent;
    }
    if table.get(Feature::ReasoningMode) == Disposition::Equivalent {
        settings.reasoning.mode = Presence::Absent;
    }
    fn strip_eager(tools: &mut [ToolDefinition]) -> bool {
        let mut changed = false;
        for tool in tools {
            match tool {
                ToolDefinition::Function(tool) => changed |= tool.eager_input.take().is_some(),
                ToolDefinition::Namespace(namespace) => {
                    changed |= strip_eager(&mut namespace.tools)
                }
                _ => {}
            }
        }
        changed
    }
    if let Disposition::Omit(rule) = table.get(Feature::EagerToolInput)
        && strip_eager(settings.tools.as_deref_mut().unwrap_or_default())
    {
        evidence.push(rule.effect(ConversionBoundary::RequestProject));
    }
    if matches!(target, Target::Messages(_))
        && !target.has_budget()
        && source.reasoning().budget_tokens.is_some()
        && settings.reasoning.budget_tokens.is_none()
    {
        settings.reasoning.mode = Presence::Value(ReasoningMode::Adaptive);
    }
    let original_controls = settings.controls.clone();
    let mut controls = original_controls
        .clone()
        .without_sampling()
        .without_penalties();
    for (feature, value) in [
        (Feature::Temperature, original_controls.temperature()),
        (Feature::TopP, original_controls.top_p()),
        (
            Feature::PresencePenalty,
            original_controls.presence_penalty(),
        ),
        (
            Feature::FrequencyPenalty,
            original_controls.frequency_penalty(),
        ),
    ] {
        if let Some(value) = value {
            if let Disposition::Omit(rule) = table.get(feature) {
                evidence.push(rule.effect(ConversionBoundary::RequestProject));
            } else {
                controls = match feature {
                    Feature::Temperature => controls.with_temperature(value),
                    Feature::TopP => controls.with_top_p(value),
                    Feature::PresencePenalty => controls.with_presence_penalty(value),
                    Feature::FrequencyPenalty => controls.with_frequency_penalty(value),
                    _ => unreachable!(),
                }
                .map_err(CodecError::from)?;
            }
        }
    }
    omit_option(&mut controls.top_k, Feature::TopK, table, &mut evidence);
    omit_option(&mut controls.seed, Feature::Seed, table, &mut evidence);
    if controls.logprobs == Some(false)
        && controls.top_logprobs.is_none()
        && matches!(table.get(Feature::Logprobs), Disposition::Omit(_))
    {
        controls.logprobs = None;
    }
    omit_option(
        &mut controls.logprobs,
        Feature::Logprobs,
        table,
        &mut evidence,
    );
    if matches!(table.get(Feature::Logprobs), Disposition::Omit(_))
        && controls.top_logprobs.take().is_some()
        && let Some(rule) = table.get(Feature::Logprobs).rule()
    {
        evidence.push(rule.effect(ConversionBoundary::RequestProject));
    }
    omit_option(
        &mut controls.truncation,
        Feature::Truncation,
        table,
        &mut evidence,
    );
    omit_option(
        &mut settings.text.verbosity,
        Feature::Verbosity,
        table,
        &mut evidence,
    );
    omit_option(
        &mut settings.prediction,
        Feature::Prediction,
        table,
        &mut evidence,
    );
    omit_option(
        &mut settings.automatic_prefix_cache,
        Feature::AutomaticPrefixCache,
        table,
        &mut evidence,
    );
    if settings.parallel_tool_calls == Some(true) {
        match table.get(Feature::ParallelTools) {
            Disposition::Equivalent => settings.parallel_tool_calls = None,
            Disposition::Omit(rule) => {
                settings.parallel_tool_calls = None;
                evidence.push(rule.effect(ConversionBoundary::RequestProject));
            }
            _ => {}
        }
    }
    if settings.parallel_tool_calls == Some(false) && !table.accepts(Feature::SerialTools) {
        return Err(crate::protocol::conversion::generation::RepresentationError::Tools.into());
    }
    if settings.reasoning.presence == ReasoningPresence::Null {
        settings.reasoning.presence = ReasoningPresence::Absent;
    }
    if !target.has_budget() && settings.reasoning.budget_tokens.is_some() {
        let budget = settings
            .reasoning
            .budget_tokens
            .take()
            .expect("present budget");
        let rule = if settings.effort.as_ref().is_some() {
            super::disposition::BUDGET_WITH_EFFORT
        } else {
            super::disposition::BUDGET_TO_EFFORT
        };
        if settings.effort.is_none() && target.has_effort() {
            let (level, _) = crate::protocol::reasoning::SHARED_BUDGETS.nearest(budget)?;
            settings.effort = Some(EffortControl {
                scope: target.effort_scope(),
                level,
            });
        }
        settings.reasoning.mode = if matches!(target, Target::Messages(_)) {
            Presence::Value(ReasoningMode::Adaptive)
        } else {
            Presence::Absent
        };
        evidence.push(rule.effect(ConversionBoundary::RequestProject));
    }
    let effort_levels = std::cell::LazyCell::new(|| target.levels());
    let mut effort_rule = None;
    if let Some(effort) = settings.effort {
        if target.has_effort() {
            if matches!(target, Target::OpenAi(_, _))
                && !target.has_budget()
                && matches!(
                    settings.reasoning.mode.value(),
                    Some(ReasoningMode::Adaptive | ReasoningMode::Budgeted)
                )
            {
                settings.reasoning.mode = Presence::Absent;
                effort_rule = Some(super::disposition::EFFORT.id);
            }
            let (mapped, rule) = target.map_effort(effort, &effort_levels)?;
            settings.effort = Some(mapped);
            if mapped != effort {
                effort_rule = Some(rule);
            }
        } else {
            settings.effort = None;
            if let Some(rule) = table.get(Feature::Effort).rule() {
                evidence.push(rule.effect(ConversionBoundary::RequestProject));
            }
        }
    }
    apply_presence(
        &mut settings.reasoning.activation,
        Feature::ReasoningActivation,
        table,
        &mut evidence,
    )?;
    apply_presence(
        &mut settings.reasoning.mode,
        Feature::ReasoningMode,
        table,
        &mut evidence,
    )?;
    // Allocation conversion may change whether a native summarizer is active.
    target.presentation_rows(&settings, table);
    apply_presence(
        &mut settings.reasoning.presentation.visibility,
        Feature::ReasoningDisplay,
        table,
        &mut evidence,
    )?;
    if let Disposition::Approximate(rule) = table.get(Feature::ReasoningSummary) {
        settings.reasoning.presentation.summary = Presence::Absent;
        evidence.push(rule.effect(ConversionBoundary::RequestProject));
    } else {
        apply_presence(
            &mut settings.reasoning.presentation.summary,
            Feature::ReasoningSummary,
            table,
            &mut evidence,
        )?;
    }
    if settings.reasoning.encrypted_output() {
        match table.get(Feature::EncryptedReasoning) {
            Disposition::Equivalent => {
                settings.reasoning = settings.reasoning.with_encrypted_output(false);
            }
            Disposition::Omit(rule) => {
                settings.reasoning = settings.reasoning.with_encrypted_output(false);
                evidence.push(rule.effect(ConversionBoundary::RequestProject));
            }
            _ => {}
        }
    }
    if let Target::Messages(contract) = target {
        if controls.max_output_tokens.is_none() {
            let default = contract
                .default_output_tokens
                .ok_or(CodecError::Unsupported(CarrierReason::Unavailable))?;
            controls.max_output_tokens = Some(
                contract
                    .model_limits
                    .maximum_output_tokens
                    .map_or(default, |maximum| default.min(maximum)),
            );
        }
        if settings.automatic_prefix_cache.is_none()
            && !settings
                .instructions
                .value()
                .is_some_and(|root| root.parts.iter().any(|part| part.cache.is_some()))
            && !semantic
                .items()
                .iter()
                .any(|(_, item)| item.has_cache_breakpoints())
            && !semantic
                .tools()
                .iter()
                .flat_map(ToolDefinition::leaves)
                .any(|tool| matches!(tool, ToolDefinition::Function(t) if t.cache.is_some()))
        {
            settings.automatic_prefix_cache = contract.automatic_cache_default;
        }
    }
    if let Some(maximum) = target.limits().maximum_output_tokens
        && controls.max_output_tokens.is_some_and(|n| n > maximum)
    {
        controls.max_output_tokens = Some(maximum);
        evidence.push(super::disposition::CLAMP_OUTPUT.effect(ConversionBoundary::RequestProject));
    }
    omit_option(
        &mut controls.max_output_tokens,
        Feature::MaxOutputTokens,
        table,
        &mut evidence,
    );
    if settings
        .reasoning
        .budget_tokens
        .zip(controls.max_output_tokens)
        .is_some_and(|(budget, ceiling)| {
            budget > ceiling || matches!(target, Target::Messages(_)) && budget == ceiling
        })
    {
        return Err(
            crate::protocol::conversion::generation::RepresentationError::Admission(
                Feature::ReasoningBudget,
            )
            .into(),
        );
    }
    settings.controls = controls;
    {
        let mut reasoning = settings.reasoning.clone();
        reasoning.presence = ReasoningPresence::Absent;
        if reasoning == ReasoningRequest::default() {
            settings.reasoning = reasoning;
        }
    }
    if settings.text.format.is_absent() && settings.text.verbosity.is_none() {
        settings.text.presence = Presence::Absent;
    }
    let mut items = semantic.items().to_vec();
    for (id, item) in &mut items {
        if let Item::ToolChange(change) = item
            && let ToolChangeAction::AddDefinition(tool) = &mut change.action
            && let Disposition::Omit(rule) = table.get(Feature::EagerToolInput)
            && tool.eager_input.take().is_some()
        {
            evidence.push(
                rule.effect(ConversionBoundary::RequestProject)
                    .at(Owner::item(*id)),
            );
        }
        if let Item::ConfigurationUpdate(update) = item
            && let Some(effort) = update.effort
        {
            if !target.has_effort() {
                update.effort = None;
                if let Some(rule) = table.get(Feature::Effort).rule() {
                    evidence.push(
                        rule.effect(ConversionBoundary::RequestProject)
                            .at(crate::protocol::conversion::generation::Owner::item(*id)),
                    );
                }
                continue;
            }
            let (mapped, rule) = target.map_effort(effort, &effort_levels)?;
            if mapped != effort {
                update.effort = Some(mapped);
                evidence.push(
                    crate::protocol::conversion::generation::Effect::new(
                        rule,
                        crate::protocol::conversion::generation::Field::Effort,
                        ConversionBoundary::RequestProject,
                        crate::protocol::conversion::generation::Change::Approximate,
                    )
                    .at(crate::protocol::conversion::generation::Owner::item(*id)),
                );
            }
        }
    }
    items.retain(|(_, item)| {
        !matches!(item, Item::ConfigurationUpdate(update)
        if update.activation.is_none() && update.effort.is_none())
    });
    if let Disposition::Omit(rule) = table.get(Feature::ToolExecutionReport) {
        for (id, item) in &mut items {
            if let Item::ToolResult(result) | Item::CustomResult(result) = item
                && result.execution.take().is_some()
            {
                evidence.push(
                    rule.effect(ConversionBoundary::RequestProject)
                        .at(crate::protocol::conversion::generation::Owner::item(*id)),
                );
            }
        }
    }
    let bailian =
        matches!(target, Target::OpenAi(_, contract) if contract.adaptation.rules.bailian_requests);
    if bailian {
        fn descriptions(tool: &mut ToolDefinition) {
            match tool {
                ToolDefinition::Function(f) => {
                    f.description.get_or_insert_with(String::new);
                }
                ToolDefinition::Namespace(ns) => ns.tools.iter_mut().for_each(descriptions),
                _ => {}
            }
        }
        if let Some(tools) = &mut settings.tools {
            tools.iter_mut().for_each(descriptions);
        }
        for (_, item) in &mut items {
            if let Item::ToolChange(change) = item
                && let ToolChangeAction::AddDefinition(tool) = &mut change.action
            {
                tool.description.get_or_insert_with(String::new);
            }
        }
    }
    for (_, item) in &mut items {
        fn detail(use_: &mut ResourceUse, target: Target<'_>, evidence: &mut EvidenceDelta) {
            let ResourceDescription::Image { detail, .. } = &mut use_.description else {
                return;
            };
            let Some(value) = *detail else { return };
            let carried = match target {
                Target::OpenAi(api, c) => {
                    c.images.details.contains(&value)
                        && !c.adaptation.rules.omit_image_detail
                        && (api != GenerationApi::Chat
                            || value != ImageDetail::Original
                            || c.adaptation.rules.chat_original_image_detail)
                }
                _ => false,
            };
            if !carried {
                *detail = None;
                if value != ImageDetail::Auto {
                    evidence.push(
                        super::disposition::OMIT_IMAGE_DETAIL
                            .effect(ConversionBoundary::RequestProject),
                    );
                }
            }
        }
        match item {
            Item::Message(message) => {
                for part in &mut message.parts {
                    if let ContentPart::Resource(use_) = &mut part.content {
                        detail(use_, target, &mut evidence);
                    }
                }
            }
            Item::ToolResult(result) | Item::CustomResult(result) => {
                if let ToolOutput::Parts(parts) = &mut result.output {
                    for part in parts {
                        if let InputPart::Resource(use_) = &mut part.content {
                            detail(use_, target, &mut evidence);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    project_cache(&mut settings, &mut items, table, &mut evidence);
    if let Disposition::Omit(rule) = table.get(Feature::MessagePhase) {
        let mut changed = false;
        for (_, item) in &mut items {
            if let Item::Message(message) = item {
                changed |= message.phase.take().is_some();
            }
        }
        if changed {
            evidence.push(rule.effect(ConversionBoundary::RequestProject));
        }
    }
    if &settings != semantic.settings() || items != semantic.items() {
        let envelopes = if items.len() != semantic.items().len() {
            semantic
                .clone()
                .into_owned()
                .with_items(items.clone())
                .map_err(CodecError::from)?
                .message_envelopes()
                .to_vec()
        } else {
            semantic.message_envelopes().to_vec()
        };
        semantic = Cow::Owned(
            GenerationRequest::from_resources_and_envelopes(
                items,
                settings,
                semantic.resources().clone(),
                envelopes,
            )
            .map_err(CodecError::from)?
            .with_cached_context(semantic.cached_context().cloned())
            .map_err(CodecError::from)?,
        );
    }
    if matches!(target, Target::Messages(_)) {
        let flatten = matches!(
            table.get(Feature::MessageEffort),
            Disposition::Approximate(_)
        ) && semantic
            .items()
            .iter()
            .any(|(_, item)| matches!(item, Item::ConfigurationUpdate(_)));
        let collapse = matches!(table.get(Feature::ToolChanges), Disposition::Approximate(_))
            && semantic
                .items()
                .iter()
                .any(|(_, item)| matches!(item, Item::ToolChange(_)));
        if semantic
            .items()
            .iter()
            .any(|(_, item)| matches!(item, Item::ToolChange(_)))
            && !table.accepts(Feature::ToolChanges)
        {
            return Err(
                crate::protocol::conversion::generation::RepresentationError::Admission(
                    Feature::ToolChanges,
                )
                .into(),
            );
        }
        if flatten || collapse {
            let mut settings = semantic.settings().clone();
            let mut items = semantic.items().to_vec();
            if flatten {
                settings.effort = crate::protocol::anthropic::messages::carriers::flatten_effort(
                    settings.effort,
                    &items,
                )?;
                items.retain(|(_, item)| !matches!(item, Item::ConfigurationUpdate(_)));
                evidence.push(
                    super::disposition::MESSAGE_EFFORT.effect(ConversionBoundary::RequestProject),
                );
            }
            if collapse {
                settings.tools = Some(
                    crate::protocol::anthropic::messages::carriers::collapse_tools(
                        semantic.tools(),
                        &items,
                    )?,
                );
                items.retain(|(_, item)| !matches!(item, Item::ToolChange(_)));
                evidence.push(
                    super::disposition::TOOL_CHANGES.effect(ConversionBoundary::RequestProject),
                );
            }
            semantic = Cow::Owned(
                semantic
                    .into_owned()
                    .with_items(items)
                    .map_err(CodecError::from)?
                    .with_settings(settings)
                    .map_err(CodecError::from)?,
            );
        }
    }
    if matches!(target, Target::Messages(_)) {
        semantic = Cow::Owned(crate::protocol::native::group_client_history(
            &semantic, true,
        )?);
    }
    if matches!(
        table.get(Feature::ConditionalToolStrictness),
        Disposition::Approximate(_)
    ) {
        let resolved = crate::protocol::openai::lowering::strictness::project(
            &semantic,
            |schema| match target {
                Target::Messages(_) => {
                    table.native(Feature::StrictTools)
                        && crate::protocol::anthropic::messages::strict_schema::admits(
                            schema.root(),
                        )
                }
                Target::OpenAi(_, contract) => {
                    table.native(Feature::StrictTools)
                        && !contract.adaptation.rules.bailian_requests
                }
                _ => false,
            },
            |request| {
                !matches!(target, Target::Messages(_))
                    || crate::protocol::anthropic::messages::strict_schema::admits_generation(
                        request,
                    )
            },
        )?;
        if let Some((copy, effects)) = resolved {
            semantic = Cow::Owned(copy);
            for effect in effects {
                evidence.push(effect);
            }
        }
    }
    if let Target::OpenAi(profile, _) = target
        && let Some((groups, rule)) = crate::protocol::openai::lowering::message_groups::project(
            semantic.items(),
            semantic.message_envelopes(),
            profile,
        )?
    {
        semantic = Cow::Owned(
            semantic
                .into_owned()
                .with_message_envelopes(groups)
                .map_err(CodecError::from)?,
        );
        evidence.push(rule.effect(ConversionBoundary::RequestProject));
    }
    if matches!(target, Target::OpenAi(_, _)) {
        semantic = with_schema_name(semantic)?.0;
    } else {
        let rule = match target {
            Target::Messages(_) => super::disposition::MESSAGES_SCHEMA_NAME,
            Target::GenerateContent(_) => super::disposition::GENERATE_CONTENT_SCHEMA_NAME,
            Target::Interactions(_) => super::disposition::INTERACTIONS_SCHEMA_NAME,
            Target::OpenAi(_, _) => unreachable!(),
        };
        table.set(Feature::SchemaName, Disposition::Omit(rule));
        let mut settings = semantic.settings().clone();
        if let Presence::Value(OutputConstraint::JsonSchema { name, .. }) =
            &mut settings.text.format
            && name.take().is_some()
        {
            semantic = Cow::Owned(
                semantic
                    .into_owned()
                    .with_settings(settings)
                    .map_err(CodecError::from)?,
            );
            evidence.push(rule.effect(ConversionBoundary::RequestProject));
        }
    }
    if matches!(
        table.get(Feature::ToolResultError),
        Disposition::Approximate(_)
    ) {
        let (copy, changed) = with_tool_error_status(semantic)?;
        semantic = copy;
        if changed && let Some(rule) = table.get(Feature::ToolResultError).rule() {
            evidence.push(rule.effect(ConversionBoundary::RequestProject));
        }
    }
    if let Some(policy) = applied_policy {
        evidence.push(policy.receipt(source, &semantic)?);
    }
    if let Some(rule) = effort_rule {
        evidence.push(crate::protocol::conversion::generation::Effect::Reasoning {
            rule: crate::protocol::conversion::generation::RuleId::new(rule),
            input: crate::protocol::conversion::generation::ReasoningControls::request(source),
            output: crate::protocol::conversion::generation::ReasoningControls::request(&semantic),
            preset_tokens: None,
            owner: crate::protocol::conversion::generation::Owner::Root,
        });
    }
    Ok(Conversion {
        value: semantic,
        evidence,
    })
}

fn project_cache(
    settings: &mut GenerationSettings,
    items: &mut [(ItemId, Item)],
    table: &Dispositions,
    evidence: &mut EvidenceDelta,
) {
    let ttl = matches!(table.get(Feature::ExplicitCache), Disposition::Omit(_));
    let lifetime = matches!(
        table.get(Feature::RequestLifetimeCache),
        Disposition::Omit(_)
    );
    let mut omitted_ttl = false;
    let mut omitted_lifetime = false;
    let mut boundary = |value: &mut Option<PrefixCacheBoundary>| match *value {
        Some(PrefixCacheBoundary::Ttl(_)) if ttl => {
            *value = None;
            omitted_ttl = true;
        }
        Some(PrefixCacheBoundary::RequestLifetime) if lifetime => {
            *value = None;
            omitted_lifetime = true;
        }
        _ => {}
    };
    if let Presence::Value(root) = &mut settings.instructions {
        for part in &mut root.parts {
            boundary(&mut part.cache);
        }
    }
    for (_, item) in items.iter_mut() {
        match item {
            Item::Instruction(instruction) => {
                for part in &mut instruction.parts {
                    boundary(&mut part.cache);
                }
            }
            Item::Message(message) => {
                for part in &mut message.parts {
                    boundary(&mut part.cache);
                }
            }
            Item::ToolResult(result) | Item::CustomResult(result) => {
                if let ToolOutput::Parts(parts) = &mut result.output {
                    for part in parts {
                        boundary(&mut part.cache);
                    }
                }
            }
            _ => {}
        }
    }
    fn strip_tools(tools: &mut [ToolDefinition]) -> bool {
        let mut changed = false;
        for tool in tools {
            match tool {
                ToolDefinition::Function(tool) => changed |= tool.cache.take().is_some(),
                ToolDefinition::Namespace(group) => changed |= strip_tools(&mut group.tools),
                _ => {}
            }
        }
        changed
    }
    if ttl {
        omitted_ttl |= strip_tools(settings.tools.as_deref_mut().unwrap_or_default());
        for (_, item) in items {
            match item {
                Item::ToolCall(call) => omitted_ttl |= call.cache.take().is_some(),
                Item::ToolResult(result) | Item::CustomResult(result) => {
                    omitted_ttl |= result.cache.take().is_some()
                }
                Item::ToolChange(change) => {
                    omitted_ttl |= change.cache.take().is_some();
                    if let ToolChangeAction::AddDefinition(tool) = &mut change.action {
                        omitted_ttl |= tool.cache.take().is_some();
                    }
                }
                _ => {}
            }
        }
    }
    for (feature, changed) in [
        (Feature::ExplicitCache, omitted_ttl),
        (Feature::RequestLifetimeCache, omitted_lifetime),
    ] {
        if changed && let Some(rule) = table.get(feature).rule() {
            evidence.push(rule.effect(ConversionBoundary::RequestProject));
        }
    }
}

pub(crate) fn project<'a>(
    source: &'a Request,
    target: Target<'_>,
    model: &str,
    native_target: Option<&crate::protocol::native::ReplayTarget>,
) -> Result<Conversion<Cow<'a, Request>>, ConversionFailure> {
    source.check_grouping()?;
    source.context.validate().map_err(CodecError::from)?;
    source.delivery.validate().map_err(CodecError::from)?;
    source.task.semantic.validate().map_err(CodecError::from)?;
    // Existing trust-qualified transfer precedes target field admission. A
    // foreign owner can disappear without making the retained history unsupported.
    let mut stripped = if let Target::OpenAi(api, contract) = target {
        let issuer = contract
            .replay_origin
            .as_ref()
            .or(contract.adaptation.scope.as_ref());
        if api == GenerationApi::Responses
            && contract.adaptation.rules.bailian_requests
            && contract.reasoning_history
                == Some(crate::protocol::reasoning::ReasoningHistory::Accepted)
        {
            source.without_uncarried_reasoning(issuer, |reasoning| {
                let omit = !crate::protocol::openai::dialect::bailian::responses_reasoning_carrier(
                    reasoning,
                );
                if omit && source.task.semantic.reasoning().read_history.value() == Some(&true) {
                    return Err(CodecError::Unsupported(CarrierReason::Reasoning));
                }
                Ok(omit)
            })?
        } else {
            source.without_foreign_reasoning(issuer)?
        }
    } else {
        let native = native_target.ok_or_else(CodecError::invalid)?;
        source.without_foreign_reasoning(Some(&native.origin()))?
    };
    let mut transferred = stripped.is_some();
    if let Some(native) = native_target {
        let formats: &[ReplayFormat] = match target {
            Target::Messages(_) => &[
                ReplayFormat::AnthropicMessagesThinking,
                ReplayFormat::AnthropicMessagesRedactedThinking,
            ],
            Target::GenerateContent(_) => &[
                ReplayFormat::GoogleGenerateContentPart,
                ReplayFormat::GoogleGenerateContentSignature,
            ],
            Target::Interactions(_) => &[
                ReplayFormat::GoogleInteractionsV1Thought,
                ReplayFormat::GoogleInteractionsV1FunctionCall,
            ],
            Target::OpenAi(_, _) => return Err(CodecError::invalid().into()),
        };
        let input = stripped.as_ref().unwrap_or(source);
        let (view, fidelity, changed) = crate::protocol::native::native_reasoning_history(
            &input.task.semantic,
            &input.task.fidelity,
            native,
            formats,
        )?;
        let changed_fidelity = fidelity != input.task.fidelity;
        let owned = match view {
            Cow::Owned(value) => Some(value),
            Cow::Borrowed(_) => None,
        };
        if owned.is_some() || changed_fidelity {
            let input = stripped.get_or_insert_with(|| source.clone());
            if let Some(semantic) = owned {
                input.task.semantic = semantic;
            }
            input.task.fidelity = fidelity;
        }
        transferred |= changed;
    }
    let input = stripped.as_ref().unwrap_or(source);
    let mut table = target.table(&input.task.semantic, model);
    target.context_rows(source, &mut table)?;
    let semantic = project_semantic_with_table(&input.task.semantic, target, &mut table)?;
    let mut evidence = semantic.evidence;
    let semantic = match semantic.value {
        Cow::Owned(value) => Some(value),
        Cow::Borrowed(_) => None,
    };
    let mut projected = stripped.map_or(Cow::Borrowed(source), Cow::Owned);
    if let Some(semantic) = semantic {
        projected.to_mut().task.semantic = semantic;
    }
    if source.cache_session.is_some() {
        if !table.accepts(Feature::SessionAffinity) {
            return Err(
                crate::protocol::conversion::generation::RepresentationError::Admission(
                    Feature::SessionAffinity,
                )
                .into(),
            );
        }
        if let Disposition::Omit(rule) = table.get(Feature::SessionAffinity) {
            projected.to_mut().cache_session = None;
            evidence.push(rule.effect(ConversionBoundary::RequestProject));
        }
    }
    if transferred {
        let rule = match target {
            Target::OpenAi(GenerationApi::Chat, _) => super::disposition::TRANSFER_CHAT_HISTORY,
            Target::OpenAi(GenerationApi::Responses, _) => {
                super::disposition::TRANSFER_RESPONSES_HISTORY
            }
            Target::Messages(_) => super::disposition::TRANSFER_MESSAGES_HISTORY,
            Target::GenerateContent(_) => super::disposition::TRANSFER_GENERATE_CONTENT_HISTORY,
            Target::Interactions(_) => super::disposition::TRANSFER_INTERACTIONS_HISTORY,
        };
        evidence.push(rule.effect(ConversionBoundary::RequestProject));
    }
    let mut context = source.context.clone();
    if !matches!(target, Target::OpenAi(_, contract)
        if !contract.adaptation.rules.responses_siwc && !contract.adaptation.rules.bailian_requests)
    {
        crate::protocol::native::normalize_stateless_defaults(&mut context);
    }
    use crate::protocol::attribution::{self, Carrier};
    let carrier = match target {
        Target::OpenAi(_, contract)
            if table.native(Feature::Identity) && !contract.adaptation.rules.responses_siwc =>
        {
            Carrier::OpenAi
        }
        Target::Messages(_) => Carrier::Messages,
        Target::GenerateContent(_) => Carrier::GoogleLabels,
        _ => Carrier::None,
    };
    let attribution = attribution::project(source, carrier)?;
    context.identity = attribution.identity;
    for rule in attribution.losses {
        evidence.push(rule.effect(ConversionBoundary::RequestProject));
    }
    if context.max_tool_calls == Presence::Null {
        context.max_tool_calls = Presence::Absent;
    }
    apply_presence(
        &mut context.max_tool_calls,
        Feature::ToolBudget,
        &table,
        &mut evidence,
    )?;
    apply_presence(
        &mut context.metadata,
        Feature::Metadata,
        &table,
        &mut evidence,
    )?;
    omit_option(
        &mut context.cache.prompt_cache_key,
        Feature::CacheKey,
        &table,
        &mut evidence,
    );
    if context.cache.prompt_cache_retention.is_some() && !table.accepts(Feature::CacheRetention) {
        return Err(
            crate::protocol::conversion::generation::RepresentationError::Admission(
                Feature::CacheRetention,
            )
            .into(),
        );
    }
    omit_option(
        &mut context.cache.prompt_cache_retention,
        Feature::CacheRetention,
        &table,
        &mut evidence,
    );
    apply_presence(
        &mut context.cache.prompt_cache_options,
        Feature::CacheOptions,
        &table,
        &mut evidence,
    )?;
    if matches!(target, Target::OpenAi(_, _)) {
        target.cache().infer_key(
            &mut context,
            source.conversation.as_ref(),
            source.inferred_affinity,
        );
    }
    let mut service = source.service_policy;
    omit_option(&mut service, Feature::ServicePolicy, &table, &mut evidence);
    if context != source.context {
        projected.to_mut().context = context;
    }
    if service != source.service_policy {
        projected.to_mut().service_policy = service;
    }
    target.projected_rows(
        &projected.task.semantic,
        model,
        projected.delivery.streaming(),
        &mut table,
    );
    table
        .check(&projected.task.semantic)
        .map_err(crate::protocol::conversion::generation::RepresentationError::Admission)?;
    Ok(Conversion {
        value: projected,
        evidence,
    })
}

/// OpenAI requires a schema name; Messages and Gemini formats have none. The fixed
/// name is a wire label, not a new semantic output constraint.
fn with_schema_name(
    request: Cow<'_, crate::semantic::task::generation::GenerationRequest>,
) -> Result<
    (
        Cow<'_, crate::semantic::task::generation::GenerationRequest>,
        bool,
    ),
    CodecError,
> {
    use crate::semantic::{
        task::generation::OutputConstraint,
        value::{Presence, Text},
    };
    if !matches!(
        request.settings().text.format,
        Presence::Value(OutputConstraint::JsonSchema { name: None, .. })
    ) {
        return Ok((request, false));
    }
    let mut settings = request.settings().clone();
    let Presence::Value(OutputConstraint::JsonSchema {
        name: name @ None, ..
    }) = &mut settings.text.format
    else {
        unreachable!("checked schema name");
    };
    *name = Some(Text::new("response", "schema name", 64).expect("static name"));
    let request = request.into_owned().with_settings(settings)?;
    Ok((Cow::Owned(request), true))
}

fn with_tool_error_status(
    request: Cow<'_, crate::semantic::task::generation::GenerationRequest>,
) -> Result<
    (
        Cow<'_, crate::semantic::task::generation::GenerationRequest>,
        bool,
    ),
    CodecError,
> {
    use crate::semantic::{task::generation::*, value::Text};
    if !request
            .items()
            .iter()
            .any(|(_, item)| matches!(item, Item::ToolResult(result) | Item::CustomResult(result) if result.is_error.is_some()))
    {
        return Ok((request, false));
    }
    let mut next = request
        .items()
        .iter()
        .flat_map(|(_, item)| item.part_ids())
        .filter(|id| id.scope() == LocalScope::DERIVED)
        .map(PartId::get)
        .max()
        .unwrap_or(0);
    let mut items = request.items().to_vec();
    for (_, item) in &mut items {
        if let Item::ToolResult(result) | Item::CustomResult(result) = item
            && let Some(is_error) = result.is_error.take()
        {
            // This reports the supplied flag, not an inferred execution conclusion.
            let prefix = format!("[tool result is_error={is_error}]\n");
            match &mut result.output {
                ToolOutput::Text(text) => text.insert_str(0, &prefix),
                ToolOutput::Structured(value) => {
                    result.output = ToolOutput::Text(format!("{prefix}{}", value.value()));
                }
                ToolOutput::Parts(parts) => {
                    next = next.checked_add(1).ok_or(CodecError::Limit)?;
                    parts.insert(
                        0,
                        Part::new(
                            PartId::scoped(LocalScope::DERIVED, next),
                            InputPart::Text(
                                Text::new(prefix, "tool error status", 64)
                                    .map_err(|_| CodecError::Limit)?,
                            ),
                        ),
                    );
                }
            }
        }
    }
    let request = request.into_owned().with_items(items)?;
    Ok((Cow::Owned(request), true))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn messages(body: serde_json::Value) -> Request {
        crate::adapter::ClientAdapter::Messages
            .decode_client_request(body.to_string().as_bytes())
            .unwrap()
    }

    #[test]
    fn messages_request_encoding_preserves_saturated_omission_evidence() {
        use crate::protocol::{
            anthropic::messages::{Dialect, Profile, TargetContract},
            native::ReplayTarget,
            openai::{Dialect as OpenAiDialect, GenerationCodec},
        };
        let mut input = vec![json!({"role":"user","content":"go"})];
        input.extend((0..65).flat_map(|i| {
            let call = format!("call_{i}");
            [
                json!({"type":"function_call","call_id":call,"name":"lookup","arguments":"{}"}),
                json!({"type":"function_call_output","call_id":call,"output":"feedback"}),
            ]
        }));
        let mut source =
            GenerationCodec::new(GenerationApi::Responses, OpenAiDialect::Standard, None)
                .decode_request(
                    json!({"model":"m","max_output_tokens":64,"input":input})
                        .to_string()
                        .as_bytes(),
                )
                .unwrap();
        let mut items = source.task.semantic.items().to_vec();
        for (_, item) in &mut items {
            if let Item::ToolResult(result) = item {
                result.execution = Some(ToolExecution::Unknown);
            }
        }
        source.task.semantic = source.task.semantic.with_items(items).unwrap();
        let original = source.clone();
        let contract = TargetContract::new(Profile::AdaptiveTextTools, Dialect::Standard);
        let target = ReplayTarget::new("synthetic", "m").unwrap();
        let encoded = contract.encode_request(&source, &target).unwrap();
        assert_eq!(
            encoded.evidence.rule_names(),
            ["messages.omit-tool-execution.v1"]
        );
        assert!(encoded.evidence.omitted_entries > 0);
        assert_eq!(
            encoded.evidence.len() as u64 + encoded.evidence.omitted_entries,
            65
        );
        assert_eq!(encoded.value["messages"].as_array().unwrap().len(), 131);
        assert_eq!(source, original);
    }

    #[test]
    fn reasoning_presentation_admission_and_losses_follow_candidate_rows() {
        use crate::protocol::openai::{Dialect, GenerationCodec};
        for api in [GenerationApi::Chat, GenerationApi::Responses] {
            let codec = GenerationCodec::new(api, Dialect::Standard, None);
            let source = GenerationCodec::new(GenerationApi::Responses, Dialect::Standard, None)
                .decode_request(br#"{"model":"m","input":"hello","reasoning":{"summary":"auto"}}"#)
                .unwrap();
            let original = source.clone();
            let result = codec
                .encode_request_with_evidence(&source, "m", &OpenAiContract::full())
                .unwrap();
            assert!(
                !result
                    .evidence
                    .contains_rule(super::super::disposition::OMIT_REASONING_SUMMARY.id)
            );
            assert_eq!(source, original);
            for display in ["summarized", "omitted"] {
                let source = messages(json!({"model":"m","max_tokens":64,
                    "thinking":{"type":"adaptive","display":display},
                    "messages":[{"role":"user","content":"hello"}]}));
                let original = source.clone();
                let contract = OpenAiContract::full();
                let table = Target::OpenAi(api, &contract).table(&source.task.semantic, "m");
                let result = codec.encode_request_with_evidence(&source, "m", &contract);
                if display == "omitted" {
                    assert!(matches!(
                        table.get(Feature::ReasoningDisplay),
                        Disposition::Reject(_)
                    ));
                    assert!(matches!(
                        result.unwrap_err().cause,
                        crate::protocol::ConversionCause::Representation(
                            crate::protocol::conversion::generation::RepresentationError::Admission(
                                Feature::ReasoningDisplay
                            )
                        )
                    ));
                } else {
                    let result = result.unwrap();
                    if api == GenerationApi::Chat {
                        let Disposition::Omit(rule) = table.get(Feature::ReasoningDisplay) else {
                            panic!("Chat must omit uncarried presentation");
                        };
                        assert_eq!(rule.id, "generation.omit-reasoning-summary.v1");
                        assert!(
                            result
                                .evidence
                                .contains_rule("generation.omit-reasoning-summary.v1")
                        );
                        assert!(result.value.get("reasoning").is_none());
                    } else {
                        assert_eq!(table.get(Feature::ReasoningDisplay), Disposition::Native);
                        assert_eq!(result.value["reasoning"]["summary"], "auto");
                    }
                }
                assert_eq!(source, original);
            }
        }
    }

    #[test]
    fn openai_effort_projection_preserves_thinking_off_and_removes_foreign_allocation_mode() {
        for disabled in [false, true] {
            let source = messages(json!({"model":"m","max_tokens":64,
                "thinking":{"type":if disabled { "disabled" } else { "adaptive" }},
                "output_config":{"effort":"high"},"messages":[{"role":"user","content":"hello"}]}));
            let before = source.clone();
            for api in [GenerationApi::Chat, GenerationApi::Responses] {
                let codec = crate::protocol::openai::GenerationCodec::new(
                    api,
                    crate::protocol::openai::Dialect::Standard,
                    None,
                );
                let output = codec
                    .encode_request_with_evidence(&source, "m", &OpenAiContract::full())
                    .unwrap();
                let effort = if api == GenerationApi::Chat {
                    &output.value["reasoning_effort"]
                } else {
                    &output.value["reasoning"]["effort"]
                };
                assert_eq!(effort, if disabled { "none" } else { "high" });
                assert!(output.value["reasoning"].get("mode").is_none());
                assert_eq!(source, before);
            }
        }
    }

    #[test]
    fn google_presentation_uses_native_carriers_and_resolves_conditional_tool_defaults() {
        let target = crate::protocol::native::ReplayTarget::new("synthetic", "m").unwrap();
        for display in ["summarized", "omitted"] {
            let source = messages(json!({"model":"m","max_tokens":64,
                "thinking":{"type":"adaptive","display":display},
                "messages":[{"role":"user","content":"hello"}]}));
            let before = source.clone();
            let gc = crate::protocol::google::generate_content::TargetContract::default()
                .encode_request(&source, &target)
                .unwrap();
            assert_eq!(
                gc.value["generationConfig"]["thinkingConfig"].get("includeThoughts"),
                (display == "summarized").then_some(&serde_json::Value::Bool(true))
            );
            let interactions = crate::protocol::google::interactions::TargetContract::default()
                .encode_request(&source, &target)
                .unwrap();
            assert_eq!(
                interactions.value["generation_config"]["thinking_summaries"],
                if display == "summarized" {
                    "auto"
                } else {
                    "none"
                }
            );
            assert_eq!(source, before);
        }
        let codec = crate::protocol::openai::GenerationCodec::new(
            GenerationApi::Responses,
            crate::protocol::openai::Dialect::Standard,
            None,
        );
        let source = codec.decode_request(br#"{"model":"m","input":"hello",
            "tools":[{"type":"function","name":"lookup","parameters":{"type":"object","properties":{},"additionalProperties":false}}]}"#).unwrap();
        let before = source.clone();
        for output in [
            crate::protocol::google::generate_content::TargetContract::default()
                .encode_request(&source, &target)
                .unwrap(),
            crate::protocol::google::interactions::TargetContract::default()
                .encode_request(&source, &target)
                .unwrap(),
        ] {
            assert!(
                output
                    .evidence
                    .contains_rule(super::super::disposition::CONDITIONAL_STRICTNESS.id)
            );
            assert!(!output.value.to_string().contains("\"strict\""));
        }
        assert_eq!(source, before);
    }

    #[test]
    fn adaptive_projection_preserves_explicit_effort_instead_of_quantizing_its_budget() {
        let source = messages(json!({"model":"m","max_tokens":10000,
            "thinking":{"type":"enabled","budget_tokens":8192},
            "output_config":{"effort":"low"},"messages":[{"role":"user","content":"hello"}]}));
        let mut contract = crate::protocol::anthropic::messages::TargetContract::new(
            crate::protocol::anthropic::messages::Profile::AdaptiveTextTools,
            crate::protocol::anthropic::messages::Dialect::Standard,
        );
        contract
            .dispositions
            .set_native(Feature::ReasoningBudget, false);
        contract.reasoning_policy = Some(crate::protocol::reasoning::ReasoningRequestPolicy {
            canonical_model: "m".into(),
            rule: "fixture.budget-to-effort",
            conversion: crate::protocol::reasoning::ReasoningConversion::BudgetToEffort {
                scope: EffortScope::Response,
                table: crate::protocol::reasoning::SHARED_BUDGETS,
            },
        });
        let original = source.clone();
        let target = crate::protocol::native::ReplayTarget::new("synthetic", "m").unwrap();
        let projection = contract.encode_request(&source, &target).unwrap();
        assert_eq!(projection.value["thinking"]["type"], "adaptive");
        assert!(projection.value["thinking"].get("budget_tokens").is_none());
        assert_eq!(projection.value["output_config"]["effort"], "low");
        assert!(
            projection
                .evidence
                .contains_rule(super::super::disposition::BUDGET_WITH_EFFORT.id)
        );
        assert_eq!(source, original);
    }

    #[test]
    fn explicit_effort_survives_budget_projection_and_model_clamping() {
        let source = messages(json!({"model":"m","max_tokens":10000,
            "thinking":{"type":"enabled","budget_tokens":8192},
            "output_config":{"effort":"low"},"messages":[{"role":"user","content":"hello"}]}));
        let original = source.clone();
        let mut contract = OpenAiContract::full();
        contract.model_limits.maximum_output_tokens = Some(4096);
        let codec = crate::protocol::openai::GenerationCodec::new(
            GenerationApi::Chat,
            crate::protocol::openai::Dialect::Standard,
            None,
        );
        let output = codec
            .encode_request_with_evidence(&source, "model", &contract)
            .unwrap();
        assert_eq!(output.value["reasoning_effort"], "low");
        assert_eq!(output.value["max_completion_tokens"], 4096);
        assert!(
            output
                .evidence
                .contains_rule(super::super::disposition::BUDGET_WITH_EFFORT.id)
        );
        assert!(
            output
                .evidence
                .contains_rule(super::super::disposition::CLAMP_OUTPUT.id)
        );
        assert_eq!(source, original);
    }

    #[test]
    fn carrierless_adaptive_controls_are_omitted_before_rendering() {
        let source = messages(
            json!({"model":"m","max_tokens":64,"thinking":{"type":"adaptive"},
            "messages":[{"role":"user","content":"q"}]}),
        );
        let before = source.clone();
        let mut contract = OpenAiContract::full();
        contract.dispositions = Dispositions::text_images();
        let codec = crate::protocol::openai::GenerationCodec::new(
            GenerationApi::Chat,
            crate::protocol::openai::Dialect::Standard,
            None,
        );
        let output = codec
            .encode_request_with_evidence(&source, "m", &contract)
            .unwrap();
        assert_eq!(output.value["messages"][0]["content"], "q");
        assert!(output.value.get("reasoning_effort").is_none());
        assert!(
            output
                .evidence
                .contains_rule("generation.omit-reasoning-activation.v1")
        );
        assert!(
            output
                .evidence
                .contains_rule("generation.omit-reasoning-mode.v1")
        );
        assert_eq!(source, before);
    }

    #[test]
    fn ignored_google_sampling_omits_top_k_with_the_other_sampling_hints() {
        let mut source = messages(
            json!({"model":"m","max_tokens":64,"messages":[{"role":"user","content":"q"}]}),
        );
        let mut settings = source.task.semantic.settings().clone();
        settings.controls.top_k = Some(7);
        source.task.semantic = source.task.semantic.with_settings(settings).unwrap();
        let before = source.clone();
        let target = crate::protocol::native::ReplayTarget::new("synthetic", "m").unwrap();
        let contract = crate::protocol::google::generate_content::TargetContract {
            sampling: crate::protocol::google::SamplingPolicy::Ignored,
            ..Default::default()
        };
        let output = contract.encode_request(&source, &target).unwrap();
        assert!(output.value["generationConfig"].get("topK").is_none());
        assert!(output.evidence.contains_rule("generation.omit-top-k.v1"));
        assert_eq!(source, before);
    }

    #[test]
    fn uncarried_cache_markers_leave_instruction_and_message_content_intact() {
        let source = messages(json!({"model":"m","max_tokens":64,
            "system":[{"type":"text","text":"rules","cache_control":{"type":"ephemeral","ttl":"5m"}}],
            "messages":[{"role":"user","content":[{"type":"text","text":"hello","cache_control":{"type":"ephemeral"}}]}]}));
        let original = source.clone();
        let codec = crate::protocol::openai::GenerationCodec::new(
            GenerationApi::Chat,
            crate::protocol::openai::Dialect::Standard,
            None,
        );
        let output = codec
            .encode_request_with_evidence(&source, "model", &OpenAiContract::full())
            .unwrap();
        assert_eq!(output.value["messages"][0]["content"], "rules");
        assert_eq!(output.value["messages"][1]["content"], "hello");
        assert!(!output.value.to_string().contains("cache_control"));
        assert!(
            output
                .evidence
                .contains_rule("generation.omit-explicit-cache.v1")
        );
        assert_eq!(source, original);
    }
}
