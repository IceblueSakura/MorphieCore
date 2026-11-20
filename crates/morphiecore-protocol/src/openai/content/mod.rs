//! Shared OpenAI content and resource wire mappings.
pub mod citations;
pub(in crate::openai) mod file;
pub(in crate::openai) mod image;
pub mod input_audio;
pub(in crate::openai) mod text;
pub(in crate::openai) mod video;

pub(in crate::openai) mod call;
pub(in crate::openai) mod intake;
