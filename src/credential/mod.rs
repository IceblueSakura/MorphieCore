//! Unified credential ownership; never imported by semantic codecs or lowering.
mod api_key;
pub use api_key::{ApiKeyAccess, ApiKeyState, ApiKeyStatus};
mod access;
pub use access::{AccessBinding, AccessGrant};
mod browser;
mod callback;
mod driver;
mod grok;
mod grok_metadata;
mod http;
mod jwt;
mod manager;
mod model;
mod oauth;
mod pool;
mod profiles;
pub mod siwc;
#[cfg(test)]
mod siwc_tests;
mod storage_fs;
#[cfg(windows)]
mod storage_windows;
mod store;
pub use pool::{
    CredentialPool, CredentialRef, MAX_POOL_MEMBERS, PoolAccess, PoolMember, PoolStatus,
};

pub use driver::{
    AuthDriver, DriverFuture, LoginObserver, RegistrationContext, RegistrationObserver,
};
pub use manager::CredentialManager;
pub use model::{
    AccessState, AccountState, AccountStatus, BrowserPrompt, Credential, DevicePrompt, Grant,
    LoginMethod, LoginOptions, LoginPrompt, LogoutOutcome, PendingRenewal, Renewal, Secret,
    SecretBytes, VerifiedIdentity,
};
pub use profiles::builtin_drivers;

/// Read one explicitly selected private file, never an environment value or auth cache.
pub fn read_private_file(
    path: &std::path::Path,
    limit: u64,
) -> Result<SecretBytes, CredentialError> {
    if !(1..=8 * 1024 * 1024).contains(&limit) {
        return Err(CredentialError::InvalidInput);
    }
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(std::path::Path::new("."));
    if !parent.is_dir() {
        return Err(CredentialError::Storage);
    }
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or(CredentialError::InvalidInput)?;
    storage_fs::Directory::open(parent)?
        .read(name, limit)?
        .ok_or(CredentialError::Storage)
}

#[derive(Debug, serde::Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CredentialStatus {
    #[serde(rename = "oauth")]
    OAuth(AccountStatus),
    ApiKey(ApiKeyStatus),
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CredentialError {
    #[error("API key record already exists; explicit revision-checked replacement required")]
    KeyExists,
    #[error("API key is unavailable or its binding changed; explicit rebinding required")]
    KeyUnavailable,
    #[error("unknown authorization profile")]
    UnknownProfile,
    #[error("operation or login method is unsupported by this profile")]
    Unsupported,
    #[error("legacy credential snapshot is unsupported; select a new explicit store directory")]
    LegacyStore,
    #[error(
        "credential publication is uncertain; explicit login, key replacement or local removal required"
    )]
    PersistenceUncertain,
    #[error("invalid account or client identifier")]
    InvalidInput,
    #[error("credential store is unavailable or unsafe")]
    Storage,
    #[error("credential store is busy")]
    Busy,
    #[error("credential pool limit reached")]
    PoolFull,
    #[error("account is not signed in; login required")]
    LoginRequired,
    #[error("authorization transaction was superseded")]
    Superseded,
    #[error("authorized identity does not match the account binding")]
    IdentityMismatch,
    #[error("required OAuth permissions were not granted")]
    Permission,
    #[error("authority returned an invalid or unsupported response")]
    Protocol,
    #[error("authority request failed; no automatic retry")]
    Network,
    #[error("authority request timed out")]
    Timeout,
    #[error("authorization was denied")]
    Denied,
    #[error("authorization transaction expired")]
    Expired,
    #[error("loopback callback listener is unavailable")]
    Callback,
    #[error("device login is not enabled for this deployment; verify authority/account policy")]
    DeviceUnavailable,
    #[error("refresh token expired; login required")]
    GrantExpired,
    #[error("refresh token already used; login required")]
    GrantReused,
    #[error("refresh token revoked; login required")]
    GrantRevoked,
    #[error("OAuth grant was rejected; login required")]
    InvalidGrant,
}

#[cfg(test)]
mod api_key_tests;
#[cfg(test)]
mod browser_tests;
#[cfg(test)]
mod manager_tests;
#[cfg(test)]
mod pool_tests;
#[cfg(test)]
mod storage_tests;
#[cfg(test)]
pub(crate) mod test_support;
#[cfg(test)]
mod tests;
