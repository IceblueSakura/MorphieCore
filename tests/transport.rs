//! Framing, Responses HTTP/SSE and body I/O are separate contracts, not interchangeable evidence.
#[path = "support/chat_profile.rs"]
mod chat_wire;
#[path = "support/responses_profile.rs"]
mod wire;

#[path = "transport/body_lifecycle.rs"]
mod body_lifecycle;
#[path = "transport/chain.rs"]
mod chain;
#[path = "transport/chat.rs"]
mod chat;
#[path = "transport/fallback_policy.rs"]
mod fallback_policy;
#[path = "transport/framing.rs"]
mod framing;
#[path = "transport/planning.rs"]
mod planning;
#[path = "transport/responses_sse.rs"]
mod responses_sse;
