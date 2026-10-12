//! Semantic validation failures, independent of protocols and execution.
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
    #[error("context transformation would change a protected authority, scope, phase or owner")]
    InvalidContextTransform,
    #[error("tool call identity is duplicated")]
    DuplicateCall,
    #[error("tool result has no matching preceding call or duplicates a result")]
    InvalidToolResult,
    #[error("invalid replay attachment format or owner")]
    InvalidReplay,
    #[error("invalid Provider operation observation or unresolved history reference")]
    InvalidProviderObservation,
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
    #[error("invalid citation coordinate or source binding")]
    InvalidCitation,
    #[error("phase labels only apply to assistant messages")]
    PhaseInUserMessage,
    #[error("refusal requires assistant role")]
    RefusalInUserMessage,
    #[error("program output has no matching program, duplicates one, or lacks a terminal status")]
    InvalidProgramOutput,
}
