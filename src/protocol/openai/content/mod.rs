//! Shared OpenAI content and resource wire mappings.
pub(crate) mod citations;
pub(in crate::protocol::openai) mod file;
pub(in crate::protocol::openai) mod image;
pub(crate) mod input_audio;
pub(in crate::protocol::openai) mod text;
pub(in crate::protocol::openai) mod video;

pub(in crate::protocol::openai) mod call;
pub(in crate::protocol::openai) mod intake;
