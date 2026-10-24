//! Protocol-neutral requests, failures and selected upstream dispatch.
use crate::{lowering::generation::RepresentationError, protocol::CodecError};
mod downstream;
pub mod embeddings;
pub mod images;
mod request;
pub mod speech;
pub mod transcription;
mod upstream;
pub use downstream::ClientAdapter;
pub(crate) use downstream::{ClientEncoder, SAFETY_ASSESSMENT_RULE};
pub use request::Request;
pub(crate) use upstream::GenerationDecoder;
pub use upstream::UpstreamAdapter;
/// Downstream wire identity, independent of the selected upstream endpoint.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum ClientProtocol {
    Chat,
    Responses,
    Messages,
}
impl From<crate::protocol::openai::Profile> for ClientProtocol {
    fn from(profile: crate::protocol::openai::Profile) -> Self {
        match profile {
            crate::protocol::openai::Profile::Chat => Self::Chat,
            crate::protocol::openai::Profile::Responses => Self::Responses,
        }
    }
}
#[derive(Clone, Debug, thiserror::Error)]
pub enum ConversionCause {
    #[error(transparent)]
    Codec(#[from] CodecError),
    #[error(transparent)]
    Representation(#[from] RepresentationError),
}

/// The first responsible conversion boundary; outer wrappers preserve it.
#[derive(Clone, Debug, thiserror::Error)]
#[error("{cause}")]
pub struct ConversionFailure {
    pub boundary: Option<crate::lowering::generation::ConversionBoundary>,
    pub locus: Option<crate::lowering::generation::SemanticLocus>,
    #[source]
    pub cause: ConversionCause,
}
impl ConversionFailure {
    pub fn at(mut self, boundary: crate::lowering::generation::ConversionBoundary) -> Self {
        if self.boundary.is_none() {
            self.boundary = Some(boundary);
        }
        self
    }
}
impl From<CodecError> for ConversionFailure {
    fn from(error: CodecError) -> Self {
        match error {
            CodecError::Representation { boundary, cause } => Self {
                boundary: Some(boundary),
                locus: None,
                cause: ConversionCause::Representation(cause),
            },
            error => Self {
                boundary: None,
                locus: None,
                cause: ConversionCause::Codec(error),
            },
        }
    }
}
impl From<RepresentationError> for ConversionFailure {
    fn from(error: RepresentationError) -> Self {
        Self {
            boundary: None,
            locus: None,
            cause: ConversionCause::Representation(error),
        }
    }
}
