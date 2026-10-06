//! Validated Generation history and settings. Response echoes reuse settings, never raw request JSON.
use super::*;
use crate::semantic::value::{Presence, Text};
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ItemId(u64);
impl ItemId {
    pub const fn new(v: u64) -> Self {
        Self(v)
    }
    pub const fn get(self) -> u64 {
        self.0
    }
}
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct PartId(u64);
impl PartId {
    pub const fn new(v: u64) -> Self {
        Self(v)
    }
    pub const fn get(self) -> u64 {
        self.0
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InstructionAuthority {
    System,
    Developer,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Instruction {
    pub authority: InstructionAuthority,
    pub parts: Vec<(PartId, Text)>,
    /// Non-complete lifecycle only. Omitted and explicit `completed` stay absent.
    pub status: Option<ItemLifecycle>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MessageRole {
    User,
    Assistant,
}
/// Assistant phase label from the standard, independent of item status. Missing and
/// null both mean unlabeled; no default label is ever synthesized.
#[derive(Clone, Copy, Debug, Eq, PartialEq, strum::EnumString, strum::IntoStaticStr)]
#[strum(serialize_all = "snake_case")]
pub enum Phase {
    Commentary,
    FinalAnswer,
}
impl Phase {
    pub fn label(self) -> &'static str {
        self.into()
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ContentPart {
    Text(TextContent),
    Refusal(RefusalContent),
    Resource(Resource),
    Audio(super::GeneratedAudio),
    AudioReference(super::AudioReference),
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Part {
    pub id: PartId,
    pub content: ContentPart,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Message {
    pub role: MessageRole,
    pub parts: Vec<Part>,
    pub status: ItemLifecycle,
    pub phase: Option<Phase>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Item {
    Instruction(Instruction),
    Message(Message),
    ToolCall(ToolCall),
    ToolResult(ToolResult),
    CustomCall(CustomCall),
    CustomResult(ToolResult),
    Reasoning(ReasoningItem),
    ConfigurationUpdate(ConfigurationUpdate),
    Program(Program),
    ProgramOutput(ProgramOutput),
}
/// Ordered reasoning-effort update. It does not patch request settings.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConfigurationUpdate {
    pub effort: Option<ReasoningEffort>,
}
impl Item {
    pub fn lifecycle(&self) -> Option<ItemLifecycle> {
        match self {
            Self::Message(m) => Some(m.status),
            Self::ToolCall(c) => Some(c.status),
            Self::Reasoning(r) => Some(r.status),
            Self::Instruction(i) => i.status,
            Self::ProgramOutput(o) => Some(o.status),
            _ => None,
        }
    }
    pub fn is_call(&self) -> bool {
        matches!(
            self,
            Self::ToolCall(_) | Self::CustomCall(_) | Self::Program(_)
        )
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Truncation {
    Auto,
    Disabled,
}
#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct GenerationControls {
    pub max_output_tokens: Option<u64>,
    temperature_bits: Option<u64>,
    top_p_bits: Option<u64>,
    pub top_logprobs: Option<u8>,
    pub logprobs: Presence<bool>,
    pub truncation: Option<Truncation>,
}
impl GenerationControls {
    pub fn with_temperature(mut self, v: f64) -> Result<Self, GenerationError> {
        if !v.is_finite() {
            return Err(GenerationError::NonFiniteTemperature);
        }
        self.temperature_bits = Some(v.to_bits());
        Ok(self)
    }
    pub fn temperature(&self) -> Option<f64> {
        self.temperature_bits.map(f64::from_bits)
    }
    pub fn with_top_p(mut self, v: f64) -> Result<Self, GenerationError> {
        if !v.is_finite() || !(0.0..=1.0).contains(&v) {
            return Err(GenerationError::InvalidControl);
        }
        self.top_p_bits = Some(v.to_bits());
        Ok(self)
    }
    pub fn top_p(&self) -> Option<f64> {
        self.top_p_bits.map(f64::from_bits)
    }
}
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct GenerationSettings {
    pub instructions: Presence<Text>,
    pub controls: GenerationControls,
    pub tools: Option<Vec<ToolDefinition>>,
    pub tool_choice: Option<ToolChoice>,
    pub parallel_tool_calls: Option<bool>,
    pub text: TextOptions,
    pub reasoning: ReasoningRequest,
    pub output_modalities: Presence<Vec<super::OutputModality>>,
    pub audio: Presence<super::AudioOutputOptions>,
}
impl GenerationSettings {
    pub fn output(&self) -> &OutputConstraint {
        self.text.format.value().unwrap_or(&OutputConstraint::Text)
    }
    pub fn validate(&self) -> Result<usize, GenerationError> {
        if self.controls.max_output_tokens == Some(0)
            || self.controls.top_logprobs.is_some_and(|n| n > 20)
            || self
                .controls
                .temperature()
                .is_some_and(|v| !(0.0..=2.0).contains(&v))
        {
            return Err(GenerationError::InvalidControl);
        }
        // An absent container cannot hide explicit values or null children.
        if !self.text.presence
            && (!self.text.format.is_absent() || !self.text.verbosity.is_absent())
        {
            return Err(GenerationError::InvalidControl);
        }
        self.reasoning.validate()?;
        if let Some(modalities) = self.output_modalities.value()
            && (modalities.is_empty()
                || modalities.len() > 2
                || (modalities.len() == 2 && modalities[0] == modalities[1]))
        {
            return Err(GenerationError::InvalidControl);
        }
        let audio = self
            .output_modalities
            .value()
            .is_some_and(|v| v.contains(&super::OutputModality::Audio));
        if audio != self.audio.value().is_some() {
            return Err(GenerationError::InvalidControl);
        }
        if let Some(options) = self.audio.value() {
            options.validate()?;
        }
        let instructions = self.instructions.value().map_or(0, |t| t.as_str().len());
        if instructions > MAX_TEXT_BYTES {
            return Err(GenerationError::Limit);
        }
        let bytes = instructions
            + self
                .audio
                .value()
                .map_or(0, |v| v.voice.as_str().len() + v.format.label().len())
            + self
                .output_modalities
                .value()
                .map_or(0, |v| v.iter().map(|m| m.label().len()).sum::<usize>())
            + super::validate::tools(
                self.tools.as_deref().unwrap_or_default(),
                self.tool_choice.as_ref(),
            )?
            + super::validate::output(self.output())?;
        if bytes > MAX_TOTAL_BYTES {
            return Err(GenerationError::Limit);
        }
        Ok(bytes)
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GenerationRequest {
    items: Vec<(ItemId, Item)>,
    settings: GenerationSettings,
}
impl GenerationRequest {
    pub fn new(
        items: Vec<(ItemId, Item)>,
        controls: GenerationControls,
    ) -> Result<Self, GenerationError> {
        Self::from_settings(
            items,
            GenerationSettings {
                controls,
                ..Default::default()
            },
        )
    }
    pub fn from_settings(
        items: Vec<(ItemId, Item)>,
        settings: GenerationSettings,
    ) -> Result<Self, GenerationError> {
        let r = Self { items, settings };
        r.validate()?;
        Ok(r)
    }
    pub fn validate(&self) -> Result<(), GenerationError> {
        let settings = self.settings.validate()?;
        let items = if self.items.is_empty() && self.settings.instructions.value().is_some() {
            0
        } else {
            super::validate::items(&self.items, false)?
        };
        if items.saturating_add(settings) > MAX_TOTAL_BYTES {
            return Err(GenerationError::Limit);
        }
        Ok(())
    }
    pub fn items(&self) -> &[(ItemId, Item)] {
        &self.items
    }
    pub fn settings(&self) -> &GenerationSettings {
        &self.settings
    }
    pub fn controls(&self) -> &GenerationControls {
        &self.settings.controls
    }
    pub fn tools(&self) -> &[ToolDefinition] {
        self.settings.tools.as_deref().unwrap_or_default()
    }
    pub fn tools_present(&self) -> bool {
        self.settings.tools.is_some()
    }
    pub fn tool_choice(&self) -> Option<&ToolChoice> {
        self.settings.tool_choice.as_ref()
    }
    pub fn parallel_tool_calls(&self) -> Option<bool> {
        self.settings.parallel_tool_calls
    }
    pub fn output(&self) -> &OutputConstraint {
        self.settings.output()
    }
    pub fn text_options(&self) -> &TextOptions {
        &self.settings.text
    }
    pub fn instructions(&self) -> &Presence<Text> {
        &self.settings.instructions
    }
    pub fn reasoning(&self) -> &ReasoningRequest {
        &self.settings.reasoning
    }
    pub fn with_settings(mut self, settings: GenerationSettings) -> Result<Self, GenerationError> {
        self.settings = settings;
        self.validate()?;
        Ok(self)
    }
    pub fn with_tool_settings(
        mut self,
        tools: Option<Vec<ToolDefinition>>,
        choice: Option<ToolChoice>,
        parallel: Option<bool>,
    ) -> Result<Self, GenerationError> {
        self.settings.tools = tools;
        self.settings.tool_choice = choice;
        self.settings.parallel_tool_calls = parallel;
        self.validate()?;
        Ok(self)
    }
    pub fn with_tools(
        self,
        tools: Vec<ToolDefinition>,
        choice: ToolChoice,
    ) -> Result<Self, GenerationError> {
        let parallel = self.parallel_tool_calls();
        self.with_tool_settings(Some(tools), Some(choice), parallel)
    }
    pub fn with_output(mut self, output: OutputConstraint) -> Self {
        self.settings.text.presence = true;
        self.settings.text.format = Presence::Value(output);
        self
    }
    pub fn with_reasoning(mut self, reasoning: ReasoningRequest) -> Self {
        self.settings.reasoning = reasoning;
        self
    }
    pub fn with_items(mut self, items: Vec<(ItemId, Item)>) -> Result<Self, GenerationError> {
        self.items = items;
        self.validate()?;
        Ok(self)
    }
    pub fn retain_items(
        self,
        mut keep: impl FnMut(ItemId, &Item) -> bool,
    ) -> Result<Self, GenerationError> {
        let items = self
            .items
            .iter()
            .filter(|(id, i)| keep(*id, i))
            .cloned()
            .collect();
        self.with_items(items)
    }
}
#[derive(Clone, Debug, Eq, thiserror::Error, PartialEq)]
pub enum GenerationError {
    #[error("generation input must not be empty")]
    EmptyInput,
    #[error("duplicate item identity")]
    DuplicateItemId,
    #[error("user message must contain a part")]
    EmptyMessage,
    #[error("duplicate part identity")]
    DuplicatePartId,
    #[error("temperature must be finite")]
    NonFiniteTemperature,
    #[error("invalid generation control")]
    InvalidControl,
    #[error("history dependency is missing or changed")]
    InvalidDependency,
    #[error("tool call identity is duplicated")]
    DuplicateCall,
    #[error("tool result has no matching preceding call or duplicates a result")]
    InvalidToolResult,
    #[error("tool calls must remain contiguous with their assistant owner")]
    InvalidMessageGroup,
    #[error("invalid tool definition")]
    InvalidToolDefinition,
    #[error("invalid or unsupported schema")]
    InvalidSchema,
    #[error("tool choice refers to an unavailable tool")]
    InvalidToolChoice,
    #[error("invalid structured JSON value")]
    InvalidJsonValue,
    #[error("incomplete structured tool arguments cannot complete their owner")]
    InvalidArguments,
    #[error("semantic value exceeds limits")]
    Limit,
    #[error("invalid response semantics")]
    InvalidResponse,
    #[error("invalid media resource or placement")]
    InvalidResource,
    #[error("phase labels only apply to assistant messages")]
    PhaseInUserMessage,
    #[error("refusal requires assistant role")]
    RefusalInUserMessage,
    #[error("program output has no matching program, duplicates one, or lacks a terminal status")]
    InvalidProgramOutput,
}
