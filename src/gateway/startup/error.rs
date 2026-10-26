//! Startup failures; values never include secrets or private paths.
#[derive(Clone, Copy, Debug, thiserror::Error)]
pub enum StartupError {
    #[error("invalid {0} configuration")]
    Configuration(&'static str),
    #[error("invalid gateway limits")]
    Limits,
    #[error("missing or invalid credential configuration")]
    Credentials,
    #[error("invalid or duplicate fixed entry binding")]
    Binding,
    #[error("invalid HTTP transport configuration")]
    Transport,
    #[error("invalid loopback listener configuration")]
    Listener,
    #[error("invalid startup environment")]
    Environment,
}
