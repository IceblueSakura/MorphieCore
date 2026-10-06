//! Generation semantic IR.
mod audio;
mod audio_stream;
pub use audio::{
    AudioFormat, AudioOutputOptions, AudioReference, GeneratedAudio, MAX_AUDIO_DECODED_BYTES,
    OutputModality,
};
pub use audio_stream::{AudioBuffer, AudioUpdate};
mod continuation;
mod dependency;
pub use dependency::{
    HistoryDependency, RequestDependencyProof, SettingsDependency, SettingsField,
};
mod contract;
pub use continuation::{CallReference, Continuation};
pub use contract::{GenerationFeature, GenerationSemanticContract};
mod event;
mod group;
pub use group::MessageGroup;
mod output;
mod pattern;
mod progress;
pub use progress::InteractionProgress;
mod reasoning;
mod replay;
pub use replay::{ReplayFormat, ReplayValue};
mod request;
mod requirements;
mod resource;
mod response;
mod schema;
mod schema_number;
mod stream_value;
pub use stream_value::StreamPartValue;
mod structured;
mod text;
mod tool;
pub use structured::StructuredValue;
mod tool_result;
pub use tool_result::{ToolExecution, ToolOutput, ToolResult, ToolResultPart};
mod turn;
pub use turn::{
    ContinuationError, ResponseContinuation, ResponseId, ResponseRelation, ResultReadiness, TurnId,
};
mod usage;
pub use usage::{OutputTokenRelation, TotalTokenRelation, Usage, UsageBasis, UsageScope};
mod usage_views;
pub use usage_views::{DerivedTokenCount, UsageFormula};
mod validate;
pub use output::{OutputConstraint, TextOptions, Verbosity};
pub use reasoning::{
    ReasoningContent, ReasoningContext, ReasoningEffort, ReasoningItem, ReasoningMode,
    ReasoningPresence, ReasoningReplay, ReasoningRequest, ReasoningSummary,
};
pub use request::{
    ConfigurationUpdate, ContentPart, GenerationControls, GenerationError, GenerationRequest,
    GenerationSettings, Instruction, InstructionAuthority, Item, ItemId, Message, MessageRole,
    Part, PartId, Phase, Truncation,
};
pub use requirements::{GenerationRequirements, GenerationResponseRequirements};
pub use resource::{
    FileDescription, FileDetail, ImageDetail, ImageFormat, MAX_FILE_DECODED_BYTES,
    MAX_FILE_NAME_BYTES, MAX_IMAGE_DECODED_BYTES, MAX_RESOURCE_MEDIA_TYPE_BYTES,
    MAX_RESOURCE_URL_BYTES, MAX_TOTAL_FILE_DECODED_BYTES, Resource, ResourceDescription,
    ResourceKind, ResourceLocation,
};
pub use text::{
    Annotation, Logprob, RefusalContent, TextContent, TopLogprob, compatible_logprobs,
    validate_logprobs,
};
pub use tool::{
    ArgumentFormat, CallContext, CallOrigin, CallerMode, CustomCall, CustomFormat, CustomTool,
    FunctionStrictness, FunctionTool, GrammarSyntax, ItemLifecycle, Program, ProgramOutput,
    StrictDefault, ToolArguments, ToolCall, ToolChoice, ToolDefinition, ToolDispatch, ToolKind,
    ToolNamespace, ToolReference,
};

pub use event::{
    EventError, ItemKind, PartKind, StreamEvent, StreamItem, StreamPart, StreamState,
    StreamTerminal, end_of_stream, materialize, reduce, snapshot_items,
};
pub use response::{GenerationResponse, IncompleteReason, Outcome, ResponseError, TerminalDetails};
pub use validate::{MAX_ITEMS, MAX_TEXT_BYTES, MAX_TOOLS, MAX_TOTAL_BYTES};
