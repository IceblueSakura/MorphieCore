//! `generateContent` upstream: request projection and streamed intake over the shared
//! IR. Upstream only; no client codec, OpenAI profile or wire switches.
mod accounting;
mod items;
mod logprobs;
mod replay;
mod request;
mod safety;
mod schema;
mod sse;
mod target;

pub use request::{RequestRule, encode_generation_request};
pub use sse::GenerationSseDecoder;
pub use target::{TargetContract, representation_contract, semantic_contract};
