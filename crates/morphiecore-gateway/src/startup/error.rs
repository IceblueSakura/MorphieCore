//! Startup failures; values never include secrets or private paths.
#[derive(Debug, thiserror::Error)]
pub enum StartupError {
    // Only schema-owned paths and sanitized reasons reach application diagnostics.
    #[error(transparent)]
    Detail(#[from] crate::configuration::ConfigurationError),
    #[error(transparent)]
    Account(#[from] crate::account::AccountError),
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

impl StartupError {
    pub fn diagnostic(&self) -> serde_json::Value {
        let mut value = serde_json::json!({"message":self.to_string()});
        if let Self::Detail(error) | Self::Account(crate::account::AccountError::Detail(error)) =
            self
        {
            value["domain"] = serde_json::json!(error.domain);
            value["field"] = serde_json::json!(error.field);
            value["reason"] = serde_json::json!(error.reason);
        }
        value
    }
}
