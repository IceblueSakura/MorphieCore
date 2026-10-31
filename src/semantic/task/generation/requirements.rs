use crate::semantic::task::generation::{
    ContentPart, EffortScope, Feature, FunctionStrictness, GenerationRequest, GenerationResponse,
    Item, OutputConstraint, PrefixCacheBoundary, ResourceKind, ResourceLocation, ResourceUse,
    StrictDefault, ToolDefinition,
};
use crate::semantic::value::Presence;
use std::collections::BTreeMap;

fn mark(facts: &mut BTreeMap<Feature, usize>, feature: Feature, present: bool) {
    if present {
        facts.entry(feature).or_insert(1);
    }
}
fn count(facts: &mut BTreeMap<Feature, usize>, feature: Feature, amount: usize) {
    if amount != 0 {
        *facts.entry(feature).or_default() += amount;
    }
}
fn cache(facts: &mut BTreeMap<Feature, usize>, boundary: PrefixCacheBoundary) {
    mark(
        facts,
        match boundary {
            PrefixCacheBoundary::RequestLifetime => Feature::RequestLifetimeCache,
            PrefixCacheBoundary::Ttl(_) => Feature::ExplicitCache,
        },
        true,
    );
}
fn input(facts: &mut BTreeMap<Feature, usize>, resource: &ResourceUse, r: &GenerationRequest) {
    let resource = resource.media(r.resources()).expect("validated resource");
    count(
        facts,
        match resource.kind() {
            ResourceKind::Image => Feature::ImageInput,
            ResourceKind::Audio => Feature::AudioInput,
            ResourceKind::Video => Feature::VideoInput,
            ResourceKind::File => Feature::FileInput,
        },
        1,
    );
    if resource.kind() == ResourceKind::File {
        if let crate::semantic::task::generation::ResourceDescription::File(file) =
            resource.description
        {
            mark(facts, Feature::DocumentContext, !file.context.is_absent());
        }
        if let ResourceLocation::Inline { media_type, .. } = resource.location {
            let kind = match media_type
                .as_str()
                .split('/')
                .next()
                .unwrap_or_default()
                .to_ascii_lowercase()
                .as_str()
            {
                "image" => Some(Feature::ImageInput),
                "audio" => Some(Feature::AudioInput),
                "video" => Some(Feature::VideoInput),
                _ => None,
            };
            if let Some(kind) = kind {
                count(facts, kind, 1);
            }
        }
    }
}
impl GenerationRequest {
    /// Requested facts in the shared vocabulary, including owner multiplicity.
    /// Their target disposition is deliberately not a semantic-IR decision.
    pub fn features(&self) -> BTreeMap<Feature, usize> {
        use Feature::*;
        let r = self;
        let mut facts = BTreeMap::new();
        for (feature, present) in [
            (CachedContext, r.cached_context().is_some()),
            (Safety, !r.settings().safety.is_empty()),
            (Temperature, r.controls().temperature().is_some()),
            (TopP, r.controls().top_p().is_some()),
            (TopK, r.controls().top_k.is_some()),
            (StopSequences, !r.controls().stop_sequences.is_empty()),
            (PresencePenalty, r.controls().presence_penalty().is_some()),
            (FrequencyPenalty, r.controls().frequency_penalty().is_some()),
            (Seed, r.controls().seed.is_some()),
            (MaxOutputTokens, r.controls().max_output_tokens.is_some()),
            (
                AutomaticPrefixCache,
                r.settings().automatic_prefix_cache.is_some(),
            ),
            (AudioOutput, r.settings().audio.value().is_some()),
            (ToolChoice, r.tool_choice().is_some()),
            (ParallelTools, r.parallel_tool_calls() == Some(true)),
            (SerialTools, r.parallel_tool_calls() == Some(false)),
            (Verbosity, !r.text_options().verbosity.is_none()),
            (
                Logprobs,
                r.controls().logprobs == Some(true) || r.controls().top_logprobs.is_some(),
            ),
            (Truncation, r.controls().truncation.is_some()),
            (
                StructuredOutput,
                !matches!(r.output(), OutputConstraint::Text),
            ),
            (
                SchemaName,
                matches!(
                    r.output(),
                    OutputConstraint::JsonSchema { name: Some(_), .. }
                ),
            ),
            (ReasoningActivation, !r.reasoning().activation.is_absent()),
            (ReasoningContext, !r.reasoning().context.is_absent()),
            (
                ReasoningHistoryRead,
                !r.reasoning().read_history.is_absent(),
            ),
            (ReasoningMode, !r.reasoning().mode.is_absent()),
            (ReasoningBudget, r.reasoning().budget_tokens.is_some()),
            (ReplayMismatch, r.reasoning().replay_mismatch.is_some()),
            (
                ReasoningDisplay,
                !r.reasoning().presentation.visibility.is_absent(),
            ),
            (
                ReasoningSummary,
                !r.reasoning().presentation.summary.is_absent(),
            ),
            (EncryptedReasoning, r.reasoning().encrypted_output()),
            (Effort, !r.settings().effort.is_none()),
            (
                ResponseEffort,
                r.settings()
                    .effort
                    .as_ref()
                    .is_some_and(|e| e.scope == EffortScope::Response),
            ),
            (Prediction, r.settings().prediction.is_some()),
        ] {
            mark(&mut facts, feature, present);
        }
        if let Some(instructions) = r.instructions().value() {
            count(&mut facts, Instructions, 1);
            for part in &instructions.parts {
                if let Some(boundary) = part.cache {
                    cache(&mut facts, boundary);
                }
            }
        }
        for tool in r.tools().iter().flat_map(ToolDefinition::leaves) {
            count(&mut facts, Tools, 1);
            mark(
                &mut facts,
                CustomTools,
                matches!(tool, ToolDefinition::Custom(_)),
            );
            if let ToolDefinition::Function(tool) = tool {
                mark(
                    &mut facts,
                    StrictTools,
                    tool.strict == FunctionStrictness::Explicit(true),
                );
                mark(
                    &mut facts,
                    ConditionalToolStrictness,
                    tool.strict == FunctionStrictness::Omitted(StrictDefault::Conditional),
                );
                mark(&mut facts, ToolExamples, tool.input_examples.is_some());
                mark(&mut facts, EagerToolInput, tool.eager_input.is_some());
                mark(&mut facts, ExplicitCache, tool.cache.is_some());
            }
        }
        mark(
            &mut facts,
            Tools,
            r.tool_choice().is_some() || r.parallel_tool_calls().is_some(),
        );
        let mut conversation = false;
        for (_, item) in r.items() {
            for boundary in item.cache_boundaries() {
                cache(&mut facts, boundary);
            }
            mark(
                &mut facts,
                OrderedInstructions,
                conversation && matches!(item, Item::Instruction(_)),
            );
            conversation |= !matches!(item, Item::Instruction(_));
            match item {
                Item::Instruction(_) => count(&mut facts, Instructions, 1),
                Item::Message(message) => {
                    count(
                        &mut facts,
                        MessageCount,
                        usize::from(!message.parts.is_empty()),
                    );
                    mark(&mut facts, MessagePhase, message.phase.is_some());
                    for part in &message.parts {
                        match &part.content {
                            ContentPart::Text(text) => {
                                count(&mut facts, TextPartCount, 1);
                                mark(
                                    &mut facts,
                                    TextMetadata,
                                    !text.annotations().is_empty()
                                        || !text.logprobs().is_absent()
                                            && !matches!(text.logprobs(), Presence::Value(p) if p.is_empty()),
                                );
                            }
                            ContentPart::Refusal(text) => mark(
                                &mut facts,
                                TextMetadata,
                                !text.logprobs().is_absent()
                                    && !matches!(text.logprobs(), Presence::Value(p) if p.is_empty()),
                            ),
                            ContentPart::Audio(_) | ContentPart::AudioReference(_) => {
                                mark(&mut facts, AudioHistory, true)
                            }
                            ContentPart::Resource(resource) => {
                                count(&mut facts, ResourceCount, 1);
                                if message.role
                                    == crate::semantic::task::generation::MessageRole::User
                                {
                                    input(&mut facts, resource, r);
                                }
                            }
                        }
                    }
                }
                Item::ToolCall(call) => {
                    mark(&mut facts, Tools, true);
                    mark(
                        &mut facts,
                        StructuredArguments,
                        !matches!(
                            call.arguments,
                            crate::semantic::task::generation::ToolArguments::Raw(_)
                        ),
                    );
                }
                Item::ToolResult(result) | Item::CustomResult(result) => {
                    mark(&mut facts, Tools, true);
                    mark(
                        &mut facts,
                        CustomTools,
                        matches!(item, Item::CustomResult(_)),
                    );
                    mark(&mut facts, ToolExecutionReport, result.execution.is_some());
                    mark(&mut facts, ToolResultError, result.is_error.is_some());
                    match &result.output {
                        crate::semantic::task::generation::ToolOutput::Structured(_) => {
                            mark(&mut facts, StructuredToolResult, true)
                        }
                        crate::semantic::task::generation::ToolOutput::Parts(parts) => {
                            for part in parts {
                                if let crate::semantic::task::generation::InputPart::Resource(
                                    resource,
                                ) = &part.content
                                {
                                    count(&mut facts, ToolResultMedia, 1);
                                    count(&mut facts, ResourceCount, 1);
                                    input(&mut facts, resource, r);
                                }
                            }
                        }
                        _ => {}
                    }
                }
                Item::Program(_) | Item::ProgramOutput(_) => mark(&mut facts, Tools, true),
                Item::CustomCall(_) => {
                    mark(&mut facts, Tools, true);
                    mark(&mut facts, CustomTools, true);
                }
                Item::Reasoning(_) => {
                    mark(&mut facts, Reasoning, true);
                    count(&mut facts, ReasoningHistory, 1);
                }
                Item::ConfigurationUpdate(update) => {
                    mark(&mut facts, ReasoningActivation, update.activation.is_some());
                    mark(&mut facts, MessageEffort, update.effort.is_some());
                    mark(&mut facts, Effort, update.effort.is_some());
                    mark(
                        &mut facts,
                        ResponseEffort,
                        update
                            .effort
                            .is_some_and(|e| e.scope == EffortScope::Response),
                    );
                }
                Item::Compaction(_) => mark(&mut facts, Compaction, true),
                Item::ProviderTool(_) => mark(&mut facts, ProviderObservation, true),
                Item::ToolChange(change) => {
                    mark(&mut facts, ToolChanges, true);
                    if let crate::semantic::task::generation::ToolChangeAction::AddDefinition(
                        tool,
                    ) = &change.action
                    {
                        mark(
                            &mut facts,
                            StrictTools,
                            tool.strict == FunctionStrictness::Explicit(true),
                        );
                        mark(&mut facts, ToolExamples, tool.input_examples.is_some());
                        mark(&mut facts, EagerToolInput, tool.eager_input.is_some());
                        mark(
                            &mut facts,
                            ConditionalToolStrictness,
                            tool.strict == FunctionStrictness::Omitted(StrictDefault::Conditional),
                        );
                    }
                }
            }
        }
        facts
    }
}

/// Actual output domains, independent of input controls or tool declarations.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct GenerationResponseRequirements {
    pub safety_reports: bool,
    pub compaction: bool,
    pub provider_observations: bool,
    pub audio_output: bool,
    pub instructions: bool,
    pub tools: bool,
    pub custom_tools: bool,
    pub structured_arguments: bool,
    pub structured_tool_results: bool,
    pub tool_execution_reports: bool,
    pub tool_result_errors: bool,
    pub tool_result_media: bool,
    pub reasoning: bool,
    pub response_effort: bool,
    pub text_metadata: bool,
    pub logprobs: bool,
}
impl GenerationResponseRequirements {
    pub fn derive(response: &GenerationResponse) -> Self {
        let mut q = Self {
            safety_reports: !response.safety_reports().is_empty(),
            ..Self::default()
        };
        for (_, item) in response.items() {
            match item {
                Item::Instruction(_) => q.instructions = true,
                Item::ProviderTool(_) => q.provider_observations = true,
                Item::ToolChange(_) => q.tools = true,
                Item::ToolCall(call) => {
                    q.tools = true;
                    q.structured_arguments |= !matches!(
                        call.arguments,
                        crate::semantic::task::generation::ToolArguments::Raw(_)
                    );
                }
                Item::CustomCall(_) => {
                    q.tools = true;
                    q.custom_tools = true;
                }
                Item::Program(_) | Item::ProgramOutput(_) => q.tools = true,
                Item::Reasoning(_) => q.reasoning = true,
                Item::ConfigurationUpdate(update) => {
                    q.reasoning |= update.activation.is_some()
                        || update.effort.is_none()
                        || update.effort.is_some_and(|e| {
                            e.scope == crate::semantic::task::generation::EffortScope::Reasoning
                        });
                    q.response_effort |= update.effort.is_some_and(|e| {
                        e.scope == crate::semantic::task::generation::EffortScope::Response
                    });
                }
                Item::Compaction(_) => q.compaction = true,
                Item::Message(message) => {
                    for part in &message.parts {
                        match &part.content {
                            ContentPart::Text(text) => {
                                let probabilities = !text.logprobs().is_absent()
                                    && !matches!(text.logprobs(), Presence::Value(probs) if probs.is_empty());
                                q.text_metadata |= !text.annotations().is_empty() || probabilities;
                                q.logprobs |= probabilities;
                            }
                            ContentPart::Refusal(text) => {
                                let probabilities = !text.logprobs().is_absent()
                                    && !matches!(text.logprobs(), Presence::Value(probs) if probs.is_empty());
                                q.text_metadata |= probabilities;
                                q.logprobs |= probabilities;
                            }
                            ContentPart::Resource(_) => {}
                            ContentPart::Audio(_) | ContentPart::AudioReference(_) => {
                                q.audio_output = true
                            }
                        }
                    }
                }
                Item::ToolResult(result) | Item::CustomResult(result) => {
                    q.tools = true;
                    q.custom_tools |= matches!(item, Item::CustomResult(_));
                    q.structured_tool_results |= matches!(
                        result.output,
                        crate::semantic::task::generation::ToolOutput::Structured(_)
                    );
                    q.tool_execution_reports |= result.execution.is_some();
                    q.tool_result_errors |= result.is_error.is_some();
                    q.tool_result_media |= item.resource_uses().next().is_some();
                }
            }
        }
        q
    }
}
