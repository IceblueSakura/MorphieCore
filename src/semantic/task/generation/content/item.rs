//! Ordered heterogeneous items shared by requests and responses.
use crate::semantic::task::generation::*;
use crate::semantic::value::Text;
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
    ConfigurationUpdate(ConfigurationUpdate),
    Program(Program),
    ProgramOutput(ProgramOutput),
    ProviderTool(ProviderToolObservation),
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
    /// Resource uses in order: message parts, then tool-result parts.
    pub fn resource_uses(&self) -> impl Iterator<Item = &ResourceUse> {
        let message: &[Part] = match self {
            Self::Message(m) => &m.parts,
            _ => &[],
        };
        let result: &[(PartId, ToolResultPart)] = match self {
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
        let result = result.iter().filter_map(|(_, p)| match p {
            ToolResultPart::Resource(r) => Some(r),
            ToolResultPart::Text(_) => None,
        });
        message.chain(result)
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
