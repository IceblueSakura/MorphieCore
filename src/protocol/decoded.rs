//! Shared Generation envelopes and safe errors, independent of a wire family.
use crate::{
    protocol::fidelity::FidelityRecords,
    semantic::task::generation::{GenerationError, GenerationRequest, GenerationResponse},
};

#[derive(Clone, Debug, PartialEq)]
pub struct DecodedRequest {
    pub semantic: GenerationRequest,
    pub fidelity: FidelityRecords,
}
#[derive(Clone, Debug, PartialEq)]
pub struct DecodedResponse {
    pub semantic: GenerationResponse,
    pub fidelity: FidelityRecords,
    pub metadata: ResponseMetadata,
}
/// Envelope identity is not a task instruction or a routing input.
#[derive(Clone, Debug, PartialEq)]
pub struct ResponseMetadata {
    pub id: String,
    pub model: String,
    pub created: serde_json::Number,
    pub context: crate::semantic::context::ResponseContext,
    /// Representation records for the independently owned instruction echo.
    pub instruction_fidelity: FidelityRecords,
}
#[derive(Clone, Debug, Eq, thiserror::Error, PartialEq)]
pub enum CodecError {
    #[error(transparent)]
    Context(#[from] crate::semantic::context::ContextError),
    #[error("invalid {0}")]
    Invalid(&'static str),
    #[error("unsupported field or representation: {0}")]
    Unsupported(String),
    #[error("codec input exceeds slice limits")]
    Limit,
    #[error("target representation belongs to another codec")]
    ProfileMismatch,
    #[error(transparent)]
    Semantic(#[from] GenerationError),
    #[error(transparent)]
    Event(#[from] crate::semantic::task::generation::EventError),
}
