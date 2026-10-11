//! Protocol-neutral requests, failures and selected upstream dispatch.
use crate::{lowering::generation::RepresentationError, protocol::CodecError};
pub mod embeddings;
pub mod images;
mod request;
pub mod speech;
pub mod transcription;
mod upstream;
pub use request::Request;
pub use upstream::UpstreamAdapter;
#[derive(Debug, thiserror::Error)]
pub enum AdapterError {
    #[error(transparent)]
    Codec(#[from] CodecError),
    #[error(transparent)]
    Representation(#[from] RepresentationError),
}
