//! Shared synthetic fixtures, property config, isolated proxy matrix, child
//! process helpers, telemetry collector, and filesystem helpers for unit and
//! integration tests.

pub mod chat_profile;
pub mod child_line;
pub mod child_process;
pub mod filesystem;
pub mod messages_stream;
pub mod messages_wire;
pub mod properties;
pub mod proxy_environment;
pub mod responses_profile;
pub mod telemetry;
pub mod tokenplan_audio;

use std::path::PathBuf;

/// Returns the repository workspace root directory.
pub fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates directory")
        .parent()
        .expect("workspace root")
        .to_path_buf()
}
