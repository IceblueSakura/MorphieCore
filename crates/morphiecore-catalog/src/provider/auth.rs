//! Credential binding identities and attempt-time auth material.
//!
//! Topology references credentials only by opaque binding id. Secret material is
//! resolved at the execution boundary, lives in memory for one attempt chain and
//! never enters topology, logs or reports.

use crate::provider::{errors::ProviderError, ident_ok};
use secrecy::{ExposeSecret, SecretString};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CredentialKind {
    ApiKey,
    OAuth(&'static str),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuthScheme {
    /// `Authorization: Bearer <secret>`.
    Bearer,
    OAuthBearer(&'static str),
    /// A dedicated API key header, e.g. `api-key: <secret>`.
    ApiKeyHeader(&'static str),
}

impl AuthScheme {
    pub const fn kind(&self) -> CredentialKind {
        match self {
            Self::OAuthBearer(profile) => CredentialKind::OAuth(profile),
            _ => CredentialKind::ApiKey,
        }
    }

    /// Sensitive header for exactly one attempt. The transport treats it as
    /// sensitive material: never logged, never forwarded to other origins.
    pub fn auth_header(&self, secret: &SecretMaterial) -> (String, String) {
        match self {
            Self::Bearer | Self::OAuthBearer(_) => (
                "authorization".into(),
                format!("Bearer {}", secret.expose()),
            ),
            Self::ApiKeyHeader(name) => ((*name).into(), secret.expose().into()),
        }
    }
}

/// Opaque credential locator carried by compiled topology. It is not secret
/// material and must not be exposed downstream.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CredentialBindingId(String);

impl CredentialBindingId {
    pub fn new(id: &str) -> Result<Self, ProviderError> {
        if ident_ok(id) {
            Ok(Self(id.into()))
        } else {
            Err(ProviderError::InvalidBinding)
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Secret bytes for one credential binding. Construction rejects empty material
/// and anything outside printable ASCII so a secret can never inject headers.
/// Debug output is redacted.
pub struct SecretMaterial(SecretString);

impl SecretMaterial {
    pub fn new(secret: &str) -> Result<Self, ProviderError> {
        if secret.is_empty()
            || secret.len() > 16384
            || !secret.chars().all(|c| c.is_ascii_graphic())
        {
            return Err(ProviderError::InvalidSecret);
        }
        Ok(Self(SecretString::from(secret)))
    }

    /// Name makes secret reads visible in review; only auth assembly uses it.
    pub fn expose(&self) -> &str {
        self.0.expose_secret()
    }
}

impl std::fmt::Debug for SecretMaterial {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SecretMaterial([redacted])")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auth_header_shapes_are_fixed_per_scheme() {
        let secret = SecretMaterial::new("sk-test-0001").unwrap();
        assert_eq!(
            AuthScheme::Bearer.auth_header(&secret),
            (
                "authorization".to_string(),
                "Bearer sk-test-0001".to_string()
            )
        );
        assert_eq!(
            AuthScheme::ApiKeyHeader("api-key").auth_header(&secret),
            ("api-key".to_string(), "sk-test-0001".to_string())
        );
    }

    #[test]
    fn secret_material_rejects_injection_and_redacts_debug() {
        assert!(SecretMaterial::new("").is_err());
        assert!(SecretMaterial::new("sk\r\nX-Injected: 1").is_err());
        assert!(SecretMaterial::new("sk with space").is_err());
        assert!(SecretMaterial::new("sk-密钥").is_err());
        let secret = SecretMaterial::new("sk-test-0001").unwrap();
        let rendered = format!("{secret:?}");
        assert!(!rendered.contains("sk-test-0001"));
        assert!(rendered.contains("redacted"));
    }

    #[test]
    fn credential_binding_ids_follow_ident_grammar() {
        assert_eq!(
            CredentialBindingId::new("deepseek-api-key")
                .unwrap()
                .as_str(),
            "deepseek-api-key"
        );
        assert!(CredentialBindingId::new("").is_err());
        assert!(CredentialBindingId::new("DeepSeek Key").is_err());
    }
}
