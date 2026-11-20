//! Shared synthetic fixtures, property config, isolated proxy matrix, and
//! filesystem helpers for unit and integration tests.

pub mod chat_profile;
pub mod filesystem;
pub mod properties;
pub mod proxy_environment;
pub mod responses_profile;

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
