//! HTTP Router integration and shared test-owned upstream lifetime.
//! Binary bootstrap and feature/failure checks live in focused child modules.
#[allow(dead_code)]
#[path = "support/chat_profile.rs"]
mod chat_wire;
#[path = "support/credential_recovery.rs"]
mod credential_recovery;
#[path = "support/filesystem.rs"]
mod filesystem;
#[path = "support/google_tool_loop.rs"]
mod google_tool_fixture;
#[path = "support/messages_stream.rs"]
mod messages_stream;
#[path = "support/messages_tool_loop.rs"]
mod messages_tool_fixture;
#[path = "support/messages_wire.rs"]
mod messages_wire;
#[path = "support/tokenplan_audio.rs"]
mod native_audio;
#[path = "support/openai_tool_loop.rs"]
mod openai_tool_loop;
#[allow(dead_code)]
#[path = "support/responses_profile.rs"]
mod responses_wire;
#[path = "support/gateway.rs"]
mod support;

#[path = "gateway/anthropic.rs"]
mod anthropic;
#[path = "gateway/bootstrap.rs"]
mod bootstrap;
#[path = "gateway/chat_tool_history.rs"]
mod chat_tool_history;
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
