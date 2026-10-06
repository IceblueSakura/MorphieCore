use super::{
    ContentPart, GenerationRequest, GenerationResponse, Item, OutputConstraint, ResourceKind,
};
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct GenerationRequirements {
    pub instruction_count: usize,
    pub message_count: usize,
    pub text_part_count: usize,
    pub resource_count: usize,
    pub image_inputs: usize,
    pub audio_inputs: usize,
    pub audio_output: bool,
    pub audio_history: bool,
    pub file_inputs: usize,
    pub tool_count: usize,
    pub custom_tools: bool,
    pub text_metadata: bool,
    pub top_p: bool,
    /// Requested output probabilities; reported history slots belong to text_metadata.
    pub logprobs: bool,
    pub truncation: bool,
    pub tool_choice: Option<super::ToolChoice>,
    pub parallel_tool_calls: Option<bool>,
    pub strict_function_tools: bool,
    pub tool_history: bool,
    pub structured_arguments: bool,
    pub structured_tool_results: bool,
    pub tool_execution_reports: bool,
    pub tool_result_images: usize,
    pub structured_output: bool,
    pub reasoning: bool,
    pub reasoning_items: usize,
    pub max_output_tokens: Option<u64>,
    pub temperature: bool,
}
impl GenerationRequirements {
    pub fn derive(r: &GenerationRequest) -> Self {
        let mut x = Self {
            audio_output: r.settings().audio.value().is_some(),
            tool_choice: r.tool_choice().cloned(),
            parallel_tool_calls: r.parallel_tool_calls(),
            instruction_count: usize::from(r.instructions().value().is_some()),
            custom_tools: r.tools().iter().flat_map(super::ToolDefinition::leaves).any(|t|matches!(t,super::ToolDefinition::Custom(_))),
            top_p: r.controls().top_p().is_some(),
            logprobs: r.controls().logprobs == crate::semantic::value::Presence::Value(true) || r.controls().top_logprobs.is_some(),
            truncation: r.controls().truncation.is_some(),
            strict_function_tools: r.tools().iter().flat_map(super::ToolDefinition::leaves).any(|t| matches!(t,super::ToolDefinition::Function(t) if matches!(t.strict,super::FunctionStrictness::Explicit(true)|super::FunctionStrictness::Omitted(super::StrictDefault::NormalizeSchema)))),
            max_output_tokens: r.controls().max_output_tokens,
            temperature: r.controls().temperature().is_some(),
            tool_count: r.tools().iter().map(|t| t.leaves().len()).sum(),
            structured_output: !matches!(r.output(), OutputConstraint::Text),
            reasoning: r.reasoning().presence() == super::ReasoningPresence::Present
                || r.reasoning().encrypted_output(),
            ..Self::default()
        };
        for (_, i) in r.items() {
            match i {
                Item::Instruction(_) => x.instruction_count += 1,
                Item::Message(m) => {
                    // A tool-only Chat owner is grouping, not an extra semantic message.
                    x.message_count += usize::from(!m.parts.is_empty());
                    for p in &m.parts {
                        match &p.content {
                            ContentPart::Text(t) => {
                                x.text_part_count += 1;
                                x.text_metadata |= !t.is_plain();
                            }
                            ContentPart::Refusal(t) => {
                                x.text_metadata |= !t.logprobs().is_absent();
                            }
                            ContentPart::Audio(_) | ContentPart::AudioReference(_) => {
                                x.audio_history = true
                            }
                            ContentPart::Resource(resource) => {
                                x.resource_count += 1;
                                match resource.kind() {
                                    ResourceKind::Image => x.image_inputs += 1,
                                    ResourceKind::Audio => x.audio_inputs += 1,
                                    ResourceKind::File => x.file_inputs += 1,
                                }
                            }
                        }
                    }
                }
                Item::ToolCall(call) => {
                    x.tool_history = true;
                    x.structured_arguments |=
                        !matches!(call.arguments, super::ToolArguments::Raw(_));
                }
                Item::ToolResult(result) | Item::CustomResult(result) => {
                    x.tool_history = true;
                    x.custom_tools |= matches!(i, Item::CustomResult(_));
                    x.tool_execution_reports |= result.execution.is_some();
                    match &result.output {
                        super::ToolOutput::Structured(_) => x.structured_tool_results = true,
                        super::ToolOutput::Parts(parts) => {
                            for (_, part) in parts {
                                if matches!(part, super::ToolResultPart::Resource(resource) if resource.kind() == ResourceKind::Image)
                                {
                                    x.tool_result_images += 1;
                                    x.resource_count += 1;
                                }
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
                Item::ConfigurationUpdate(_) => x.reasoning = true,
            }
        }
        x
    }
}

/// Actual output domains, independent of input controls or tool declarations.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct GenerationResponseRequirements {
    pub audio_output: bool,
    pub instructions: bool,
    pub tools: bool,
    pub custom_tools: bool,
    pub structured_arguments: bool,
    pub reasoning: bool,
    pub text_metadata: bool,
    pub logprobs: bool,
}
impl GenerationResponseRequirements {
    pub fn derive(response: &GenerationResponse) -> Self {
        let mut q = Self::default();
        for (_, item) in response.items() {
            match item {
                Item::Instruction(_) => q.instructions = true,
                Item::ToolCall(call) => {
                    q.tools = true;
                    q.structured_arguments |=
                        !matches!(call.arguments, super::ToolArguments::Raw(_));
                }
                Item::CustomCall(_) => {
                    q.tools = true;
                    q.custom_tools = true;
                }
                Item::Program(_) | Item::ProgramOutput(_) => q.tools = true,
                Item::Reasoning(_) | Item::ConfigurationUpdate(_) => q.reasoning = true,
                Item::Message(message) => {
                    for part in &message.parts {
                        match &part.content {
                            ContentPart::Text(text) => {
                                q.text_metadata |= !text.is_plain();
                                q.logprobs |= !text.logprobs().is_absent();
                            }
                            ContentPart::Refusal(text) => {
                                q.text_metadata |= !text.logprobs().is_absent();
                                q.logprobs |= !text.logprobs().is_absent();
                            }
                            ContentPart::Resource(_) => {}
                            ContentPart::Audio(_) | ContentPart::AudioReference(_) => {
                                q.audio_output = true
                            }
                        }
                    }
                }
                // Complete response validation, not request admission, rejects these.
                Item::ToolResult(_) | Item::CustomResult(_) => {}
            }
        }
        q
    }
}
