//! Carrier-specific admission and envelope preparation around semantic projection.
use morphiecore_lowering::request::{
    self as semantic_projection, apply_option, apply_presence, omit_option,
};
use std::borrow::Cow;

use crate::{
    CodecError, ConversionFailure, GenerationRequestEnvelope as Request,
    conversion::generation::{Conversion, ConversionBoundary, EvidenceDelta},
    lowering::{
        capabilities::{Presentation, TargetCapabilities},
        disposition::{Disposition, Dispositions},
    },
    openai::{GenerationApi, TargetContract as OpenAiContract},
    semantic::task::generation::*,
};

/// Borrow the existing target declaration rather than maintaining another capability object.
#[derive(Clone, Copy)]
pub enum Target<'a> {
    OpenAi(GenerationApi, &'a OpenAiContract),
    Messages(&'a crate::anthropic::messages::TargetContract),
    Interactions(&'a crate::google::interactions::TargetContract),
}
pub fn admit_source(source: &Request) -> Result<(), ConversionFailure> {
    source.check_grouping()?;
    source.task.semantic.validate().map_err(CodecError::from)?;
    source.context.validate().map_err(CodecError::from)?;
    source.delivery.validate().map_err(CodecError::from)?;
    // Candidate projections own target capabilities; source admission only
    // applies union-wide exclusions after validating grammar and bounds.
    Dispositions::full()
        .check(&source.task.semantic)
        .map_err(crate::conversion::generation::RepresentationError::Admission)?;
    Ok(())
}
impl<'a> Target<'a> {
    fn retains_configuration(self, source: &GenerationRequest) -> bool {
        let boundary = match self {
            Self::OpenAi(GenerationApi::Responses, c)
                if !c.adaptation.rules.hooks.bailian_requests && self.has_effort() =>
            {
                Some(ConfigurationBoundary::FollowingResponse)
            }
            Self::Messages(c) if c.dispositions.native(Feature::MessageEffort) => {
                Some(ConfigurationBoundary::NewInputTurn)
            }
            _ => None,
        };
        source.items().iter().all(|(_, item)| match item {
            Item::ConfigurationUpdate(update) => {
                Some(update.boundary) == boundary
                    && match self {
                        Self::Messages(_) => update.activation.is_none() && update.effort.is_some(),
                        Self::OpenAi(_, _) => match update.activation {
                            Some(ReasoningActivation::Enabled) => update.effort.is_some(),
                            Some(ReasoningActivation::Disabled) => update.effort.is_none(),
                            None => true,
                        },
                        _ => false,
                    }
            }
            _ => true,
        })
    }
    fn declared(self) -> &'a Dispositions {
        match self {
            Self::OpenAi(_, c) => &c.dispositions,
            Self::Messages(c) => &c.dispositions,
            Self::Interactions(c) => &c.dispositions,
        }
    }
    fn limits(self) -> &'a crate::model_limits::ModelLimits {
        match self {
            Self::OpenAi(_, c) => &c.model_limits,
            Self::Messages(c) => &c.model_limits,
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
            Self::Interactions(_) => &[Minimal, Low, Medium, High, XHigh],
            _ => &[Minimal, Low, Medium, High, XHigh, Max],
        }
    }
    fn has_effort(self) -> bool {
        match self {
            Self::OpenAi(_, c) => {
                c.dispositions.native(Feature::Reasoning)
                    && !c.adaptation.rules.request.reject_reasoning_controls
            }
            Self::Messages(c) => c.dispositions.native(Feature::ResponseEffort),
            Self::Interactions(c) => c.dispositions.native(Feature::Reasoning),
        }
    }
    fn has_budget(self) -> bool {
        match self {
            Self::OpenAi(GenerationApi::Chat, c) => c.has_numeric_budget(),
            Self::Messages(c) => c.dispositions.native(Feature::ReasoningBudget),
            _ => false,
        }
    }
    fn policy(self) -> Option<&'a crate::reasoning::ReasoningRequestPolicy> {
        match self {
            Self::OpenAi(_, c) => c.adaptation.reasoning_policy.as_ref(),
            Self::Messages(c) => c.reasoning_policy.as_ref(),
            Self::Interactions(c) => c.reasoning_policy.as_ref(),
        }
    }
    fn cache(self) -> crate::cache::CacheTargetPolicy {
        match self {
            Self::OpenAi(_, c) => c.cache,
            Self::Messages(c) => c.cache,
            Self::Interactions(c) => c.cache,
        }
    }
    fn capabilities(self) -> TargetCapabilities<'a> {
        use super::{disposition::*, tool_set::OrderedCarrier};
        let messages = match self {
            Self::Messages(c) => Some(c),
            _ => None,
        };
        let openai = match self {
            Self::OpenAi(api, c) => Some((api, c)),
            _ => None,
        };
        let rules = openai.map(|(_, c)| &c.adaptation.rules);
        let bailian = rules.is_some_and(|r| r.hooks.bailian_requests);
        let presentation = match self {
            Self::OpenAi(GenerationApi::Responses, c)
                if c.adaptation.rules.reasoning.qwen4_responses_reasoning =>
            {
                Presentation::ExplicitAuto {
                    unsupported: "Qwen4 Responses only supports explicit automatic reasoning summaries",
                }
            }
            Self::OpenAi(GenerationApi::Responses, c)
                if c.adaptation.rules.reasoning.deepseek_responses_reasoning =>
            {
                Presentation::Unavailable
            }
            Self::OpenAi(GenerationApi::Responses, _) if !bailian => Presentation::Native,
            Self::OpenAi(_, _) => Presentation::Absent,
            Self::Messages(_) => Presentation::Summarize {
                rule: MESSAGES_SUMMARY,
                only_when_allocated: true,
            },
            Self::Interactions(_) => Presentation::Summarize {
                rule: INTERACTIONS_SUMMARY,
                only_when_allocated: false,
            },
        };
        TargetCapabilities {
            limits: self.limits(),
            effort_scope: self.effort_scope(),
            effort_levels: self.effort_levels(),
            effort_mapping: openai.and_then(|(_, c)| c.adaptation.effort_mapping),
            reasoning_policy: self.policy(),
            effort: self.has_effort(),
            budget: self.has_budget(),
            adaptive_budget: messages.is_some(),
            presentation,
            reject_hidden_reasoning: openai.is_some(),
            omit_unavailable_allocation_mode: openai.is_some(),
            require_output_ceiling: messages.is_some(),
            default_output_tokens: messages.and_then(|c| c.default_output_tokens),
            automatic_cache_default: messages.and_then(|c| c.automatic_cache_default),
            named_tool_subsets: openai.is_some(),
            whole_inventory_choice: matches!(openai, Some((GenerationApi::Chat, c)) if c.adaptation.rules.hooks.deepseek_chat_requests),
            omit_explicit_text_format: bailian,
            require_tool_description: bailian,
            ordered_tools: match self {
                Self::OpenAi(GenerationApi::Responses, _) => OrderedCarrier::Batches,
                Self::Messages(c) if c.dispositions.native(Feature::ToolChanges) => {
                    OrderedCarrier::Changes
                }
                _ => OrderedCarrier::None,
            },
            filter_allowed_inventory: messages.is_some(),
            filter_singleton_inventory: matches!(openai, Some((GenerationApi::Responses, _)))
                && bailian,
            image_details: openai
                .filter(|(_, c)| !c.adaptation.rules.request.omit_image_detail)
                .map_or(&[], |(_, c)| c.images.details.as_slice()),
            original_image_detail: openai.is_some_and(|(api, c)| {
                api != GenerationApi::Chat || c.adaptation.rules.request.chat_original_image_detail
            }),
        }
    }
    fn table(self, source: &GenerationRequest) -> Dispositions {
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
                // Responses carries ordered declarations natively; Chat admits
                // them only through the selected active-set projection.
                table.set_native(ToolChanges, api == GenerationApi::Responses);
                if api == GenerationApi::Chat {
                    table.set(
                        ToolChanges,
                        Disposition::Approximate(super::disposition::TOOL_CHANGES),
                    );
                }
                let rules = &c.adaptation.rules;
                if rules.reasoning.thinking_controls
                    || rules.hooks.bailian_requests
                    || rules.reasoning.chat_template_thinking
                {
                    table.set(ReasoningActivation, Disposition::Native);
                }
                if self.has_effort()
                    && source.reasoning().activation.as_ref()
                        == Some(&crate::semantic::task::generation::ReasoningActivation::Disabled)
                {
                    table.set(ReasoningActivation, Disposition::Native);
                }
                if api == GenerationApi::Chat && rules.reasoning.chat_preserve_thinking {
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
                table.set_native(
                    TopK,
                    api == GenerationApi::Chat && rules.reasoning.thinking_controls,
                );
                c.sampling.apply(source, &mut table);
                table.set_native(
                    ToolBudget,
                    api == GenerationApi::Responses
                        && c.dispositions.native(ToolBudget)
                        && !rules.hooks.bailian_requests
                        && !rules.hooks.responses_siwc,
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
                if rules.hooks.responses_siwc {
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
                if rules.hooks.bailian_requests {
                    c.model_policy.apply(source, &mut table);
                    if api == GenerationApi::Responses
                        && controls.max_output_tokens.is_some_and(|n| n < 16)
                    {
                        omit(&mut table, MaxOutputTokens);
                    }
                    for feature in [Metadata, ServicePolicy, Verbosity] {
                        omit(&mut table, feature);
                    }
                    if controls
                        .temperature()
                        .is_some_and(|v| !(0.0..2.0).contains(&v))
                    {
                        omit(&mut table, Temperature);
                    }
                    if controls.top_p().is_some_and(|v| v <= 0.0) {
                        omit(&mut table, TopP);
                    }
                    if controls.top_k.is_some_and(|v| v > 100) {
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
                if api == GenerationApi::Chat || rules.hooks.bailian_requests {
                    table.set(
                        MessagePhase,
                        Disposition::Omit(super::disposition::OMIT_PHASE),
                    );
                    omit(&mut table, ConditionalToolStrictness);
                } else {
                    table.set(MessagePhase, Disposition::Native);
                }
                table.set_native(
                    RequestLifetimeCache,
                    api == GenerationApi::Responses
                        && !rules.hooks.bailian_requests
                        && !rules.hooks.responses_siwc,
                );
                omit(&mut table, ExplicitCache);
                omit(&mut table, AutomaticPrefixCache);
                if rules.request.parallel_tools_always_enabled {
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
                        crate::conversion::generation::Field::ToolExecution,
                        crate::conversion::generation::Change::Omit,
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
                omit(&mut table, ConditionalToolStrictness);
                table.set(
                    ToolChanges,
                    if c.dispositions.native(ToolChanges) {
                        Disposition::Native
                    } else if source.features().contains_key(&ToolExamples) {
                        Disposition::Reject(
                            "tool-history collapse cannot preserve example-dependent tool versions",
                        )
                    } else {
                        // Retained additions, the leading lifted prefix and the
                        // active-set collapse share the selected bounded owner.
                        Disposition::Approximate(super::disposition::TOOL_CHANGES)
                    },
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
        if matches!(self, Self::Interactions(_)) {
            table.set(
                ToolChanges,
                Disposition::Approximate(super::disposition::TOOL_CHANGES),
            );
        }
        self.capabilities()
            .presentation_rows(source.settings(), &mut table);
        table.set(
            Effort,
            if self.has_effort() {
                Disposition::Approximate(super::disposition::EFFORT)
            } else {
                super::disposition::unsupported(Effort)
            },
        );
        if matches!(self, Target::Interactions(_))
            && source.tool_choice() == Some(&crate::semantic::task::generation::ToolChoice::None)
        {
            // Google's native NONE also satisfies at-most-one without removing
            // any declaration or weakening a guarantee for enabled calls.
            table.set(SerialTools, Disposition::Equivalent);
        }
        if !matches!(self, Target::Messages(_)) || !table.native(EagerToolInput) {
            table.set(
                EagerToolInput,
                Disposition::Omit(super::disposition::OMIT_EAGER_INPUT),
            );
        }
        if matches!(self, Target::OpenAi(GenerationApi::Chat, contract)
            if contract.adaptation.rules.reasoning.thinking_controls)
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
        if let Target::OpenAi(api, contract) = self {
            table.set(SchemaName, Disposition::Native);
            if api == GenerationApi::Responses && !contract.adaptation.rules.hooks.bailian_requests
            {
                table.set(ReasoningContext, Disposition::Native);
            }
            if !contract.adaptation.rules.reasoning.thinking_controls
                && !contract.adaptation.rules.hooks.bailian_requests
                && !contract.adaptation.rules.reasoning.chat_template_thinking
                && source.reasoning().activation.as_ref()
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
        if (self.limits().reject_disabled_reasoning
            || matches!(self, Target::OpenAi(_, contract) if contract.adaptation.rules.request.reject_disabled_reasoning)
            || matches!(self, Target::Interactions(_)))
            && (source.reasoning().activation.as_ref() == Some(&crate::semantic::task::generation::ReasoningActivation::Disabled)
                || source.items().iter().any(|(_, item)| matches!(item, Item::ConfigurationUpdate(update)
                    if update.activation == Some(crate::semantic::task::generation::ReasoningActivation::Disabled))))
        {
            table.set(ReasoningActivation, Disposition::Reject("the selected always-thinking model cannot disable reasoning"));
        }
        table.set(
            Feature::MessageEffort,
            if self.retains_configuration(source) {
                Disposition::Native
            } else {
                Disposition::Approximate(super::disposition::CONFIGURATION)
            },
        );
        table
    }
    fn projected_rows(self, source: &GenerationRequest, streaming: bool, table: &mut Dispositions) {
        let Target::OpenAi(api, contract) = self else {
            return;
        };
        let settings = source.settings();
        if streaming
            && settings
                .audio
                .as_ref()
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
            && streaming
            && custom_output
            && source
                .tool_declarations()
                .flat_map(|(_, tool)| tool.leaves())
                .any(|tool| matches!(tool, ToolDeclaration::Custom(_)))
        {
            table.set(
                Feature::CustomTools,
                Disposition::Reject("Chat streaming has no selected custom-call output carrier"),
            );
        }
        if contract.adaptation.rules.request.reject_reasoning_controls
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
        if contract.adaptation.rules.hooks.responses_siwc
            && (source.tool_declarations().any(|(owner, tool)| {
                owner.is_none() && !matches!(tool, ToolDeclaration::Namespace(_))
                    || !tool.dispatch_inactive()
            }) || {
                // Inline `additional_tools` declarations enable later direct
                // calls by leaf name; root declarations stay namespace-qualified.
                // https://developers.openai.com/siwc/token-sharing-open-source/preview-limitations
                let mut inline: Vec<(ToolKind, &str)> = Vec::new();
                source.items().iter().any(|(_, item)| match item {
                    Item::ToolBatch(batch) => {
                        for tool in &batch.tools {
                            if let Some(kind) = tool.kind() {
                                inline.push((kind, tool.name().as_str()));
                            }
                        }
                        false
                    }
                    Item::ToolChange(ToolChange {
                        action: ToolChangeAction::AddDefinition(tool),
                        ..
                    }) => {
                        inline.push((ToolKind::Function, tool.name.as_str()));
                        false
                    }
                    Item::ToolCall(call) => {
                        call.context.async_call
                            || call.context.caller.is_some()
                            || call.context.namespace.is_none()
                                && !inline.contains(&(ToolKind::Function, call.name.as_str()))
                    }
                    Item::CustomCall(call) => {
                        call.context.async_call
                            || call.context.caller.is_some()
                            || call.context.namespace.is_none()
                                && !inline.contains(&(ToolKind::Custom, call.name.as_str()))
                    }
                    Item::Program(_) | Item::ProgramOutput(_) => true,
                    _ => false,
                })
            })
        {
            table.set(Feature::Tools, Disposition::Reject("the selected native tool contract requires inactive namespaces and namespace-qualified direct calls"));
        }
        if contract.adaptation.rules.request.auto_tool_choice_only
            && source
                .tool_choice()
                .is_some_and(|choice| !matches!(choice, ToolChoice::Auto))
        {
            table.set(
                Feature::ToolChoice,
                Disposition::Reject("this target only supports automatic tool selection"),
            );
        }
        if contract.adaptation.rules.hooks.deepseek_chat_requests && api == GenerationApi::Chat {
            if matches!(source.tool_choice(), Some(ToolChoice::Allowed { .. })) {
                table.set(
                    Feature::ToolChoice,
                    Disposition::Reject(
                        "DeepSeek Chat cannot carry an allowed-tool subset without changing declaration visibility",
                    ),
                );
            }
            if matches!(source.output(), OutputConstraint::JsonSchema { .. }) {
                table.set(
                    Feature::StructuredOutput,
                    Disposition::Reject(
                        "DeepSeek Chat cannot enforce the requested JSON Schema output constraint",
                    ),
                );
            }
            if settings.reasoning.activation != Some(ReasoningActivation::Disabled)
                && matches!(
                    source.tool_choice(),
                    Some(
                        ToolChoice::Required
                            | ToolChoice::Specific(_)
                            | ToolChoice::Custom(_)
                            | ToolChoice::Qualified(_)
                            | ToolChoice::Allowed { required: true, .. }
                    )
                )
            {
                table.set(
                    Feature::ToolChoice,
                    Disposition::Reject(
                        "DeepSeek Chat cannot enforce forced tool selection while thinking is enabled",
                    ),
                );
            }
        }
        if contract.adaptation.rules.hooks.bailian_requests {
            let policy = contract.model_policy;
            let disabled =
                settings.reasoning.activation.as_ref() == Some(&ReasoningActivation::Disabled);
            if policy.text_output_only && !matches!(source.output(), OutputConstraint::Text) {
                table.set(
                    Feature::StructuredOutput,
                    Disposition::Reject(
                        "the selected model cannot enforce the requested output constraint",
                    ),
                );
            }
            if !source.reasoning().context.is_absent()
                || api == GenerationApi::Responses && !source.reasoning().read_history.is_none()
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
            if source.tool_declarations().any(|(_, tool)| match tool {
                ToolDeclaration::Function(function) => {
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
                ) || policy.reject_required_tools
                    && matches!(choice, Some(ToolChoice::Required))
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
                    && (source.available_tool_count() != Ok(1)
                        || policy.forced_tools_unavailable(&settings.reasoning))
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
                        Item::ToolResult(result) if !result.output.is_text_only() => {
                            table.set(Feature::Tools, Disposition::Reject("the selected function-output carrier is text-only; omitting feedback would change the tool result"));
                        }
                        Item::Reasoning(reasoning)
                            if !crate::openai::dialect::bailian::responses_reasoning_carrier(
                                reasoning,
                                &contract.adaptation.rules,
                            ) =>
                        {
                            table.set(Feature::ReasoningHistory, Disposition::Reject("the selected history carrier cannot preserve the retained reasoning owner"));
                        }
                        Item::CustomCall(_)
                        | Item::CustomResult(_)
                        | Item::Program(_)
                        | Item::ProgramOutput(_) => {
                            table.set(Feature::Tools, Disposition::Reject("the selected tool grammar cannot preserve this executable owner"));
                        }
                        Item::ConfigurationUpdate(_) => {
                            for feature in [Feature::Effort, Feature::ReasoningActivation] {
                                table.set(feature, Disposition::Reject("the selected history grammar has no configuration-update carrier"));
                            }
                        }
                        Item::Compaction(_) => {
                            table.set(
                                Feature::Compaction,
                                Disposition::Reject(
                                    "the selected history grammar has no compaction-state carrier",
                                ),
                            );
                        }
                        _ => {}
                    }
                }
            }
        }
    }
    fn context_rows(self, source: &Request, table: &mut Dispositions) -> Result<(), CodecError> {
        use crate::semantic::context::{CacheOptions, CacheRetention};
        let cache = self.cache();
        table.set(
            Feature::SessionAffinity,
            match self {
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
        table.set(
            Feature::CacheOptions,
            if cache.options {
                Disposition::Native
            } else if source
                .context
                .cache
                .prompt_cache_options
                .as_ref()
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
                Target::OpenAi(api, _) => service
                    .is_none_or(|value| crate::openai::service::policy_wire(*value, api).is_ok()),
                Target::Messages(_) => service.is_none_or(|value| {
                    crate::anthropic::messages::service::policy_wire(*value).is_ok()
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

pub fn project_semantic<'a>(
    source: &'a GenerationRequest,
    target: Target<'_>,
) -> Result<Conversion<Cow<'a, GenerationRequest>>, ConversionFailure> {
    let mut table = target.table(source);
    let projected = project_semantic_with_table(source, source, target, &mut table)?;
    target.projected_rows(projected.value.as_ref(), false, &mut table);
    table
        .check(projected.value.as_ref())
        .map_err(crate::conversion::generation::RepresentationError::Admission)?;
    Ok(projected)
}

fn project_semantic_with_table<'a>(
    source: &'a GenerationRequest,
    original: &GenerationRequest,
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
    // Responses has no source message-container carrier. Validate and project
    // those relations before item conversions replace their members' identities.
    if matches!(target, Target::OpenAi(GenerationApi::Responses, _))
        && let Some(groups) = crate::openai::lowering::message_groups::project(
            source.items(),
            source.message_envelopes(),
            GenerationApi::Responses,
        )?
    {
        semantic = Cow::Owned(
            source
                .clone()
                .with_message_envelopes(groups.value)
                .map_err(CodecError::from)?,
        );
        evidence.extend(groups.evidence);
    }
    let capabilities = target.capabilities();
    if !target.retains_configuration(source) || !original.configuration_timing_matches(source) {
        semantic =
            semantic_projection::collapse_configuration(source, original, semantic, &mut evidence)?;
        *table = target.table(&semantic);
    }
    let controls =
        semantic_projection::project_controls(semantic, source, capabilities, table, evidence)?;
    let mut evidence = controls.evidence;
    let mut semantic = semantic_projection::project_tool_inventory(
        controls.value,
        capabilities,
        table,
        &mut evidence,
    )?;
    if matches!(target, Target::OpenAi(GenerationApi::Responses, contract)
        if contract.adaptation.rules.hooks.responses_siwc)
        && let Some(projected) =
            super::tool_set::inline_root_tools(&semantic).map_err(CodecError::from)?
    {
        semantic = Cow::Owned(projected);
        table.set(Feature::Tools, target.table(&semantic).get(Feature::Tools));
        evidence
            .push(super::disposition::INLINE_ROOT_TOOLS.effect(ConversionBoundary::RequestProject));
    }
    if matches!(target, Target::Messages(_)) {
        semantic = Cow::Owned(crate::native::group_client_history(&semantic, true)?);
    }
    semantic = semantic_projection::relax_tool_strictness(semantic, table, &mut evidence)?;
    if let Disposition::Omit(rule) = table.get(Feature::ConditionalToolStrictness)
        && !(matches!(target, Target::Interactions(_))
            && semantic.tool_choice() == Some(&ToolChoice::None))
    {
        let resolved = crate::openai::lowering::strictness::project(
            &semantic,
            |schema| match target {
                Target::Messages(_) => {
                    table.native(Feature::StrictTools)
                        && crate::anthropic::messages::strict_schema::admits(schema.root())
                }
                Target::OpenAi(_, _) => table.native(Feature::StrictTools),
                Target::Interactions(_) => {
                    table.native(Feature::StrictTools)
                        && crate::google::strict_tools::admits(schema)
                }
            },
            |request| match target {
                Target::Messages(_) => {
                    crate::anthropic::messages::strict_schema::admits_generation(request)
                }
                Target::Interactions(_) => crate::google::strict_tools::validated(request).is_ok(),
                _ => true,
            },
        )?;
        if let Some((copy, omitted)) = resolved {
            semantic = Cow::Owned(copy);
            if omitted {
                evidence.push(rule.effect(ConversionBoundary::RequestProject));
            }
        }
    }
    if let Target::OpenAi(profile, _) = target
        && let Some(groups) = crate::openai::lowering::message_groups::project(
            semantic.items(),
            semantic.message_envelopes(),
            profile,
        )?
    {
        semantic = Cow::Owned(
            semantic
                .into_owned()
                .with_message_envelopes(groups.value)
                .map_err(CodecError::from)?,
        );
        evidence.extend(groups.evidence);
    }
    if matches!(target, Target::OpenAi(_, _)) {
        semantic = with_schema_name(semantic)?.0;
    } else {
        let rule = match target {
            Target::Messages(_) => super::disposition::MESSAGES_SCHEMA_NAME,
            Target::Interactions(_) => super::disposition::INTERACTIONS_SCHEMA_NAME,
            Target::OpenAi(_, _) => unreachable!(),
        };
        table.set(Feature::SchemaName, Disposition::Omit(rule));
        let mut settings = semantic.settings().clone();
        if let Some(OutputConstraint::JsonSchema { name, .. }) = &mut settings.text.format
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
    let semantic = semantic_projection::project_tool_result_errors(semantic, table, &mut evidence)?;
    crate::anthropic::messages::check_signed_configuration_prefix(source, &semantic)?;
    controls
        .reasoning
        .record(source, &semantic, &mut evidence)?;
    Ok(Conversion {
        value: semantic,
        evidence,
    })
}

pub fn project<'a>(
    source: &'a Request,
    target: Target<'_>,
    native_target: Option<&crate::native::ReplayTarget>,
) -> Result<Conversion<Cow<'a, Request>>, ConversionFailure> {
    source.check_grouping()?;
    source.context.validate().map_err(CodecError::from)?;
    source.delivery.validate().map_err(CodecError::from)?;
    source.task.semantic.validate().map_err(CodecError::from)?;
    // Existing trust-qualified transfer precedes target field admission. A
    // foreign owner can disappear without making the retained history unsupported.
    let native_origin = native_target.map(|native| native.origin());
    let issuer = match target {
        Target::OpenAi(_, contract) => contract.replay_origin(),
        _ => Some(native_origin.as_ref().ok_or_else(CodecError::invalid)?),
    };
    let reasoning_issuer = match target {
        Target::OpenAi(_, contract) => contract.reasoning_origin(),
        _ => None,
    };
    let mut stripped =
        source.without_uncarried_reasoning(issuer, reasoning_issuer, |reasoning, following| {
            let carries = match target {
                Target::OpenAi(GenerationApi::Responses, contract) => {
                    if contract.adaptation.rules.hooks.bailian_requests {
                        crate::openai::dialect::bailian::responses_reasoning_carrier(
                            reasoning,
                            &contract.adaptation.rules,
                        )
                    } else {
                        reasoning
                            .replay
                            .as_ref()
                            .is_none_or(|value| value.format() == ReplayFormat::ResponsesEncrypted)
                    }
                }
                Target::OpenAi(GenerationApi::Chat, contract) => {
                    crate::openai::lowering::chat_reasoning_carrier(
                        reasoning,
                        following,
                        &contract.adaptation.rules,
                    ) && reasoning
                        .replay
                        .as_ref()
                        .is_none_or(|value| value.format() == ReplayFormat::ResponsesEncrypted)
                }
                Target::Messages(_) => reasoning.replay.as_ref().is_some_and(|value| {
                    reasoning.status == ItemLifecycle::Completed
                        && match value.format() {
                            ReplayFormat::AnthropicMessagesThinking => {
                                reasoning.parts.len() <= 1
                                    && reasoning.parts.iter().all(|(_, part)| {
                                        matches!(part, ReasoningContent::Summary(_))
                                    })
                            }
                            ReplayFormat::AnthropicMessagesRedactedThinking => {
                                reasoning.parts.is_empty()
                            }
                            _ => false,
                        }
                }),
                Target::Interactions(_) => reasoning.replay.is_some(),
            };
            Ok(!carries)
        })?;
    let mut transferred = stripped.is_some();
    // Citation syntax is protocol-owned. Keep carriable annotations; discard only
    // uncarriable historical annotations without changing their visible text.
    let input = stripped.as_ref().unwrap_or(source);
    let mut items = Cow::Borrowed(input.task.semantic.items());
    let mut citations_changed = false;
    for (index, (_, item)) in input.task.semantic.items().iter().enumerate() {
        if let Item::Message(message) = item {
            for (ordinal, part) in message.parts.iter().enumerate() {
                if let ContentPart::Text(text) = &part.content {
                    let annotations: Vec<_> = text
                        .annotations()
                        .iter()
                        .filter(|annotation| match target {
                            Target::OpenAi(api @ GenerationApi::Responses, _)
                                if message.role == MessageRole::Assistant =>
                            {
                                crate::openai::content::citations::check(
                                    annotation,
                                    input.task.semantic.resources(),
                                    api,
                                )
                                .is_ok()
                            }
                            _ => false,
                        })
                        .cloned()
                        .collect();
                    if annotations.len() != text.annotations().len() {
                        let projected = TextContent::new(
                            crate::semantic::value::Text::allowing_empty(
                                text.as_str(),
                                "history text",
                                MAX_TEXT_BYTES,
                            )
                            .map_err(|_| CodecError::Limit)?,
                            annotations,
                            text.logprobs().clone(),
                        )
                        .map_err(CodecError::from)?;
                        let Item::Message(message) = &mut items.to_mut()[index].1 else {
                            unreachable!("the copied owner is a message");
                        };
                        message.parts[ordinal].content = ContentPart::Text(projected);
                        citations_changed = true;
                    }
                }
            }
        }
    }
    if citations_changed {
        let items = items.into_owned();
        let input = stripped.get_or_insert_with(|| source.clone());
        input.task.semantic = input
            .task
            .semantic
            .clone()
            .with_items(items)
            .map_err(CodecError::from)?;
        transferred = true;
    }
    if let Some(native) = native_target {
        let formats: &[ReplayFormat] = match target {
            Target::Messages(_) => &[
                ReplayFormat::AnthropicMessagesThinking,
                ReplayFormat::AnthropicMessagesRedactedThinking,
            ],
            Target::Interactions(_) => &[
                ReplayFormat::GoogleInteractionsV1Thought,
                ReplayFormat::GoogleInteractionsV1FunctionCall,
            ],
            Target::OpenAi(_, _) => return Err(CodecError::invalid().into()),
        };
        let input = stripped.as_ref().unwrap_or(source);
        let (view, fidelity, changed) = crate::native::native_reasoning_history(
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
    let mut table = target.table(&input.task.semantic);
    let semantic = project_semantic_with_table(
        &input.task.semantic,
        &source.task.semantic,
        target,
        &mut table,
    )?;
    target.context_rows(source, &mut table)?;
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
                crate::conversion::generation::RepresentationError::Admission(
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
            Target::Interactions(_) => super::disposition::TRANSFER_INTERACTIONS_HISTORY,
        };
        evidence.push(rule.effect(ConversionBoundary::RequestProject));
    }
    let mut context = source.context.clone();
    if !matches!(target, Target::OpenAi(_, contract)
        if !contract.adaptation.rules.hooks.responses_siwc && !contract.adaptation.rules.hooks.bailian_requests)
    {
        crate::native::normalize_stateless_defaults(&mut context);
    }
    use crate::attribution::{self, Carrier};
    let carrier = match target {
        Target::OpenAi(_, contract)
            if table.native(Feature::Identity)
                && !contract.adaptation.rules.hooks.responses_siwc =>
        {
            Carrier::OpenAi
        }
        Target::Messages(_) => Carrier::Messages,
        _ => Carrier::None,
    };
    let attribution = attribution::project(source, carrier)?;
    context.identity = attribution.identity;
    for rule in attribution.losses {
        evidence.push(rule.effect(ConversionBoundary::RequestProject));
    }
    if context.max_tool_calls.is_some() {
        match table.get(Feature::ToolBudget) {
            Disposition::Reject(_) => {
                return Err(
                    crate::conversion::generation::RepresentationError::Admission(
                        Feature::ToolBudget,
                    )
                    .into(),
                );
            }
            Disposition::Equivalent => context.max_tool_calls = None,
            _ => omit_option(
                &mut context.max_tool_calls,
                Feature::ToolBudget,
                &table,
                &mut evidence,
            ),
        }
    }
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
            crate::conversion::generation::RepresentationError::Admission(Feature::CacheRetention)
                .into(),
        );
    }
    omit_option(
        &mut context.cache.prompt_cache_retention,
        Feature::CacheRetention,
        &table,
        &mut evidence,
    );
    apply_option(
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
        projected.delivery.streaming(),
        &mut table,
    );
    table
        .check(&projected.task.semantic)
        .map_err(crate::conversion::generation::RepresentationError::Admission)?;
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
    use crate::semantic::{task::generation::OutputConstraint, value::Text};
    if !matches!(
        request.settings().text.format,
        Some(OutputConstraint::JsonSchema { name: None, .. })
    ) {
        return Ok((request, false));
    }
    let mut settings = request.settings().clone();
    let Some(OutputConstraint::JsonSchema {
        name: name @ None, ..
    }) = &mut settings.text.format
    else {
        unreachable!("checked schema name");
    };
    *name = Some(Text::new("response", "schema name", 64).expect("static name"));
    let request = request.into_owned().with_settings(settings)?;
    Ok((Cow::Owned(request), true))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn conditional_strictness_projection_reports_its_declared_omission() {
        use crate::conversion::generation::{Change, Field};
        let codec = crate::openai::GenerationCodec::new(
            GenerationApi::Responses,
            crate::openai::Dialect::Standard,
            None,
        );
        let source = codec
            .decode_request(
                br#"{"model":"m","input":"hello","tools":[
                    {"type":"function","name":"lookup","parameters":{"type":"object"}}]}"#,
            )
            .unwrap();
        let source = &source.task.semantic;
        let contract = OpenAiContract::full();
        let target = Target::OpenAi(GenerationApi::Chat, &contract);
        let disposition = target.table(source).get(Feature::ConditionalToolStrictness);
        let Disposition::Omit(rule) = disposition else {
            panic!("conditional strictness should be an omission, got {disposition:?}");
        };
        let output = project_semantic(source, target).unwrap();
        let effect = output
            .evidence
            .iter()
            .find(|effect| effect.rule() == "openai.omit-conditional-tool-strictness.v1")
            .expect("omission receipt");
        assert_eq!(effect.change(), Change::Omit);
        assert_eq!(effect.semantic_field(), Field::Tools);
        assert_eq!(effect, &rule.effect(ConversionBoundary::RequestProject));
    }

    fn messages(body: serde_json::Value) -> Request {
        crate::adapter::ClientAdapter::Messages
            .decode_client_request(body.to_string().as_bytes())
            .unwrap()
    }

    #[test]
    fn visible_history_transfer_preserves_text_and_tool_links_without_forging_thinking() {
        use crate::{
            anthropic::messages::{Dialect as MessagesDialect, Profile, TargetContract},
            native::ReplayTarget,
            openai::{Dialect, GenerationCodec},
        };
        let codec = GenerationCodec::new(GenerationApi::Responses, Dialect::Standard, None);
        let source = codec.decode_client_request(json!({"model":"m","max_output_tokens":64,"input":[
            {"role":"user","content":"go"},
            {"type":"reasoning","id":"rs_1","summary":[{"type":"summary_text","text":"private plan"}]},
            {"type":"message","id":"msg_1","status":"completed","role":"assistant","content":[{"type":"output_text","text":"visible",
                "annotations":[{"type":"url_citation","start_index":0,"end_index":7,
                    "url":"https://example.invalid/source","title":"Source"}]}]},
            {"type":"function_call","call_id":"call_1","name":"lookup","arguments":"{}"},
            {"type":"function_call_output","call_id":"call_1","output":"feedback"}
        ]}).to_string().as_bytes()).unwrap();
        let before = source.clone();
        let chat = GenerationCodec::new(GenerationApi::Chat, Dialect::Standard, None)
            .encode_request_with_evidence(&source, "m", &OpenAiContract::full())
            .unwrap();
        assert!(
            chat.evidence
                .contains_rule("chat.transfer-visible-history.v1")
        );
        assert_eq!(chat.value["messages"][1]["content"], "visible");
        assert_eq!(chat.value["messages"][1]["tool_calls"][0]["id"], "call_1");
        assert_eq!(chat.value["messages"][2]["tool_call_id"], "call_1");
        assert!(!chat.value.to_string().contains("private plan"));
        let messages = TargetContract::new(Profile::AdaptiveTextTools, MessagesDialect::Standard)
            .encode_request(&source, &ReplayTarget::new("synthetic", "m").unwrap())
            .unwrap();
        assert!(
            messages
                .evidence
                .contains_rule("messages.transfer-visible-history.v1")
        );
        assert_eq!(
            messages.value["messages"][1]["content"],
            json!([
                {"type":"text","text":"visible"},
                {"type":"tool_use","id":"call_1","name":"lookup","input":{}}
            ])
        );
        assert_eq!(
            messages.value["messages"][2]["content"][0]["tool_use_id"],
            "call_1"
        );
        assert!(!messages.value.to_string().contains("private plan"));
        assert!(!messages.value.to_string().contains("example.invalid"));
        assert_eq!(source, before);
        let mut intent = source.clone();
        let mut settings = intent.task.semantic.settings().clone();
        settings.reasoning.read_history = Some(true);
        settings.reasoning.presence = ReasoningPresence::Present;
        intent.task.semantic = intent.task.semantic.with_settings(settings).unwrap();
        assert!(
            TargetContract::new(Profile::AdaptiveTextTools, MessagesDialect::Standard)
                .encode_request(&intent, &ReplayTarget::new("synthetic", "m").unwrap())
                .is_err()
        );
    }

    #[test]
    fn structured_chat_history_retains_unsigned_summary_when_a_carrier_exists() {
        use crate::openai::{Dialect, GenerationCodec};
        let source = GenerationCodec::new(GenerationApi::Responses, Dialect::Standard, None)
            .decode_client_request(br#"{"model":"m","input":[
                {"type":"reasoning","id":"rs_1","summary":[{"type":"summary_text","text":"retained summary"}]},
                {"type":"message","id":"msg_1","role":"assistant","status":"completed",
                    "content":[{"type":"output_text","text":"visible","annotations":[]}]}
            ]}"#).unwrap();
        let before = source.clone();
        let target = GenerationCodec::new(GenerationApi::Chat, Dialect::OpenRouter, None);
        let result = target
            .encode_request_with_evidence(&source, "m", &OpenAiContract::full())
            .unwrap();
        assert_eq!(
            result.value["messages"][0]["reasoning_details"][0]["type"],
            "reasoning.summary"
        );
        assert_eq!(
            result.value["messages"][0]["reasoning_details"][0]["summary"],
            "retained summary"
        );
        assert!(
            !result
                .evidence
                .contains_rule("chat.transfer-visible-history.v1")
        );
        assert_eq!(source, before);
    }

    #[test]
    fn reasoning_presentation_admission_and_losses_follow_candidate_rows() {
        use crate::openai::{Dialect, GenerationCodec};
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
                let table = Target::OpenAi(api, &contract).table(&source.task.semantic);
                let result = codec.encode_request_with_evidence(&source, "m", &contract);
                if display == "omitted" {
                    assert!(matches!(
                        table.get(Feature::ReasoningDisplay),
                        Disposition::Reject(_)
                    ));
                    assert!(matches!(
                        result.unwrap_err().cause,
                        crate::ConversionCause::Representation(
                            crate::conversion::generation::RepresentationError::Admission(
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
                let codec = crate::openai::GenerationCodec::new(
                    api,
                    crate::openai::Dialect::Standard,
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
        let target = crate::native::ReplayTarget::new("synthetic", "m").unwrap();
        for display in ["summarized", "omitted"] {
            let source = messages(json!({"model":"m","max_tokens":64,
                "thinking":{"type":"adaptive","display":display},
                "messages":[{"role":"user","content":"hello"}]}));
            let before = source.clone();
            let interactions = crate::google::interactions::TargetContract::default()
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
        let codec = crate::openai::GenerationCodec::new(
            GenerationApi::Responses,
            crate::openai::Dialect::Standard,
            None,
        );
        let source = codec.decode_request(br#"{"model":"m","input":"hello",
            "tools":[{"type":"function","name":"lookup","parameters":{"type":"object","properties":{},"additionalProperties":false}}]}"#).unwrap();
        let before = source.clone();
        let output = crate::google::interactions::TargetContract::default()
            .encode_request(&source, &target)
            .unwrap();
        assert!(
            !output
                .evidence
                .contains_rule(super::super::disposition::CONDITIONAL_STRICTNESS.id)
        );
        assert!(!output.value.to_string().contains("\"strict\""));
        assert_eq!(
            output.value["generation_config"]["tool_choice"],
            "validated"
        );
        assert_eq!(source, before);
    }

    #[test]
    fn adaptive_projection_preserves_explicit_effort_instead_of_quantizing_its_budget() {
        let source = messages(json!({"model":"m","max_tokens":10000,
            "thinking":{"type":"enabled","budget_tokens":8192},
            "output_config":{"effort":"low"},"messages":[{"role":"user","content":"hello"}]}));
        let mut contract = crate::anthropic::messages::TargetContract::new(
            crate::anthropic::messages::Profile::AdaptiveTextTools,
            crate::anthropic::messages::Dialect::Standard,
        );
        contract
            .dispositions
            .set_native(Feature::ReasoningBudget, false);
        contract.reasoning_policy = Some(crate::reasoning::ReasoningRequestPolicy {
            canonical_model: "m".into(),
            rule: "fixture.budget-to-effort",
            conversion: crate::reasoning::ReasoningConversion::BudgetToEffort {
                scope: EffortScope::Response,
                table: crate::reasoning::SHARED_BUDGETS,
            },
        });
        let original = source.clone();
        let target = crate::native::ReplayTarget::new("synthetic", "m").unwrap();
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
        let codec = crate::openai::GenerationCodec::new(
            GenerationApi::Chat,
            crate::openai::Dialect::Standard,
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
        let codec = crate::openai::GenerationCodec::new(
            GenerationApi::Chat,
            crate::openai::Dialect::Standard,
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
        let target = crate::native::ReplayTarget::new("synthetic", "m").unwrap();
        let contract = crate::google::interactions::TargetContract {
            sampling: crate::google::SamplingPolicy::Ignored,
            ..Default::default()
        };
        let output = contract.encode_request(&source, &target).unwrap();
        assert!(output.value["generation_config"].get("top_k").is_none());
        assert!(output.evidence.contains_rule("generation.omit-top-k.v1"));
        assert_eq!(source, before);
    }

    #[test]
    fn uncarried_cache_markers_leave_instruction_and_message_content_intact() {
        let source = messages(json!({"model":"m","max_tokens":64,
            "system":[{"type":"text","text":"rules","cache_control":{"type":"ephemeral","ttl":"5m"}}],
            "messages":[{"role":"user","content":[{"type":"text","text":"hello","cache_control":{"type":"ephemeral"}}]}]}));
        let original = source.clone();
        let codec = crate::openai::GenerationCodec::new(
            GenerationApi::Chat,
            crate::openai::Dialect::Standard,
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
