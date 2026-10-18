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
#[derive(Debug, thiserror::Error)]
pub enum AdapterError {
    #[error(transparent)]
    Codec(#[from] CodecError),
    #[error(transparent)]
    Representation(#[from] RepresentationError),
}
