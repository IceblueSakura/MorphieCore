//! Chat wire entry points. Task requests and shared message/item mapping have separate owners.
pub(in crate::openai) mod annotations;
pub(in crate::openai) mod audio;
pub mod envelope;
pub mod logprobs;
pub(in crate::openai) mod owners;
pub(in crate::openai) mod reasoning;
pub mod response;
pub mod sse;
pub use response::{decode as decode_response, encode as encode_response};
pub(super) mod items;
pub(super) mod request;
pub use request::{decode_generation, encode_generation};
