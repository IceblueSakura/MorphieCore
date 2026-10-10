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
    /// An upstream-reported timestamp; absence never implies zero or local time.
    pub created: Option<serde_json::Number>,
    pub context: crate::semantic::context::ResponseContext,
    /// Representation records for the independently owned instruction echo.
    pub instruction_fidelity: FidelityRecords,
}
impl ResponseMetadata {
    /// Validate shared facts without imposing a wire family's required fields.
    pub fn validate(&self) -> Result<(), CodecError> {
        if self.id.is_empty()
            || self.model.is_empty()
            || self.id.len() > 256
            || self.model.len() > 256
        {
            return Err(CodecError::Invalid("metadata"));
        }
        if self
            .created
            .as_ref()
            .is_some_and(|created| !crate::semantic::value::valid_timestamp(created))
        {
            return Err(CodecError::Invalid("timestamp"));
        }
        self.context.validate()?;
        Ok(())
    }
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
