//! Generation semantic IR.
mod audio;
mod audio_stream;
pub use audio::{
    AudioFormat, AudioOutputOptions, AudioReference, GeneratedAudio, MAX_AUDIO_DECODED_BYTES,
    OutputModality,
};
pub use audio_stream::{AudioBuffer, AudioUpdate};
mod configuration;
mod continuation;
pub use configuration::{ConfigurationId, ConfigurationSnapshot, ToolDefinitionBinding};
mod dependency;
pub use dependency::{
    HistoryDependency, RequestDependencyProof, SettingsDependency, SettingsField,
};
mod contract;
pub use continuation::{CallReference, Continuation, ProviderContinuationRequirement};
pub use contract::{GenerationFeature, GenerationSemanticContract};
mod envelope;
mod event;
pub use envelope::{MessageEnvelope, MessageEnvelopeRole};
mod group;
pub use group::{GroupId, MessageGroup, ReplayGroup, ReplayGroupView};
mod identity;
pub use identity::{
    AliasResolutionError, LocalScope, NativeAliasDomain, NativeCallAlias, NativeIdKind,
    resolve_call_alias,
};
mod output;
mod pattern;
mod progress;
pub use progress::InteractionProgress;
mod provider;
pub use provider::{
    ProviderAction, ProviderExecutionProgress, ProviderOperation, ProviderOperationReference,
    ProviderRequester, ProviderResultFormat, ProviderToolObservation,
};
mod reasoning;
mod replay;
pub(crate) use replay::ReplayDependency;
pub use replay::{ReplayFormat, ReplayOwner, ReplayValue};
mod request;
mod requirements;
mod resource;
mod resource_table;
pub use resource_table::{
    ResourceBody, ResourceConditions, ResourceDeclaration, ResourceDependency, ResourceId,
    ResourcePurpose, ResourceTable, ResourceTarget, ResourceUse,
};
mod response;
mod schema;
mod schema_document;
pub use schema_document::{
    MAX_SCHEMA_REFERENCES, MAX_SCHEMA_RESOURCES, SchemaDialect, SchemaDocument, SchemaResource,
};
mod schema_number;
mod stream_value;
pub use stream_value::StreamPartValue;
mod citation;
mod structured;
mod text;
pub use citation::{
    Annotation, Citation, CitationKind, ClaimAnchor, SourceBounds, SourceCoordinates, TextRange,
    TextUnit,
};
mod tool;
pub use structured::StructuredValue;
mod tool_result;
pub use tool_result::{ToolExecution, ToolOutput, ToolResult, ToolResultPart};
mod turn;
pub use turn::{
    ContinuationError, ResponseContinuation, ResponseId, ResponseRelation, ResultReadiness, TurnId,
};
mod usage;
pub use usage::{
    InputTokenRelation, OutputTokenRelation, TotalTokenRelation, Usage, UsageBasis, UsageScope,
};
mod usage_views;
pub use usage_views::{DerivedTokenCount, UsageFormula};
mod validate;
pub use output::{OutputConstraint, TextOptions, Verbosity};
pub use reasoning::{
    ReasoningBudget, ReasoningContent, ReasoningContext, ReasoningDisplay, ReasoningEffort,
    ReasoningItem, ReasoningMode, ReasoningPresence, ReasoningReplay, ReasoningRequest,
    ReasoningSummary,
};
pub use request::{
    ClientManaged, ConfigurationUpdate, ContentPart, ContextBuild, ContextChange, ContextEdit,
    ContextError, ContextStage, GenerationControls, GenerationError, GenerationRequest,
    GenerationSettings, Instruction, InstructionAuthority, Item, ItemId, Message, MessageRole,
    Part, PartId, Phase, SelectedHistory, Truncation,
};
pub use requirements::{GenerationRequirements, GenerationResponseRequirements};
pub use resource::{
    FileDescription, FileDetail, ImageDetail, ImageFormat, MAX_FILE_DECODED_BYTES,
    MAX_FILE_NAME_BYTES, MAX_IMAGE_DECODED_BYTES, MAX_RESOURCE_MEDIA_TYPE_BYTES,
    MAX_RESOURCE_URL_BYTES, MAX_TOTAL_FILE_DECODED_BYTES, Resource, ResourceDescription,
    ResourceKind, ResourceLocation, ResourceView,
};
pub use text::{
    Logprob, RefusalContent, TextContent, TopLogprob, compatible_logprobs, validate_logprobs,
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
