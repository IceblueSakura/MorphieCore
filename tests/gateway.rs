//! HTTP Router integration and shared test-owned upstream lifetime.
//! Binary bootstrap and feature/failure checks live in focused child modules.
pub(crate) use morphiecore_test_harness::{
    embedding_loop, gateway as support, google_tool_loop as google_tool_fixture,
    messages_tool_loop as messages_tool_fixture, openai_tool_loop,
};
pub(crate) use morphiecore_test_support::{
    chat_profile as chat_wire, filesystem, messages_stream, messages_wire,
    responses_profile as responses_wire, telemetry, tokenplan_audio as native_audio,
};

#[path = "gateway/anthropic.rs"]
mod anthropic;
#[path = "gateway/bootstrap.rs"]
mod bootstrap;
#[path = "gateway/chat_tool_history.rs"]
mod chat_tool_history;
#[path = "gateway/conformance.rs"]
mod conformance;
#[path = "gateway/embeddings.rs"]
mod embeddings;
#[path = "gateway/google_tool_loop.rs"]
mod google_tool_loop;
#[path = "gateway/interactions.rs"]
mod interactions;
#[path = "gateway/messages_tool_loop.rs"]
mod messages_tool_loop;
#[path = "gateway/responses_history.rs"]
mod responses_history;
#[path = "gateway/router_smoke.rs"]
mod router_smoke;
#[path = "gateway/tool_publication.rs"]
mod tool_publication;

struct Guard(tokio::task::JoinHandle<()>);
impl Drop for Guard {
    fn drop(&mut self) {
        self.0.abort();
    }
}
