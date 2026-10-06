//! Ordered credential references and local availability; execution owns retries and I/O.
use super::{
    AccessBinding, ApiKeyAccess, CredentialError as Error, CredentialManager, model::valid_alias,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const MAX_POOL_MEMBERS: usize = 64;
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
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
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CredentialPool {
    pub members: Vec<CredentialRef>,
    #[serde(default)]
    pub fallback: bool,
    pub max_attempts: usize,
}
impl CredentialPool {
    pub fn validate(&self) -> Result<(), Error> {
        if self.members.is_empty()
            || self.members.len() > MAX_POOL_MEMBERS
            || !(1..=MAX_POOL_MEMBERS).contains(&self.max_attempts)
            || (self
                .members
                .iter()
                .any(|m| matches!(m, CredentialRef::OAuth { profile, .. } if profile == "siwc"))
                && (self.members.len() != 1 || self.fallback || self.max_attempts != 1))
        {
            return Err(Error::InvalidInput);
        }
        let mut seen = BTreeSet::new();
        for member in &self.members {
            if !seen.insert(member) {
                return Err(Error::InvalidInput);
            }
            match member {
                CredentialRef::ApiKey { alias } => valid_alias(alias)?,
                CredentialRef::OAuth { profile, alias } => {
                    super::model::valid_profile(profile)?;
                    valid_alias(alias)?;
                }
            }
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PoolStatus {
    pub revision: u64,
    pub config: CredentialPool,
}
#[derive(Clone)]
pub enum PoolMember {
    ApiKey(ApiKeyAccess),
    OAuth(AccessBinding),
}
pub struct PoolAccess {
    pub members: Vec<PoolMember>,
    pub fallback: bool,
    pub max_attempts: usize,
}
impl CredentialManager {
    /// A binding name is trusted deployment configuration, never a business field.
    pub fn set_pool(
        &self,
        provider: &str,
        binding: &str,
        revision: u64,
        config: CredentialPool,
    ) -> Result<PoolStatus, Error> {
        super::model::valid_profile(provider)?;
        valid_alias(binding)?;
        config.validate()?;
        let tx = self.store.transaction()?;
        let mut document = tx.provider(provider)?;
        let current = document.pools.get(binding).map_or(0, |p| p.revision);
        if current != revision {
            return Err(Error::Superseded);
        }
        // Validate references without borrowing or refreshing access material.
        for member in &config.members {
            match member {
                CredentialRef::ApiKey { alias } => {
                    if !document.api_keys.contains_key(alias) {
                        return Err(Error::KeyUnavailable);
                    }
                }
                CredentialRef::OAuth { profile, alias } => {
                    if self.load(&tx, profile, alias)?.account.is_none() {
                        return Err(Error::LoginRequired);
                    }
                }
            }
        }
        let status = PoolStatus {
            revision: revision.checked_add(1).ok_or(Error::Storage)?,
            config,
        };
        document.pools.insert(binding.into(), status.clone());
        tx.publish_provider(&mut document)?;
        Ok(status)
    }
    pub fn pools(&self) -> Result<Vec<(String, String, PoolStatus)>, Error> {
        let tx = self.store.transaction()?;
        let mut pools = Vec::new();
        for provider in tx.namespaces()? {
            for (binding, pool) in tx.provider(&provider)?.pools {
                pools.push((provider.clone(), binding, pool));
            }
        }
        Ok(pools)
    }
    pub fn bind_pool(&self, provider: &str, config: &CredentialPool) -> Result<PoolAccess, Error> {
        config.validate()?;
        let mut members = Vec::new();
        for member in &config.members {
            // A disabled key remains a pinned candidate; borrow rejects it. It is
            // never silently replaced by a new generation after activation.
            members.push(match member {
                CredentialRef::ApiKey { alias } => {
                    PoolMember::ApiKey(self.bind_pool_key(provider, alias)?)
                }
                CredentialRef::OAuth { profile, alias } => {
                    PoolMember::OAuth(self.bind_access(profile, alias)?)
                }
            });
        }
        Ok(PoolAccess {
            members,
            fallback: config.fallback,
            max_attempts: config.max_attempts,
        })
    }
}
