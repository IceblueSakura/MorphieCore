//! Pure conversion contracts. Domain projections remain with their codecs.
pub mod generation;
use crate::{CodecError, conversion::generation::RepresentationError};
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum ConversionCause {
    #[error(transparent)]
    Codec(#[from] CodecError),
    #[error(transparent)]
    Representation(#[from] RepresentationError),
}

/// The first responsible conversion boundary; outer wrappers preserve it.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
#[error("{cause}")]
pub struct ConversionFailure {
    pub boundary: Option<crate::conversion::generation::ConversionBoundary>,
    pub locus: Option<crate::conversion::generation::SemanticLocus>,
    #[source]
    pub cause: ConversionCause,
}
impl ConversionFailure {
    pub fn at(mut self, boundary: crate::conversion::generation::ConversionBoundary) -> Self {
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
