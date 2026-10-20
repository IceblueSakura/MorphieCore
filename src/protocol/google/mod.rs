//! Google wire APIs. `generateContent` and stateless `Interactions` are separate
//! operations of one vendor; vendor-wide restrictions live beside them, not in an API.
pub mod generate_content;
pub mod interactions;
mod media;

pub(crate) use media::check_public_media_url;
