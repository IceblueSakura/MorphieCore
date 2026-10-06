//! Trusted provider definitions: fixed origins, entry paths, auth scheme and
//! failure classification.
//!
//! A provider definition is compile-time topology input. No business request can
//! select an origin, path or credential, and provider code never mutates encoded
//! task semantics.

pub mod auth;
pub mod catalog;
pub mod definition;
pub mod errors;
pub(crate) mod subscription;

pub use auth::{AuthScheme, CredentialBindingId, CredentialKind, SecretMaterial};
pub use definition::{EndpointPath, ProviderDefinition, ProviderId, TrustedOrigin};
pub use errors::{ErrorClass, ProviderError, StatusClass, classify_status};

/// Shared identity grammar for provider, endpoint, route and model labels.
pub(crate) fn ident_ok(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '-' | '_' | '.'))
}
