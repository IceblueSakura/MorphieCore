//! Trusted credential sources; no account selection from business data.
#[cfg(test)]
#[path = "credential_binding_tests.rs"]
mod binding_tests;
use super::StartupError;
use crate::{
    credential::{AccessBinding, AccessGrant, ApiKeyAccess},
    provider::{CredentialBindingId, CredentialKind, SecretMaterial},
};
use std::{collections::BTreeMap, sync::Arc};

#[derive(Clone)]
pub(super) enum Source {
    ApiKey(Arc<SecretMaterial>),
    ManagedKey(ApiKeyAccess),
    Account(AccessBinding),
}
#[derive(Default)]
pub struct Credentials(BTreeMap<CredentialBindingId, Binding>);
pub(super) struct Binding {
    pub sources: Vec<Source>,
    pub fallback: bool,
    pub max_attempts: usize,
}
impl Binding {
    fn single(source: Source) -> Self {
        Self {
            sources: vec![source],
            fallback: false,
            max_attempts: 1,
        }
    }
}
impl From<BTreeMap<CredentialBindingId, Arc<SecretMaterial>>> for Credentials {
    fn from(keys: BTreeMap<CredentialBindingId, Arc<SecretMaterial>>) -> Self {
        Self(
            keys.into_iter()
                .map(|(id, key)| (id, Binding::single(Source::ApiKey(key))))
                .collect(),
        )
    }
}
impl Credentials {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn insert(&mut self, id: CredentialBindingId, secret: Arc<SecretMaterial>) {
        self.0.insert(id, Binding::single(Source::ApiKey(secret)));
    }
    pub fn insert_managed_key(&mut self, id: CredentialBindingId, binding: ApiKeyAccess) {
        self.0
            .insert(id, Binding::single(Source::ManagedKey(binding)));
    }
    pub fn insert_account(&mut self, id: CredentialBindingId, binding: AccessBinding) {
        self.0.insert(id, Binding::single(Source::Account(binding)));
    }
    pub fn insert_pool(
        &mut self,
        id: CredentialBindingId,
        pool: crate::credential::PoolAccess,
    ) -> Result<(), StartupError> {
        if pool.members.is_empty()
            || pool.members.len() > crate::credential::MAX_POOL_MEMBERS
            || !(1..=64).contains(&pool.max_attempts)
            || (pool.members.iter().any(
                |m| matches!(m, crate::credential::PoolMember::OAuth(a) if a.profile() == "siwc"),
            ) && (pool.members.len() != 1 || pool.fallback || pool.max_attempts != 1))
        {
            return Err(StartupError::Credentials);
        }
        let sources: Vec<_> = pool
            .members
            .into_iter()
            .map(|member| match member {
                crate::credential::PoolMember::ApiKey(key) => Source::ManagedKey(key),
                crate::credential::PoolMember::OAuth(account) => Source::Account(account),
            })
            .collect();
        let mut seen = std::collections::BTreeSet::new();
        if sources.iter().any(|s| !seen.insert(s.provenance())) {
            return Err(StartupError::Credentials);
        }
        self.0.insert(
            id,
            Binding {
                sources,
                fallback: pool.fallback,
                max_attempts: pool.max_attempts,
            },
        );
        Ok(())
    }
    pub(super) fn get(&self, id: &CredentialBindingId) -> Option<&Binding> {
        self.0.get(id)
    }
}
fn unavailable(error: crate::credential::CredentialError) -> super::ApiError {
    use crate::credential::CredentialError as Error;
    let mut response = super::ApiError::new(
        axum::http::StatusCode::SERVICE_UNAVAILABLE,
        "credential_unavailable",
    );
    if matches!(
        error,
        Error::KeyUnavailable | Error::LoginRequired | Error::Expired
    ) {
        response.fallback = Some(crate::provider::ErrorClass::CredentialUnavailable);
    }
    response
}
fn check_profile(kind: CredentialKind, profile: Option<&str>) -> Result<(), StartupError> {
    match (kind, profile) {
        (CredentialKind::ApiKey, None) => Ok(()),
        (CredentialKind::OAuth(expected), Some(actual)) if actual == expected => Ok(()),
        _ => Err(StartupError::Credentials),
    }
}

#[cfg(test)]
#[test]
fn credential_kind_and_authorization_profile_must_both_match() {
    for (kind, profile, accepted) in [
        (CredentialKind::ApiKey, None, true),
        (CredentialKind::ApiKey, Some("first"), false),
        (CredentialKind::OAuth("first"), None, false),
        (CredentialKind::OAuth("first"), Some("first"), true),
        (CredentialKind::OAuth("first"), Some("second"), false),
    ] {
        assert_eq!(check_profile(kind, profile).is_ok(), accepted);
    }
}

impl Source {
    pub(super) fn check(&self, kind: CredentialKind, domain: &str) -> Result<(), StartupError> {
        if let Self::ManagedKey(binding) = self
            && binding.domain() != domain
        {
            return Err(StartupError::Credentials);
        }
        let profile = match self {
            Self::ApiKey(_) | Self::ManagedKey(_) => None,
            Self::Account(binding) => Some(binding.profile()),
        };
        check_profile(kind, profile)
    }
    pub(super) fn provenance(&self) -> String {
        match self {
            Self::ApiKey(secret) => secret.expose().into(),
            Self::Account(binding) => binding.provenance(),
            Self::ManagedKey(binding) => binding.provenance(),
        }
    }
    pub(super) fn resolve(
        &self,
    ) -> Result<(Arc<SecretMaterial>, Option<AccessGrant>), super::ApiError> {
        match self {
            Self::ApiKey(secret) => Ok((secret.clone(), None)),
            Self::ManagedKey(binding) => {
                let key = binding.borrow().map_err(unavailable)?;
                let secret =
                    SecretMaterial::new(key.expose()).map_err(|_| super::ApiError::upstream())?;
                Ok((Arc::new(secret), None))
            }
            Self::Account(binding) => {
                let grant = binding.borrow().map_err(unavailable)?;
                let secret = SecretMaterial::new(grant.access.expose())
                    .map_err(|_| super::ApiError::upstream())?;
                Ok((Arc::new(secret), Some(grant)))
            }
        }
    }
}
