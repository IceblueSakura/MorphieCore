//! Selected upstream and client codec dispatch; pure contracts live in protocol.
mod downstream;
pub mod embeddings;
pub mod images;
pub mod speech;
pub mod transcription;
mod upstream;
pub use downstream::ClientAdapter;
pub(crate) use downstream::{ClientEncoder, SAFETY_ASSESSMENT_RULE};
pub(crate) use upstream::GenerationDecoder;
pub use upstream::UpstreamAdapter;
