//! Chat wire entry points. Task requests and shared message/item mapping have separate owners.
pub(in crate::protocol::openai) mod annotations;
pub(in crate::protocol::openai) mod audio;
pub mod envelope;
pub(crate) mod logprobs;
pub(in crate::protocol::openai) mod owners;
pub(in crate::protocol::openai) mod reasoning;
pub mod sse;
pub use crate::protocol::openai::static_response::{
    decode_chat as decode_response, encode_chat as encode_response,
};
pub(super) mod items;
pub(super) mod request;
pub use request::{decode_generation, encode_generation};
