//! Responses wire entry points. Envelopes, task requests, shared items and SSE stay distinct.
pub mod envelope;
pub(crate) mod managed;
pub mod sse;
pub use crate::protocol::openai::static_response::{
    decode_responses as decode_response, encode_responses as encode_response,
};
pub(super) mod items;
pub(super) mod request;
pub use request::{decode_generation, encode_generation};
