//! Stateless Interactions v1 request projection over the shared IR.
//! Source: <https://ai.google.dev/api/interactions-api-v1>.
mod accounting;
mod replay;
mod request;
mod schema;
mod sse;
mod target;

pub use request::{RequestRule, encode_generation_request};
pub use sse::GenerationSseDecoder;
pub use target::{TargetCaps, representation_contract, semantic_contract};
