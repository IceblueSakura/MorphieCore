//! Typed event grammar and semantic stream failures.
use crate::semantic::task::generation::*;
use crate::semantic::value::Text;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StreamTerminal {
    Completed,
    Incomplete,
    Failed,
    Cancelled,
    Error,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ItemKind {
    Message {
        phase: Option<Phase>,
    },
    ToolCall {
        format: ArgumentFormat,
        call_id: Text,
        name: Text,
        context: CallContext,
    },
    CustomCall {
        call_id: Text,
        name: Text,
        context: CallContext,
    },
    ToolResult(ToolResult),
    CustomResult(ToolResult),
    Reasoning,
    Compaction,
    ConfigurationUpdate(ConfigurationUpdate),
    Program {
        call_id: Text,
        code: String,
        fingerprint: String,
    },
    ProgramOutput {
        call_id: Text,
        result: String,
    },
    /// An already reported value, not a fragmented Provider wire payload.
    ProviderTool(ProviderToolObservation),
    /// Header only; exactly one result part supplies the final output.
    ProviderResult {
        observation: ProviderToolObservation,
        format: ProviderResultFormat,
    },
}
impl ItemKind {
    pub fn call(&self) -> Option<(&Text, &Text)> {
        match self {
            Self::ToolCall { call_id, name, .. } | Self::CustomCall { call_id, name, .. } => {
                Some((call_id, name))
            }
            _ => None,
        }
    }
    /// Unique call-ID namespace for calls and programs; results alias their call.
    pub fn call_id(&self) -> Option<&Text> {
        match self {
            Self::ToolCall { call_id, .. }
            | Self::CustomCall { call_id, .. }
            | Self::Program { call_id, .. } => Some(call_id),
            _ => None,
        }
    }
    pub(super) fn alias_domain(&self) -> Option<&NativeAliasDomain> {
        match self {
            Self::ToolCall { context, .. } | Self::CustomCall { context, .. } => {
                context.alias_domain.as_ref()
            }
            _ => None,
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PartKind {
    Text,
    Audio,
    Refusal,
    Summary,
    ReasoningText,
    Arguments,
    StructuredArguments,
    CustomInput,
    ResultText,
    ResultJson,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StreamEvent {
    ResourceDeclared {
        id: ResourceId,
        resource: ResourceLocation,
    },
    Started,
    Queued,
    ItemStarted {
        item: ItemId,
        kind: ItemKind,
        replay: Option<ItemReplay>,
    },
    /// Replace an open result's reported value, not its association or execution facts.
    ToolResultOutput {
        item: ItemId,
        output: ToolOutput,
    },
    PartStarted {
        item: ItemId,
        part: PartId,
        kind: PartKind,
    },
    Delta {
        item: ItemId,
        part: PartId,
        fragment: String,
        logprobs: Vec<Logprob>,
    },
    AudioDelta {
        item: ItemId,
        part: PartId,
        update: AudioUpdate,
    },
    LogprobsSnapshot {
        item: ItemId,
        part: PartId,
        logprobs: Vec<Logprob>,
    },
    AnnotationAdded {
        item: ItemId,
        part: PartId,
        annotation: Annotation,
    },
    /// Final readable-part metadata; refusal permits probabilities but never annotations.
    TextMetadata {
        item: ItemId,
        part: PartId,
        annotations: Vec<Annotation>,
        logprobs: crate::semantic::value::Presence<Vec<Logprob>>,
    },
    ValueFinished {
        item: ItemId,
        part: PartId,
    },
    PartFinished {
        item: ItemId,
        part: PartId,
    },
    ItemFinished {
        item: ItemId,
        status: ItemLifecycle,
        replay: Option<ItemReplay>,
    },
    Usage(Usage),
    UsageReport(UsageReport),
    SafetyReport(SafetyReport),
    /// Complete container declaration after its members close, before response terminal.
    MessageEnvelope(MessageEnvelope),
    Progress(InteractionProgress),
    Terminal {
        terminal: StreamTerminal,
        details: TerminalDetails,
    },
}
#[derive(Clone, Debug, Eq, thiserror::Error, PartialEq)]
pub enum EventError {
    #[error("conflicting event identity")]
    Identity,
    #[error("invalid event lifecycle")]
    Lifecycle,
    #[error("event state limit exceeded")]
    Limit,
    #[error("stream failed: {0:?}")]
    TerminalFailure(StreamTerminal),
    #[error("EOF before terminal")]
    EofBeforeTerminal,
    #[error(transparent)]
    Semantic(#[from] GenerationError),
}
