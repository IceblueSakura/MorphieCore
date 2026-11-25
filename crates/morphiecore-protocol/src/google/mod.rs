//! Google wire APIs (embeddings and stateless Interactions).
//! Vendor-wide restrictions live beside them, not in an API.
pub mod embeddings;
pub mod interactions;
mod media;
mod sampling;
mod service;
pub mod strict_tools;

pub use media::check_public_media_url;
pub use sampling::SamplingPolicy;
