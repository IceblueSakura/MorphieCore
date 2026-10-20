//! Native Messages wire types: request, response envelope, blocks and SSE payloads.
//! Unknown fields stay unknown; shape reading and bounds live beside the types.
pub(in crate::protocol::anthropic::messages) mod content;
pub(in crate::protocol::anthropic::messages) mod envelope;
pub(in crate::protocol::anthropic::messages) mod event;
pub(in crate::protocol::anthropic::messages) mod request;

pub use content::{Block, Caller};
pub use envelope::{CacheCreation, Message, ServerToolUsage, StopReason, Usage};
pub use event::{Delta, Event};
pub use request::*;
