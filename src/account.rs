//! Trusted account declarations shared by startup, provisioning and diagnostics.
//! Secret material and verified identities remain owned by the credential store.
use crate::{
    credential::{AccessBinding, ApiKeyAccess, CredentialError, CredentialManager},
    provider::{CredentialKind, ProviderDefinition, catalog},
    semantic::value::{JsonLimits, parse_json},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    sync::Arc,
};

#[derive(Debug, thiserror::Error)]
pub enum AccountError {
    #[error("invalid or unsafe account configuration")]
    Configuration,
    #[error("unknown Provider: {0}")]
    Provider(String),
    #[error("invalid account declaration for Provider {provider}, account {account}")]
    Declaration { provider: String, account: String },
    #[error("Provider {provider}, account {account}: {cause}")]
    Credential {
        provider: String,
        account: String,
        #[source]
        cause: CredentialError,
    },
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CredentialRef {
    ApiKey {
        alias: String,
    },
    #[serde(rename = "oauth")]
    OAuth {
        profile: String,
        alias: String,
    },
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Account {
    pub name: String,
    pub enabled: bool,
    pub credential: CredentialRef,
    pub base_url: Option<String>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderAccounts {
    pub accounts: Vec<Account>,
    #[serde(default)]
    pub recovery: crate::provider::RecoveryPolicy,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Accounts {
    pub version: u32,
    pub providers: BTreeMap<String, ProviderAccounts>,
}

#[derive(Clone)]
pub enum AccountCredential {
    ApiKey(ApiKeyAccess),
    OAuth(AccessBinding),
}

pub struct BoundAccount {
    pub name: String,
    pub enabled: bool,
    pub provider: ProviderDefinition,
    pub credential: AccountCredential,
}

pub struct BoundAccounts(BTreeMap<String, Vec<Arc<BoundAccount>>>);

impl BoundAccount {
    pub fn binding_id(&self) -> crate::provider::CredentialBindingId {
        use sha2::{Digest, Sha256};
        let mut hash = Sha256::new();
        for value in [self.provider.id.as_str(), self.name.as_str()] {
            hash.update((value.len() as u64).to_le_bytes());
            hash.update(value.as_bytes());
        }
        let id: String = hash
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        crate::provider::CredentialBindingId::new(&id).expect("fixed account binding digest")
    }
}

impl Account {
    fn invalid(&self, provider: &str) -> AccountError {
        AccountError::Declaration {
            provider: provider.into(),
            account: self.name.clone(),
        }
    }
    pub fn target(
        &self,
        provider: &ProviderDefinition,
    ) -> Result<ProviderDefinition, AccountError> {
        match &self.base_url {
            None => Ok(provider.clone()),
            Some(root) if provider.auth.kind() == CredentialKind::ApiKey => provider
                .with_api_root(root)
                .map_err(|_| self.invalid(provider.id.as_str())),
            Some(_) => Err(self.invalid(provider.id.as_str())),
        }
    }
    pub fn bind(
        &self,
        provider: &ProviderDefinition,
        manager: &CredentialManager,
    ) -> Result<BoundAccount, AccountError> {
        crate::credential::valid_alias(&self.name).map_err(|_| AccountError::Configuration)?;
        match (&self.credential, provider.auth.kind()) {
            (CredentialRef::ApiKey { alias }, CredentialKind::ApiKey) => {
                crate::credential::valid_alias(alias)
                    .map_err(|_| self.invalid(provider.id.as_str()))?;
            }
            (CredentialRef::OAuth { profile, alias }, CredentialKind::OAuth(expected))
                if profile == expected =>
            {
                crate::credential::valid_alias(alias)
                    .map_err(|_| self.invalid(provider.id.as_str()))?;
            }
            _ => return Err(self.invalid(provider.id.as_str())),
        }
        let target = self.target(provider)?;
        let credential = match &self.credential {
            CredentialRef::ApiKey { alias } => manager
                .bind_key_reference(provider.id.as_str(), alias)
                .map(AccountCredential::ApiKey),
            CredentialRef::OAuth { profile, alias } => manager
                .bind_access(profile, alias)
                .map(AccountCredential::OAuth),
        }
        .map_err(|cause| AccountError::Credential {
            provider: provider.id.as_str().into(),
            account: self.name.clone(),
            cause,
        })?;
        Ok(BoundAccount {
            name: self.name.clone(),
            enabled: self.enabled,
            provider: target,
            credential,
        })
    }
}

impl Accounts {
    pub fn load(path: &Path) -> Result<Self, AccountError> {
        let bytes = crate::credential::read_private_file(path, 65536)
            .map_err(|_| AccountError::Configuration)?;
        let value = parse_json(
            &bytes,
            JsonLimits {
                bytes: 65536,
                depth: 16,
                nodes: 16384,
            },
        )
        .map_err(|_| AccountError::Configuration)?;
        let accounts: Self =
            serde_json::from_value(value).map_err(|_| AccountError::Configuration)?;
        accounts.validate()?;
        Ok(accounts)
    }
    pub fn validate(&self) -> Result<(), AccountError> {
        if self.version != 1 || self.providers.len() > 64 {
            return Err(AccountError::Configuration);
        }
        for (id, configured) in &self.providers {
            let provider = catalog::get(id).ok_or_else(|| AccountError::Provider(id.clone()))?;
            if configured.accounts.is_empty() || configured.accounts.len() > 64 {
                return Err(AccountError::Configuration);
            }
            let mut names = BTreeSet::new();
            for account in &configured.accounts {
                crate::credential::valid_alias(&account.name)
                    .map_err(|_| AccountError::Configuration)?;
                if !names.insert(&account.name) {
                    return Err(account.invalid(id));
                }
                let alias = match (&account.credential, provider.auth.kind()) {
                    (CredentialRef::ApiKey { alias }, CredentialKind::ApiKey) => alias,
                    (CredentialRef::OAuth { profile, alias }, CredentialKind::OAuth(expected))
                        if profile == expected =>
                    {
                        alias
                    }
                    _ => return Err(account.invalid(id)),
                };
                crate::credential::valid_alias(alias).map_err(|_| account.invalid(id))?;
                account.target(provider)?;
            }
        }
        Ok(())
    }
    pub fn account(
        &self,
        provider: &str,
        name: &str,
    ) -> Result<(&ProviderDefinition, &Account), AccountError> {
        let definition =
            catalog::get(provider).ok_or_else(|| AccountError::Provider(provider.into()))?;
        let account = self
            .providers
            .get(provider)
            .and_then(|p| p.accounts.iter().find(|a| a.name == name))
            .ok_or_else(|| AccountError::Declaration {
                provider: provider.into(),
                account: name.into(),
            })?;
        Ok((definition, account))
    }
    pub fn bind(&self, manager: &CredentialManager) -> Result<BoundAccounts, AccountError> {
        self.validate()?;
        let mut providers = BTreeMap::new();
        for (id, configured) in &self.providers {
            let provider = catalog::get(id).ok_or_else(|| AccountError::Provider(id.clone()))?;
            let mut subjects = BTreeSet::new();
            let mut bound = Vec::new();
            for account in &configured.accounts {
                let binding = account.bind(provider, manager)?;
                if let AccountCredential::OAuth(access) = &binding.credential
                    && !subjects.insert(access.subject().to_owned())
                {
                    return Err(account.invalid(id));
                }
                bound.push(Arc::new(binding));
            }
            providers.insert(id.clone(), bound);
        }
        Ok(BoundAccounts(providers))
    }
}

impl BoundAccounts {
    pub fn provider(&self, provider: &str) -> &[Arc<BoundAccount>] {
        self.0.get(provider).map_or(&[], Vec::as_slice)
    }
    pub fn all(&self) -> impl Iterator<Item = &Arc<BoundAccount>> {
        self.0.values().flatten()
    }
    pub fn account(&self, provider: &str, name: &str) -> Option<&Arc<BoundAccount>> {
        self.provider(provider)
            .iter()
            .find(|account| account.name == name)
    }
}

#[cfg(test)]
mod tests;
