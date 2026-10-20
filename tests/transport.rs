//! Framing, Responses HTTP/SSE and body I/O are separate contracts, not interchangeable evidence.
#[path = "support/chat_profile.rs"]
mod chat_wire;
#[path = "support/responses_profile.rs"]
mod wire;

#[path = "transport/anthropic_sse.rs"]
mod anthropic_sse;
#[path = "transport/attribution.rs"]
mod attribution;
#[path = "transport/body_lifecycle.rs"]
mod body_lifecycle;
#[path = "transport/chain.rs"]
mod chain;
#[path = "transport/chat.rs"]
mod chat;
#[path = "transport/framing.rs"]
mod framing;
#[path = "transport/google_sse.rs"]
mod google_sse;
#[path = "transport/model_values.rs"]
mod model_values;
#[path = "transport/native.rs"]
mod native;
#[path = "transport/planning.rs"]
mod planning;
#[path = "transport/responses_sse.rs"]
mod responses_sse;
