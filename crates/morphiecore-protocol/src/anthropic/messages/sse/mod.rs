//! Messages SSE: the native wire reducer and the IR mapping that reads it.
//! Framing, ordering and aggregation are validated once; the IR mapping never
//! re-derives wire state, and strict EOF alone releases a complete message.
mod decode;
mod wire;

pub use decode::GenerationSseDecoder;
pub use wire::MessageSseDecoder;
