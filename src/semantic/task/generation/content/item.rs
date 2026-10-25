//! Ordered heterogeneous items shared by requests and responses.
use crate::semantic::task::generation::*;
use crate::semantic::value::Text;
/// Input-only content of tool results: text or an inert resource.
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
/// Ordered application/developer instructions. Legacy and protocol-specific
/// system spelling belongs to codecs, not a second semantic authority tier.
pub struct Instruction {
    /// Model-visible participant label, independent of authority and tool identity.
    pub participant: Option<Text>,
    pub parts: Vec<Part<Text>>,
    /// Non-complete lifecycle only. Omitted and explicit `completed` stay absent.
    pub status: Option<ItemLifecycle>,
}
impl Instruction {
    pub fn texts(&self) -> impl Iterator<Item = (PartId, &str)> {
        self.parts
            .iter()
            .map(|part| (part.id, part.content.as_str()))
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
pub struct Part<T = ContentPart> {
    pub id: PartId,
    pub content: T,
    pub cache: Option<PrefixCacheBoundary>,
    /// Opaque continuation attached to this generated value, never to adjacent text.
    pub replay: Option<ReplayValue>,
}
impl<T> Part<T> {
    pub fn new(id: PartId, content: T) -> Self {
        Self {
            id,
            content,
            cache: None,
            replay: None,
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Message {
    pub role: MessageRole,
    /// Request-history participant label; not a generated-output observation.
    pub participant: Option<Text>,
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
    /// Ordered tool availability change; request tools stay the initial declaration.
    ToolChange(ToolChange),
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
/// Offers or withdraws a tool from this position onward.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToolChange {
    pub action: ToolChangeAction,
    pub cache: Option<PrefixCacheTtl>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ToolChangeAction {
    /// A tool declared in the request or defined by an earlier change.
    AddReference(Text),
    /// A definition by value, replacing an earlier one with the same name.
    AddDefinition(Box<FunctionTool>),
    Remove(Text),
}
impl ToolChange {
    pub fn name(&self) -> &Text {
        match &self.action {
            ToolChangeAction::AddReference(name) | ToolChangeAction::Remove(name) => name,
            ToolChangeAction::AddDefinition(tool) => &tool.name,
        }
    }
}
impl Item {
    pub fn participant(&self) -> Option<&Text> {
        match self {
            Self::Instruction(i) => i.participant.as_ref(),
            Self::Message(m) => m.participant.as_ref(),
            _ => None,
        }
    }
    /// All content-part identities owned by this item; operation IDs are separate.
    pub fn part_ids(&self) -> impl Iterator<Item = PartId> {
        let instructions: &[Part<Text>] = match self {
            Self::Instruction(i) => &i.parts,
            _ => &[],
        };
        let messages: &[Part] = match self {
            Self::Message(m) => &m.parts,
            _ => &[],
        };
        let results: &[Part<InputPart>] = match self {
            Self::ToolResult(r) | Self::CustomResult(r) => match &r.output {
                ToolOutput::Parts(parts) => parts,
                _ => &[],
            },
            _ => &[],
        };
        let reasoning: &[(PartId, ReasoningContent)] = match self {
            Self::Reasoning(r) => &r.parts,
            _ => &[],
        };
        instructions
            .iter()
            .map(|p| p.id)
            .chain(messages.iter().map(|p| p.id))
            .chain(results.iter().map(|p| p.id))
            .chain(reasoning.iter().map(|(id, _)| *id))
    }
    pub fn has_replay_attachment(&self) -> bool {
        match self {
            Self::Message(message) => message.parts.iter().any(|part| part.replay.is_some()),
            Self::ToolCall(call) => call.replay.is_some(),
            _ => false,
        }
    }
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
    /// Resource uses of message and tool-result parts.
    pub fn resource_uses(&self) -> impl Iterator<Item = &ResourceUse> {
        let message: &[Part] = match self {
            Self::Message(m) => &m.parts,
            _ => &[],
        };
        let input: &[Part<InputPart>] = match self {
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
        let input = input.iter().filter_map(|p| match &p.content {
            InputPart::Resource(r) => Some(r),
            InputPart::Text(_) => None,
        });
        message.chain(input)
    }
    pub fn cache_boundaries(&self) -> impl Iterator<Item = PrefixCacheBoundary> {
        let instructions: &[Part<Text>] = match self {
            Self::Instruction(i) => &i.parts,
            _ => &[],
        };
        let messages: &[Part] = match self {
            Self::Message(m) => &m.parts,
            _ => &[],
        };
        let results: &[Part<InputPart>] = match self {
            Self::ToolResult(r) | Self::CustomResult(r) => match &r.output {
                ToolOutput::Parts(parts) => parts,
                _ => &[],
            },
            _ => &[],
        };
        let outer = match self {
            Self::ToolCall(c) => [c.cache, None],
            Self::ToolChange(c) => [
                c.cache,
                match &c.action {
                    ToolChangeAction::AddDefinition(t) => t.cache,
                    _ => None,
                },
            ],
            Self::ToolResult(r) | Self::CustomResult(r) => [r.cache, None],
            _ => [None, None],
        };
        instructions
            .iter()
            .filter_map(|p| p.cache)
            .chain(messages.iter().filter_map(|p| p.cache))
            .chain(results.iter().filter_map(|p| p.cache))
            .chain(outer.into_iter().flatten().map(PrefixCacheBoundary::Ttl))
    }
    pub fn has_cache_breakpoints(&self) -> bool {
        self.cache_boundaries().next().is_some()
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
