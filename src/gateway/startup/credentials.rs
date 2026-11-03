//! Trusted credential sources; no account selection from business data.
#[cfg(test)]
mod binding_tests;
use crate::{
    account::{AccountCredential, BoundAccount, BoundAccounts},
    credential::{AccessBinding, AccessGrant, ApiKeyAccess},
    gateway::StartupError,
    provider::{CredentialBindingId, CredentialKind, ProviderDefinition, SecretMaterial},
};
use std::{collections::BTreeMap, sync::Arc};

#[derive(Clone)]
pub(in crate::gateway) enum Source {
    ApiKey(Arc<SecretMaterial>),
    ManagedKey(ApiKeyAccess),
    Account(AccessBinding),
}
#[derive(Default)]
pub struct Credentials {
    bindings: BTreeMap<CredentialBindingId, Binding>,
    task_accounts: BTreeMap<String, CredentialBindingId>,
    pub(in crate::gateway) oauth_accounts: Vec<AccessBinding>,
}
pub(in crate::gateway) struct Binding {
    pub source: Source,
    pub provider: Option<ProviderDefinition>,
    pub name: Option<String>,
}
impl Binding {
    fn single(source: Source) -> Self {
        Self {
            source,
            provider: None,
            name: None,
        }
    }
}
impl From<BTreeMap<CredentialBindingId, Arc<SecretMaterial>>> for Credentials {
    fn from(keys: BTreeMap<CredentialBindingId, Arc<SecretMaterial>>) -> Self {
        Self {
            bindings: keys
                .into_iter()
                .map(|(id, key)| (id, Binding::single(Source::ApiKey(key))))
                .collect(),
            task_accounts: BTreeMap::new(),
            oauth_accounts: vec![],
        }
    }
}
impl From<&BoundAccounts> for Credentials {
    fn from(accounts: &BoundAccounts) -> Self {
        let mut credentials = Self::new();
        for account in accounts.all() {
            if let AccountCredential::OAuth(binding) = &account.credential {
                credentials.retain_oauth(binding);
            }
            if account.enabled {
                credentials.insert_configured(account.binding_id(), account);
            }
        }
        credentials
    }
}
impl Credentials {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn insert(&mut self, id: CredentialBindingId, secret: Arc<SecretMaterial>) {
        self.bindings
            .insert(id, Binding::single(Source::ApiKey(secret)));
    }
    pub fn insert_managed_key(&mut self, id: CredentialBindingId, binding: ApiKeyAccess) {
        self.bindings
            .insert(id, Binding::single(Source::ManagedKey(binding)));
    }
    pub fn insert_account(&mut self, id: CredentialBindingId, binding: AccessBinding) {
        self.retain_oauth(&binding);
        self.bindings
            .insert(id, Binding::single(Source::Account(binding)));
    }
    fn retain_oauth(&mut self, binding: &AccessBinding) {
        if !self
            .oauth_accounts
            .iter()
            .any(|other| binding.same_account(other))
        {
            self.oauth_accounts.push(binding.clone());
        }
    }
    fn insert_configured(&mut self, id: CredentialBindingId, account: &BoundAccount) {
        let source = match &account.credential {
            AccountCredential::ApiKey(binding) => Source::ManagedKey(binding.clone()),
            AccountCredential::OAuth(binding) => Source::Account(binding.clone()),
        };
        self.bindings.insert(
            id,
            Binding {
                source,
                provider: Some(account.provider.clone()),
                name: Some(account.name.clone()),
            },
        );
    }
    pub(in crate::gateway) fn get(&self, id: &CredentialBindingId) -> Option<&Binding> {
        self.bindings.get(id)
    }
    pub(super) fn select_task(&mut self, model: String, id: CredentialBindingId) {
        self.task_accounts.insert(model, id);
    }
    pub(in crate::gateway) fn task(
        &self,
        model: &str,
        default: &CredentialBindingId,
    ) -> Option<&Binding> {
        self.get(self.task_accounts.get(model).unwrap_or(default))
    }
}
fn unavailable(_error: crate::credential::CredentialError) -> crate::gateway::ApiError {
    crate::gateway::ApiError::new(
        axum::http::StatusCode::SERVICE_UNAVAILABLE,
        crate::gateway::error::ErrorKind::CredentialUnavailable,
    )
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
    /// Lifecycle eligibility is a local veto, not an upstream health observation.
    pub(in crate::gateway) async fn availability_for_request(
        &self,
    ) -> Result<Option<u64>, crate::gateway::ApiError> {
        if matches!(self, Self::ApiKey(_)) {
            return Ok(None);
        }
        let source = self.clone();
        tokio::task::spawn_blocking(move || match source {
            Self::ApiKey(_) => Ok(None),
            Self::ManagedKey(binding) => binding.borrow().map(|_| None).map_err(unavailable),
            Self::Account(binding) => binding.material_revision().map(Some).map_err(unavailable),
        })
        .await
        .map_err(|_| crate::gateway::ApiError::upstream())?
    }
    pub(in crate::gateway) fn check(
        &self,
        kind: CredentialKind,
        domain: &str,
    ) -> Result<(), StartupError> {
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
    pub(in crate::gateway) fn provenance(&self) -> String {
        match self {
            Self::ApiKey(secret) => secret.expose().into(),
            Self::Account(binding) => binding.provenance(),
            Self::ManagedKey(binding) => binding.provenance(),
        }
    }
    /// Request-time resolution. Store-backed sources read private files under a
    /// briefly contended lock, so they run off the async workers.
    pub(in crate::gateway) async fn resolve_for_request(
        &self,
    ) -> Result<(Arc<SecretMaterial>, Option<AccessGrant>), crate::gateway::ApiError> {
        if let Self::ApiKey(secret) = self {
            return Ok((secret.clone(), None));
        }
        let source = self.clone();
        tokio::task::spawn_blocking(move || source.resolve())
            .await
            .map_err(|_| crate::gateway::ApiError::upstream())?
    }
    pub(in crate::gateway) fn resolve(
        &self,
    ) -> Result<(Arc<SecretMaterial>, Option<AccessGrant>), crate::gateway::ApiError> {
        match self {
            Self::ApiKey(secret) => Ok((secret.clone(), None)),
            Self::ManagedKey(binding) => {
                let key = binding.borrow().map_err(unavailable)?;
                let secret = SecretMaterial::new(key.expose())
                    .map_err(|_| crate::gateway::ApiError::upstream())?;
                Ok((Arc::new(secret), None))
            }
            Self::Account(binding) => {
                let grant = binding.borrow().map_err(unavailable)?;
                let secret = SecretMaterial::new(grant.access.expose())
                    .map_err(|_| crate::gateway::ApiError::upstream())?;
                Ok((Arc::new(secret), Some(grant)))
            }
        }
    }
}
