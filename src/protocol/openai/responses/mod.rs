//! Responses wire entry points. Envelopes, task requests, shared items and SSE stay distinct.
pub(in crate::protocol::openai) mod dialect;
pub mod envelope;
mod history;
pub(crate) mod managed;
pub(crate) mod response;
pub mod sse;
pub use response::{decode as decode_response, encode as encode_response};
pub(super) mod items;
pub(super) mod request;
pub use request::{decode_generation, encode_generation};
