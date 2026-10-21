//! Generation semantic IR.
mod content;
mod contract;
mod error;
mod history;
mod identity;
mod output;
mod request;
mod requirements;
mod resources;
mod response;
mod safety;
mod schema;
mod stream;
mod terminal_reports;
mod tools;
mod usage;
mod validate;
pub use content::audio::{
    AudioFormat, AudioOutputOptions, AudioReference, GeneratedAudio, MAX_AUDIO_DECODED_BYTES,
    OutputModality,
};
pub use content::citation::{
    Annotation, Citation, CitationKind, ClaimAnchor, SourceCoordinates, TextPoint, TextRange,
    TextUnit,
};
pub use content::item::{
    ConfigurationUpdate, ContentPart, InputPart, Instruction, InstructionAuthority, Item, Message,
    MessageRole, Part, Phase, ToolChange, ToolChangeAction,
};
pub use content::reasoning::{
    EffortControl, EffortLevel, EffortScope, ReasoningActivation, ReasoningContent,
    ReasoningContext, ReasoningDisplay, ReasoningItem, ReasoningMode, ReasoningPresence,
    ReasoningRequest, ReasoningSummary, ReplayMismatch,
};
pub use content::structured::StructuredValue;
pub use content::text::{
    Logprob, RefusalContent, TextContent, TopLogprob, compatible_logprobs, validate_logprobs,
};
pub use contract::{GenerationFeature, GenerationSemanticContract};
pub use error::GenerationError;
pub use history::continuation::{CallReference, Continuation};
pub use history::envelope::{MessageEnvelope, MessageEnvelopeRole};
pub use history::group::GroupId;
pub use history::progress::InteractionProgress;
pub use history::replay::{ItemReplay, ReplayFormat, ReplayValue};
pub use history::turn::{
    ContinuationError, ResponseContinuation, ResponseId, ResponseRelation, ResultReadiness, TurnId,
};
pub use identity::{ItemId, LocalScope, NativeAliasDomain, PartId};
pub use output::{OutputConstraint, TextOptions, Verbosity};
pub use request::{
    CachedContextReference, ClientManaged, ContextBuild, ContextChange, ContextEdit, ContextError,
    ContextStage, GenerationControls, GenerationRequest, GenerationSettings, SelectedHistory,
    Truncation,
};
pub use requirements::{GenerationRequirements, GenerationResponseRequirements};
pub use resources::table::{ResourceDependency, ResourceId, ResourceTable, ResourceUse};
pub use resources::{
    FileDescription, FileDetail, ImageDetail, ImageFormat, MAX_FILE_DECODED_BYTES,
    MAX_FILE_NAME_BYTES, MAX_IMAGE_DECODED_BYTES, MAX_RESOURCE_MEDIA_TYPE_BYTES,
    MAX_RESOURCE_URL_BYTES, MAX_TOTAL_FILE_DECODED_BYTES, Resource, ResourceDescription,
    ResourceKind, ResourceLocation, ResourceView,
};
pub use schema::SchemaDocument;
pub use terminal_reports::{RefusalCategory, RefusalDetails};
pub const MAX_TERMINAL_REPORT_BYTES: usize = 64 << 10;
pub use stream::audio::{AudioBuffer, AudioUpdate};
pub use stream::value::StreamPartValue;
pub use tools::provider::{
    ProviderAction, ProviderExecutionProgress, ProviderOperation, ProviderOperationReference,
    ProviderRequester, ProviderResultFormat, ProviderToolObservation,
};
pub use tools::result::{ToolExecution, ToolOutput, ToolResult};
pub use tools::{
    ArgumentFormat, CallContext, CallOrigin, CallerMode, CustomCall, CustomFormat, CustomTool,
    FunctionStrictness, FunctionTool, GrammarSyntax, ItemLifecycle, PrefixCacheTtl, Program,
    ProgramOutput, StrictDefault, ToolArguments, ToolCall, ToolChoice, ToolDefinition,
    ToolDispatch, ToolKind, ToolNamespace, ToolReference,
};
pub use usage::reports::{
    CacheWriteTokens, ModalityTokens, SplitTotalRelation, TokenModality, ToolInputRelation,
    ToolUsageKind, ToolUseCount, UsageAccounting, UsageBucket, UsageCounter, UsageDetails,
    UsageReport, UsageReports, UsageScope, UsageStage,
};
pub use usage::views::{DerivedTokenCount, UsageFormula};
pub use usage::{InputTokenRelation, OutputTokenRelation, TotalTokenRelation, Usage};

pub use response::{
    CompletionCause, GenerationResponse, IncompleteReason, Outcome, ResponseError, TerminalDetails,
};
pub use safety::{
    SafetyBlockReason, SafetyCategory, SafetyProbability, SafetyRating, SafetyReport,
    SafetySetting, SafetySubject, SafetyThreshold,
};
pub use stream::{
    EventError, ItemKind, PartKind, StreamEvent, StreamItem, StreamPart, StreamState,
    StreamTerminal, end_of_stream, materialize, reduce, snapshot_items,
};
pub use validate::{MAX_ITEMS, MAX_TEXT_BYTES, MAX_TOOLS, MAX_TOTAL_BYTES};
