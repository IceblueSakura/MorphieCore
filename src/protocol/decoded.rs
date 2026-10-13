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
    /// API storage facts, separate from task settings and upstream execution hints.
    pub storage: Option<ResponseStorage>,
    /// Unreported upstream identity stays absent. Delivery may fill its own copy.
    pub id: Option<String>,
    pub model: String,
    /// Decoders keep an unreported timestamp absent. Delivery may explicitly
    /// assign its own creation time on a separate downstream envelope copy.
    pub created: Option<serde_json::Number>,
    pub context: crate::semantic::context::ResponseContext,
    /// Representation records for the independently owned instruction echo.
    pub instruction_fidelity: FidelityRecords,
}
impl ResponseMetadata {
    /// Wire encoders require an identity; they never generate one.
    pub(crate) fn required_id(&self) -> Result<&str, CodecError> {
        self.id
            .as_deref()
            .filter(|id| !id.is_empty() && id.len() <= 256)
            .ok_or(CodecError::Invalid("response id"))
    }
    /// Validate shared facts without imposing a wire family's required fields.
    pub fn validate(&self) -> Result<(), CodecError> {
        if let Some(storage) = &self.storage {
            storage.validate()?;
        }
        if self
            .id
            .as_ref()
            .is_some_and(|id| id.is_empty() || id.len() > 256)
            || self.model.is_empty()
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
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ResponseStorage {
    pub store: crate::semantic::value::Presence<bool>,
    pub previous_response_id: crate::semantic::value::Presence<String>,
}
impl ResponseStorage {
    pub fn validate(&self) -> Result<(), CodecError> {
        if self.previous_response_id.value().is_some_and(|id| {
            id.is_empty() || id.len() > 256 || !id.bytes().all(|b| b.is_ascii_graphic())
        }) {
            return Err(CodecError::Invalid("previous_response_id"));
        }
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
    /// An upstream error inside a stream, as the HTTP status its type documents.
    #[error("upstream stream error (HTTP {0})")]
    StreamFailure(u16),
    #[error("target representation belongs to another codec")]
    ProfileMismatch,
    #[error(transparent)]
    Semantic(#[from] GenerationError),
    #[error(transparent)]
    Event(#[from] crate::semantic::task::generation::EventError),
}
