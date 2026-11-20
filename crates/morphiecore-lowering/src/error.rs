//! Safe codec and lowering rejections, independent of a wire family.
use crate::{
    conversion::generation::{ConversionBoundary, RepresentationError},
    failure::{CarrierReason, Resource, ShapeReason, SourceFailureEvidence},
};
use morphiecore_semantic::{
    context::ContextError,
    task::generation::{EventError, GenerationError},
    value::{JsonError, JsonResource},
};

#[derive(Clone, Debug, Eq, thiserror::Error, PartialEq)]
pub enum CodecError {
    #[error("{cause}")]
    Representation {
        boundary: ConversionBoundary,
        #[source]
        cause: RepresentationError,
    },
    #[error(transparent)]
    Context(#[from] ContextError),
    #[error("invalid {0}")]
    Invalid(ShapeReason),
    #[error("unsupported field or representation: {0}")]
    Unsupported(CarrierReason),
    #[error("codec input exceeds slice limits")]
    Limit,
    #[error("codec resource limit: {0:?}")]
    Resource(Resource),
    /// An upstream error inside a stream, as the HTTP status its type documents.
    #[error("upstream stream error ({0})")]
    StreamFailure(SourceFailureEvidence),
    #[error("target representation belongs to another codec")]
    ProfileMismatch,
    #[error(transparent)]
    Semantic(#[from] GenerationError),
    #[error(transparent)]
    Event(#[from] EventError),
}
impl CodecError {
    /// The common structural rejection: a known field has an invalid value.
    pub const fn invalid() -> Self {
        Self::Invalid(ShapeReason::InvalidValue)
    }
}

impl From<JsonError> for CodecError {
    fn from(error: JsonError) -> Self {
        match error {
            JsonError::Invalid => Self::Invalid(ShapeReason::Json),
            JsonError::Limit(resource) => Self::Resource(match resource {
                JsonResource::Bytes => Resource::Bytes,
                JsonResource::Depth => Resource::Depth,
                JsonResource::Nodes => Resource::Nodes,
            }),
        }
    }
}
