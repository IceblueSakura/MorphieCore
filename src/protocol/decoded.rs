//! Shared Generation envelopes and safe errors, independent of a wire family.
use crate::{
    protocol::fidelity::FidelityRecords,
    semantic::task::generation::{GenerationError, GenerationRequest, GenerationResponse},
};

#[derive(Clone, Debug, PartialEq)]
pub struct DecodedRequest {
    pub evidence: crate::protocol::conversion::generation::EvidenceDelta,
    pub semantic: GenerationRequest,
    pub fidelity: FidelityRecords,
}
#[derive(Clone, Debug, PartialEq)]
pub struct DecodedResponse {
    pub evidence: crate::protocol::conversion::generation::EvidenceDelta,
    pub semantic: GenerationResponse,
    pub fidelity: FidelityRecords,
    pub metadata: ResponseMetadata,
}
/// Envelope identity is not a task instruction or a routing input.
#[derive(Clone, Debug, PartialEq)]
pub struct ResponseMetadata {
    /// Explicit downstream presentation only; source decoders never populate it.
    pub delivery: Option<std::sync::Arc<GenerationDelivery>>,
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
            .ok_or(CodecError::Invalid(super::failure::ShapeReason::Metadata))
    }
    /// Validate shared facts without imposing a wire family's required fields.
    pub fn validate(&self) -> Result<(), CodecError> {
        if let Some(delivery) = &self.delivery {
            delivery.settings.validate()?;
            crate::semantic::context::validate_metadata(delivery.request_metadata.as_ref())?;
            if let Some(usage) = delivery.usage {
                usage.validate()?;
            }
        }
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
            return Err(CodecError::Invalid(super::failure::ShapeReason::Metadata));
        }
        if self
            .created
            .as_ref()
            .is_some_and(|created| !crate::semantic::value::valid_timestamp(created))
        {
            return Err(CodecError::Invalid(super::failure::ShapeReason::Metadata));
        }
        self.context.validate()?;
        Ok(())
    }
}
/// Request-local presentation and completed accounting are not upstream observations.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GenerationDelivery {
    pub settings: crate::semantic::task::generation::GenerationSettings,
    pub usage: Option<crate::semantic::task::generation::Usage>,
    /// Original admitted request association, not candidate-prepared metadata.
    pub request_metadata: Option<std::collections::BTreeMap<String, String>>,
}
impl ResponseMetadata {
    /// Whole-field report precedence includes explicit null and empty objects.
    /// Only Responses rendering consumes this fallback.
    pub(crate) fn request_metadata_echo(
        &self,
    ) -> Option<&std::collections::BTreeMap<String, String>> {
        if !self.context.execution.metadata.is_absent() {
            return None;
        }
        self.delivery.as_ref()?.request_metadata.as_ref()
    }
}
impl GenerationDelivery {
    pub(crate) fn request_metadata_bytes(&self) -> usize {
        std::mem::size_of_val(&self.request_metadata).saturating_add(
            crate::semantic::context::metadata_retained_bytes(self.request_metadata.as_ref()),
        )
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
            return Err(CodecError::Invalid(
                super::failure::ShapeReason::InvalidValue,
            ));
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Eq, thiserror::Error, PartialEq)]
pub enum CodecError {
    #[error("{cause}")]
    Representation {
        boundary: crate::protocol::conversion::generation::ConversionBoundary,
        #[source]
        cause: crate::protocol::conversion::generation::RepresentationError,
    },
    #[error(transparent)]
    Context(#[from] crate::semantic::context::ContextError),
    #[error("invalid {0}")]
    Invalid(super::failure::ShapeReason),
    #[error("unsupported field or representation: {0}")]
    Unsupported(super::failure::CarrierReason),
    #[error("codec input exceeds slice limits")]
    Limit,
    #[error("codec resource limit: {0:?}")]
    Resource(super::failure::Resource),
    /// An upstream error inside a stream, as the HTTP status its type documents.
    #[error("upstream stream error ({0})")]
    StreamFailure(super::failure::SourceFailureEvidence),
    #[error("target representation belongs to another codec")]
    ProfileMismatch,
    #[error(transparent)]
    Semantic(#[from] GenerationError),
    #[error(transparent)]
    Event(#[from] crate::semantic::task::generation::EventError),
}
impl CodecError {
    /// The common structural rejection: a known field has an invalid value.
    pub const fn invalid() -> Self {
        Self::Invalid(crate::protocol::failure::ShapeReason::InvalidValue)
    }
}

impl From<crate::semantic::value::JsonError> for CodecError {
    fn from(error: crate::semantic::value::JsonError) -> Self {
        use crate::semantic::value::{JsonError, JsonResource};
        match error {
            JsonError::Invalid => Self::Invalid(super::failure::ShapeReason::Json),
            JsonError::Limit(resource) => Self::Resource(match resource {
                JsonResource::Bytes => super::failure::Resource::Bytes,
                JsonResource::Depth => super::failure::Resource::Depth,
                JsonResource::Nodes => super::failure::Resource::Nodes,
            }),
        }
    }
}
