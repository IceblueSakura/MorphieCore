//! Explicit, ignored SDK gates: codec fixtures plus the actual gateway Router
//! against a synthetic HTTP Provider. All listeners and credentials are test-owned.
#[path = "sdk/chat_media.rs"]
mod chat_media_sdk;
#[path = "sdk/chat.rs"]
mod chat_sdk;
#[path = "support/chat_profile.rs"]
mod chat_wire;
#[path = "support/child_process.rs"]
mod child_process;
#[path = "support/credential_recovery.rs"]
mod credential_recovery;
#[path = "support/embedding_loop.rs"]
mod embedding_loop;
#[path = "support/embeddings.rs"]
mod embedding_support;
#[path = "sdk/embeddings.rs"]
mod embeddings;
#[path = "support/semantic_events.rs"]
#[allow(dead_code)]
mod events_support;
#[path = "support/filesystem.rs"]
mod filesystem;
#[path = "sdk/gateway.rs"]
mod gateway_sdk;
#[path = "support/google_tool_loop.rs"]
mod google_tool_fixture;
#[path = "sdk/google_tool_history.rs"]
mod google_tool_history;
#[path = "sdk/messages.rs"]
mod messages_sdk;
#[path = "support/messages_tool_loop.rs"]
mod messages_tool_fixture;
#[path = "sdk/messages_tool_loop.rs"]
mod messages_tool_loop;
#[path = "support/messages_wire.rs"]
mod messages_wire;
#[path = "sdk/openai_tool_history.rs"]
mod openai_tool_history;
#[path = "support/openai_tool_loop.rs"]
mod openai_tool_loop;
#[path = "support/responses_profile.rs"]
mod responses_wire;
#[path = "sdk/standard_requests.rs"]
mod standard_requests;
#[path = "sdk/text_loop.rs"]
mod text_loop;
#[path = "support/upstream_assets.rs"]
#[allow(dead_code)] // Framing and intake-only helpers are exercised by the transport target.
mod upstream_assets;
#[path = "sdk/upstream_assets.rs"]
mod upstream_fixture_sdk;

use axum::{body::Body, http::StatusCode, response::Response};
use std::sync::{Arc, Mutex};
use tokio::process::Command;

fn isolated_sdk_python(home: &std::path::Path) -> Command {
    let mut command = Command::new(
        std::env::var_os("MORPHIECORE_SDK_PYTHON").unwrap_or_else(|| "python3".into()),
    );
    command
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .env_clear()
        .env("HOME", home)
        .env("PATH", std::env::var_os("PATH").unwrap_or_default());
    command
}

#[derive(Clone, Default)]
struct Suite(
    Arc<Mutex<(usize, Option<&'static str>)>>,
    // Responses-only fixture variant: readable reasoning without reported opaque.
    bool,
);

fn failure(code: StatusCode, phase: &'static str, state: &Suite) -> Response {
    state.0.lock().unwrap().1 = Some(phase);
    Response::builder()
        .status(code)
        .body(Body::empty())
        .unwrap()
}

struct ServerGuard(tokio::task::AbortHandle);
impl Drop for ServerGuard {
    fn drop(&mut self) {
        self.0.abort();
    }
}
