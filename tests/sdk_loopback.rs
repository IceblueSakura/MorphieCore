//! Explicit, ignored SDK gates: codec fixtures plus the actual gateway Router
//! against a synthetic HTTP Provider. All listeners and credentials are test-owned.
pub(crate) use morphiecore_test_harness::{
    embedding_loop, google_tool_loop as google_tool_fixture,
    messages_tool_loop as messages_tool_fixture, openai_tool_loop,
    semantic_events as events_support, upstream_assets,
};
pub(crate) use morphiecore_test_support::{
    chat_profile as chat_wire, child_process, responses_profile as responses_wire,
};
#[path = "sdk/chat_media.rs"]
mod chat_media_sdk;
#[path = "sdk/chat.rs"]
mod chat_sdk;
#[path = "sdk/embeddings.rs"]
mod embeddings;
#[path = "sdk/gateway.rs"]
mod gateway_sdk;
#[path = "sdk/google_tool_history.rs"]
mod google_tool_history;
#[path = "sdk/messages.rs"]
mod messages_sdk;
#[path = "sdk/messages_tool_loop.rs"]
mod messages_tool_loop;
#[path = "sdk/openai_tool_history.rs"]
mod openai_tool_history;
#[path = "sdk/standard_requests.rs"]
mod standard_requests;
#[path = "sdk/text_loop.rs"]
mod text_loop;
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
