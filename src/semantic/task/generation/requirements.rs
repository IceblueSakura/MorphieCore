use crate::semantic::task::generation::{
    ContentPart, EffortScope, FunctionStrictness, GenerationRequest, GenerationResponse, Item,
    OutputConstraint, PrefixCacheBoundary, ReasoningPresence, ResourceKind, ResourceLocation,
    ResourceUse, StrictDefault, ToolDefinition,
};
use crate::semantic::value::Presence;
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct GenerationRequirements {
    pub instruction_count: usize,
    /// An instruction item follows conversation content, as a mid-conversation system message.
    pub ordered_instructions: bool,
    pub message_count: usize,
    pub text_part_count: usize,
    pub resource_count: usize,
    pub image_inputs: usize,
    pub audio_inputs: usize,
    pub video_inputs: usize,
    pub audio_output: bool,
    pub audio_history: bool,
    pub file_inputs: usize,
    pub document_context: bool,
    pub tool_count: usize,
    pub custom_tools: bool,
    pub text_metadata: bool,
    pub top_p: bool,
    pub top_k: bool,
    pub stop_sequences: bool,
    pub presence_penalty: bool,
    pub frequency_penalty: bool,
    pub seed: bool,
    pub verbosity: bool,
    /// Requested output probabilities; reported history slots belong to text_metadata.
    pub logprobs: bool,
    pub truncation: bool,
    pub tool_choice: Option<crate::semantic::task::generation::ToolChoice>,
    pub parallel_tool_calls: Option<bool>,
    pub strict_function_tools: bool,
    pub tool_examples: bool,
    pub tool_changes: bool,
    pub normalization_default_tools: bool,
    /// Owner-supplied TTL support. Request-lifetime boundaries use codec admission.
    pub explicit_cache: bool,
    pub cached_context: bool,
    pub safety_settings: bool,
    pub automatic_prefix_cache: bool,
    pub tool_history: bool,
    pub provider_observations: bool,
    pub structured_arguments: bool,
    pub structured_tool_results: bool,
    pub tool_execution_reports: bool,
    pub tool_result_errors: bool,
    pub tool_result_media: usize,
    pub structured_output: bool,
    pub reasoning: bool,
    pub response_effort: bool,
    pub reasoning_items: usize,
    pub compaction: bool,
    pub max_output_tokens: Option<u64>,
    pub temperature: bool,
}
impl GenerationRequirements {
    fn input(&mut self, resource: &ResourceUse, r: &GenerationRequest) {
        let resource = resource.media(r.resources()).expect("validated resource");
        match resource.kind() {
            ResourceKind::Image => self.image_inputs += 1,
            ResourceKind::Audio => self.audio_inputs += 1,
            ResourceKind::Video => self.video_inputs += 1,
            ResourceKind::File => {
                self.file_inputs += 1;
                if let crate::semantic::task::generation::ResourceDescription::File(file) =
                    resource.description
                {
                    self.document_context |= !file.context.is_absent();
                }
                // A file the model must see or hear also needs that modality, e.g. an
                // audio data URL sent as a Responses `input_file`. Remote MIME is unknown.
                if let ResourceLocation::Inline { media_type, .. } = resource.location {
                    let family = media_type.as_str().split('/').next().unwrap_or_default();
                    match family.to_ascii_lowercase().as_str() {
                        "image" => self.image_inputs += 1,
                        "audio" => self.audio_inputs += 1,
                        "video" => self.video_inputs += 1,
                        _ => {}
                    }
                }
            }
        }
    }
    pub fn derive(r: &GenerationRequest) -> Self {
        let leaves = || r.tools().iter().flat_map(ToolDefinition::leaves);
        let functions = || {
            leaves().filter_map(|t| match t {
                ToolDefinition::Function(t) => Some(t),
                _ => None,
            })
        };
        let effort_scope = r.settings().effort.value().map(|e| e.scope);
        let mut x = Self {
            cached_context: r.cached_context().is_some(),
            safety_settings: !r.settings().safety.is_empty(),
            top_k: r.controls().top_k.is_some(),
            stop_sequences: !r.controls().stop_sequences.is_empty(),
            presence_penalty: r.controls().presence_penalty().is_some(),
            frequency_penalty: r.controls().frequency_penalty().is_some(),
            seed: r.controls().seed.is_some(),
            automatic_prefix_cache: r.settings().automatic_prefix_cache.is_some(),
            audio_output: r.settings().audio.value().is_some(),
            tool_choice: r.tool_choice().cloned(),
            parallel_tool_calls: r.parallel_tool_calls(),
            instruction_count: usize::from(r.instructions().value().is_some()),
            custom_tools: leaves().any(|t| matches!(t, ToolDefinition::Custom(_))),
            top_p: r.controls().top_p().is_some(),
            verbosity: !r.text_options().verbosity.is_absent(),
            logprobs: r.controls().logprobs == Presence::Value(true)
                || r.controls().top_logprobs.is_some(),
            truncation: r.controls().truncation.is_some(),
            strict_function_tools: functions()
                .any(|t| matches!(t.strict, FunctionStrictness::Explicit(true))),
            tool_examples: functions().any(|t| t.input_examples.is_some()),
            normalization_default_tools: functions()
                .any(|t| t.strict == FunctionStrictness::Omitted(StrictDefault::NormalizeSchema)),
            explicit_cache: functions().any(|t| t.cache.is_some())
                || r.instructions().value().is_some_and(|root| {
                    root.parts
                        .iter()
                        .any(|p| matches!(p.cache, Some(PrefixCacheBoundary::Ttl(_))))
                }),
            max_output_tokens: r.controls().max_output_tokens,
            temperature: r.controls().temperature().is_some(),
            tool_count: r.tools().iter().map(|t| t.leaves().len()).sum(),
            structured_output: !matches!(r.output(), OutputConstraint::Text),
            reasoning: r.reasoning().presence() == ReasoningPresence::Present
                || r.reasoning().encrypted_output()
                || effort_scope == Some(EffortScope::Reasoning),
            response_effort: effort_scope == Some(EffortScope::Response),
            ..Self::default()
        };
        let mut conversation = false;
        for (_, i) in r.items() {
            x.explicit_cache |= i
                .cache_boundaries()
                .any(|c| matches!(c, PrefixCacheBoundary::Ttl(_)));
            x.ordered_instructions |= conversation && matches!(i, Item::Instruction(_));
            conversation |= !matches!(i, Item::Instruction(_));
            match i {
                Item::Instruction(_) => {
                    x.instruction_count += 1;
                }
                Item::Message(m) => {
                    // An empty assistant message only groups calls; it adds no content.
                    x.message_count += usize::from(!m.parts.is_empty());
                    for p in &m.parts {
                        match &p.content {
                            ContentPart::Text(t) => {
                                x.text_part_count += 1;
                                x.text_metadata |= !t.annotations().is_empty()
                                    || !t.logprobs().is_absent()
                                        && !matches!(t.logprobs(), Presence::Value(probs) if probs.is_empty());
                            }
                            ContentPart::Refusal(t) => {
                                x.text_metadata |= !t.logprobs().is_absent()
                                    && !matches!(t.logprobs(), Presence::Value(probs) if probs.is_empty());
                            }
                            ContentPart::Audio(_) | ContentPart::AudioReference(_) => {
                                x.audio_history = true
                            }
                            ContentPart::Resource(resource) => {
                                x.resource_count += 1;
                                if m.role == crate::semantic::task::generation::MessageRole::User {
                                    x.input(resource, r);
                                }
                            }
                        }
                    }
                }
                Item::ToolCall(call) => {
                    x.tool_history = true;
                    x.structured_arguments |= !matches!(
                        call.arguments,
                        crate::semantic::task::generation::ToolArguments::Raw(_)
                    );
                }
                Item::ToolResult(result) | Item::CustomResult(result) => {
                    x.tool_history = true;
                    x.custom_tools |= matches!(i, Item::CustomResult(_));
                    x.tool_execution_reports |= result.execution.is_some();
                    x.tool_result_errors |= result.is_error.is_some();
                    match &result.output {
                        crate::semantic::task::generation::ToolOutput::Structured(_) => {
                            x.structured_tool_results = true
                        }
                        crate::semantic::task::generation::ToolOutput::Parts(parts) => {
                            for part in parts {
                                let crate::semantic::task::generation::InputPart::Resource(
                                    resource,
                                ) = &part.content
                                else {
                                    continue;
                                };
                                x.tool_result_media += 1;
                                x.resource_count += 1;
                                // Tool media also needs the model's input modality.
                                x.input(resource, r);
                            }
                        }
                        _ => {}
                    }
                }
                Item::Program(_) | Item::ProgramOutput(_) => x.tool_history = true,
                Item::CustomCall(_) => {
                    x.tool_history = true;
                    x.custom_tools = true;
                }
                Item::Reasoning(_) => {
                    x.reasoning_items += 1;
                    x.reasoning = true;
                }
                Item::ConfigurationUpdate(update) => {
                    x.reasoning |= update.activation.is_some()
                        || update.effort.is_none()
                        || update.effort.is_some_and(|e| {
                            e.scope == crate::semantic::task::generation::EffortScope::Reasoning
                        });
                    x.response_effort |= update.effort.is_some_and(|e| {
                        e.scope == crate::semantic::task::generation::EffortScope::Response
                    });
                }
                Item::Compaction(_) => x.compaction = true,
                Item::ProviderTool(_) => x.provider_observations = true,
                Item::ToolChange(change) => {
                    x.tool_changes = true;
                    if let crate::semantic::task::generation::ToolChangeAction::AddDefinition(t) =
                        &change.action
                    {
                        x.strict_function_tools |=
                            matches!(t.strict, FunctionStrictness::Explicit(true));
                        x.tool_examples |= t.input_examples.is_some();
                        x.normalization_default_tools |=
                            t.strict == FunctionStrictness::Omitted(StrictDefault::NormalizeSchema);
                    }
                }
            }
        }
        x
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
