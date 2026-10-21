//! Actual CLI and independent file expectations; all credentials are synthetic.
#[cfg(unix)]
#[path = "credential/api_keys.rs"]
mod api_keys;
#[cfg(unix)]
#[path = "support/child_process.rs"]
mod child_process;
#[cfg(unix)]
#[path = "support/filesystem.rs"]
mod test_files;
#[cfg(unix)]
#[path = "credential/oauth_accounts.rs"]
mod unix;
