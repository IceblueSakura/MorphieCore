//! Ordered heterogeneous items shared by requests and responses.
use crate::semantic::task::generation::*;
use crate::semantic::value::Text;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InstructionAuthority {
    System,
    Developer,
}
/// Input-only content of instructions and tool results: text or an inert resource.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InputPart {
    Text(Text),
    Resource(ResourceUse),
}
impl InputPart {
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Self::Text(text) => Some(text.as_str()),
            Self::Resource(_) => None,
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Instruction {
    pub authority: InstructionAuthority,
    pub parts: Vec<(PartId, InputPart)>,
    /// Non-complete lifecycle only. Omitted and explicit `completed` stay absent.
    pub status: Option<ItemLifecycle>,
}
impl Instruction {
    /// The text parts, or `None` when a resource part needs a media carrier.
    pub fn texts(&self) -> Option<Vec<(PartId, &str)>> {
        self.parts
            .iter()
            .map(|(id, part)| part.as_text().map(|t| (*id, t)))
            .collect()
    }
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
    Resource(ResourceUse),
    Audio(crate::semantic::task::generation::GeneratedAudio),
    AudioReference(crate::semantic::task::generation::AudioReference),
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
    /// Opaque compacted context at this exact position, never readable reasoning.
    Compaction(ReplayValue),
    ConfigurationUpdate(ConfigurationUpdate),
    Program(Program),
    ProgramOutput(ProgramOutput),
    ProviderTool(ProviderToolObservation),
}
/// Ordered activation/effort update. It does not patch request settings.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ConfigurationUpdate {
    pub activation: Option<ReasoningActivation>,
    pub effort: Option<EffortControl>,
}
impl ConfigurationUpdate {
    pub fn validate(&self) -> Result<(), GenerationError> {
        if self.activation == Some(ReasoningActivation::Disabled)
            && self
                .effort
                .is_some_and(|e| e.scope == EffortScope::Reasoning)
        {
            return Err(GenerationError::InvalidControl);
        }
        Ok(())
    }
}
impl Item {
    pub fn lifecycle(&self) -> Option<ItemLifecycle> {
        match self {
            Self::Message(m) => Some(m.status),
            Self::ToolCall(c) => Some(c.status),
            Self::Reasoning(r) => Some(r.status),
            Self::Instruction(i) => i.status,
            Self::ToolResult(r) | Self::CustomResult(r) => r.status,
            Self::ProgramOutput(o) => Some(o.status),
            _ => None,
        }
    }
    /// Resource uses of message, instruction and tool-result parts.
    pub fn resource_uses(&self) -> impl Iterator<Item = &ResourceUse> {
        let message: &[Part] = match self {
            Self::Message(m) => &m.parts,
            _ => &[],
        };
        let input: &[(PartId, InputPart)] = match self {
            Self::Instruction(i) => &i.parts,
            Self::ToolResult(r) | Self::CustomResult(r) => match &r.output {
                ToolOutput::Parts(parts) => parts,
                _ => &[],
            },
            _ => &[],
        };
        let message = message.iter().filter_map(|p| match &p.content {
            ContentPart::Resource(r) => Some(r),
            _ => None,
        });
        let input = input.iter().filter_map(|(_, p)| match p {
            InputPart::Resource(r) => Some(r),
            InputPart::Text(_) => None,
        });
        message.chain(input)
    }
    pub fn is_call(&self) -> bool {
        matches!(
            self,
            Self::ToolCall(_) | Self::CustomCall(_) | Self::Program(_)
        )
    }
    /// Includes Provider operations only for identity/derivation protection.
    pub(in crate::semantic::task::generation) fn is_operation(&self) -> bool {
        self.is_call()
            || matches!(self, Self::ProviderTool(observed)
                if matches!(observed.operation, ProviderOperation::Reported { .. }))
    }
}
