//! Shared credential semantics, independent of product wire and filesystem layout.
use super::CredentialError as Error;
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
pub type SecretBytes = zeroize::Zeroizing<Vec<u8>>;
use std::time::{SystemTime, UNIX_EPOCH};

pub struct Secret(SecretString);
// Persistence is an explicit exposure boundary; secrecy does not implicitly serialize.
impl Serialize for Secret {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.expose())
    }
}
impl<'de> Deserialize<'de> for Secret {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::new(String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}
impl Secret {
    pub fn new(value: String) -> Result<Self, Error> {
        let secret = Self(SecretString::from(value));
        if !secret.valid() {
            return Err(Error::Protocol);
        }
        Ok(secret)
    }
    pub fn expose(&self) -> &str {
        self.0.expose_secret()
    }
    pub(crate) fn valid(&self) -> bool {
        let value = self.expose();
        !value.is_empty() && value.len() <= 16384 && value.bytes().all(|c| c.is_ascii_graphic())
    }
}
impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Secret([redacted])")
    }
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Credential {
    pub access: Secret,
    pub refresh: Option<Secret>,
    pub id_token: Option<Secret>,
    pub expires_at: Option<u64>,
    pub scopes: Option<String>,
}
impl Credential {
    pub(crate) fn valid(&self) -> bool {
        self.access.valid()
            && self.refresh.as_ref().is_none_or(Secret::valid)
            && self.id_token.as_ref().is_none_or(Secret::valid)
            && self.expires_at.is_none_or(|v| v > 0)
            && self
                .scopes
                .as_ref()
                .is_none_or(|s| s.len() <= 4096 && s.bytes().all(|c| (0x20..=0x7e).contains(&c)))
    }
}
/// Driver-verified principal. Scope disambiguates authority-local tenants/workspaces;
/// it is part of immutable identity, not a bag of product metadata.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerifiedIdentity {
    pub subject: String,
    pub scope: Option<String>,
}
impl std::fmt::Debug for VerifiedIdentity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("VerifiedIdentity([private])")
    }
}
impl VerifiedIdentity {
    pub(crate) fn valid(&self) -> bool {
        identity_field(&self.subject) && self.scope.as_deref().is_none_or(identity_field)
    }
}
pub struct Grant {
    pub identity: VerifiedIdentity,
    pub credential: Credential,
}
/// Replacement material is quarantined until the driver's identity check succeeds.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PendingRenewal {
    pub credential: Credential,
    pub verify_id_token: bool,
}
pub enum Renewal {
    Verified(Grant),
    Pending(PendingRenewal),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoginMethod {
    Device,
    Browser,
}
#[derive(Clone, Debug)]
pub struct LoginOptions {
    pub method: LoginMethod,
    pub client_id: Option<String>,
    /// Explicit SIWC plan-permission consent, never enabled by routine reauthorization.
    pub consent: bool,
    /// None uses the driver's registered default; explicit zero is not absence.
    pub callback_port: Option<u16>,
}
impl Default for LoginOptions {
    fn default() -> Self {
        Self {
            method: LoginMethod::Device,
            client_id: None,
            consent: false,
            callback_port: None,
        }
    }
}
#[derive(Clone, Debug)]
pub struct DevicePrompt {
    pub verification_uri: String,
    pub user_code: String,
}
#[derive(Clone, Debug)]
pub struct BrowserPrompt {
    pub authorization_url: String,
    pub redirect_uri: String,
}
#[derive(Debug)]
pub enum LoginPrompt {
    Device(DevicePrompt),
    Browser(BrowserPrompt),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountState {
    Active,
    SignedOut,
    NeedsReauthorization,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AccessState {
    Unavailable,
    Unknown,
    Valid,
    Expired,
}
/// Deliberately excludes tokens, client registration and verified principal.
#[derive(Debug, Serialize)]
pub struct AccountStatus {
    pub renewal_pending: bool,
    /// Only profiles with separate identity and plan permission report this field.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan_usage_enabled: Option<bool>,
    pub profile: String,
    pub account: String,
    pub state: AccountState,
    pub access: AccessState,
    pub revision: u64,
    pub generation: u64,
    pub expires_at: Option<u64>,
    pub login_pending: bool,
    pub recovery_required: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogoutOutcome {
    LocalOnly,
    Revoked,
    RevocationUnconfirmed,
    AlreadySignedOut,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Account {
    #[serde(default)]
    pub pending_renewal: Option<PendingRenewal>,
    pub profile: String,
    pub alias: String,
    pub client_id: String,
    pub identity: Option<VerifiedIdentity>,
    pub revision: u64,
    pub generation: u64,
    pub login_attempt: Option<String>,
    pub state: AccountState,
    pub credential: Option<Credential>,
}
impl Account {
    pub fn new(profile: &str, alias: &str, client: &str) -> Self {
        Self {
            pending_renewal: None,
            profile: profile.into(),
            alias: alias.into(),
            client_id: client.into(),
            identity: None,
            revision: 0,
            generation: 0,
            login_attempt: None,
            state: AccountState::SignedOut,
            credential: None,
        }
    }
    pub fn advance(&mut self) -> Result<(), Error> {
        self.revision = self.revision.checked_add(1).ok_or(Error::Storage)?;
        Ok(())
    }
    pub fn replace_credential(&mut self, credential: Option<Credential>) -> Result<(), Error> {
        self.generation = self.generation.checked_add(1).ok_or(Error::Storage)?;
        self.credential = credential;
        Ok(())
    }
    pub fn validate(&self, profile: &str, alias: &str) -> Result<(), Error> {
        if self.profile != profile
            || self.alias != alias
            || valid_profile(profile).is_err()
            || valid_alias(alias).is_err()
            || valid_client(&self.client_id).is_err()
            || self.identity.as_ref().is_some_and(|v| !v.valid())
            || self.pending_renewal.as_ref().is_some_and(|pending| {
                !pending.credential.valid()
                    || self.identity.is_none()
                    || self.state != AccountState::NeedsReauthorization
                    || self.credential.is_some()
                    || self.login_attempt.is_some()
            })
            || self
                .login_attempt
                .as_ref()
                .is_some_and(|v| v.len() != 32 || !v.bytes().all(|c| c.is_ascii_hexdigit()))
        {
            return Err(Error::Storage);
        }
        match (&self.state, &self.credential) {
            (AccountState::Active, Some(c))
                if self.identity.is_some() && c.valid() && self.generation > 0 =>
            {
                Ok(())
            }
            (AccountState::SignedOut | AccountState::NeedsReauthorization, None) => Ok(()),
            _ => Err(Error::Storage),
        }
    }
    pub fn status(&self, recovery_required: bool) -> Result<AccountStatus, Error> {
        let timestamp = now()?;
        let expires_at = self.credential.as_ref().and_then(|c| c.expires_at);
        let access = if recovery_required || self.credential.is_none() {
            AccessState::Unavailable
        } else {
            match expires_at {
                None => AccessState::Unknown,
                Some(v) if v <= timestamp => AccessState::Expired,
                Some(_) => AccessState::Valid,
            }
        };
        Ok(AccountStatus {
            renewal_pending: !recovery_required && self.pending_renewal.is_some(),
            plan_usage_enabled: None,
            profile: self.profile.clone(),
            account: self.alias.clone(),
            state: if recovery_required {
                AccountState::NeedsReauthorization
            } else {
                self.state
            },
            access,
            revision: self.revision,
            generation: self.generation,
            expires_at: if recovery_required { None } else { expires_at },
            login_pending: self.login_attempt.is_some(),
            recovery_required,
        })
    }
}
pub(crate) fn valid_alias(value: &str) -> Result<(), Error> {
    if value.is_empty()
        || value.len() > 64
        || !value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"-_".contains(&c))
    {
        Err(Error::InvalidInput)
    } else {
        Ok(())
    }
}
pub(super) fn valid_profile(value: &str) -> Result<(), Error> {
    valid_alias(value)?;
    if value.bytes().any(|c| c.is_ascii_uppercase()) {
        Err(Error::InvalidInput)
    } else {
        Ok(())
    }
}
pub(super) fn valid_client(value: &str) -> Result<(), Error> {
    if value.is_empty() || value.len() > 256 || !value.bytes().all(|c| c.is_ascii_graphic()) {
        Err(Error::InvalidInput)
    } else {
        Ok(())
    }
}
fn identity_field(value: &str) -> bool {
    !value.is_empty() && value.len() <= 256 && !value.chars().any(char::is_control)
}
pub(super) fn now() -> Result<u64, Error> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|v| v.as_secs())
        .map_err(|_| Error::Storage)
}
pub(super) fn random_id() -> Result<String, Error> {
    let mut bytes = [0; 16];
    getrandom::fill(&mut bytes).map_err(|_| Error::Storage)?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}
