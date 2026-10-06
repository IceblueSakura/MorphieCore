//! Operator-provisioned keys, not OAuth grants or evidence of upstream validity.
use super::{
    CredentialError as Error, CredentialManager, Secret,
    model::{random_id, valid_alias, valid_profile},
    store::{StoredDocument, Transaction},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApiKeyState {
    Enabled,
    Disabled,
    Removed,
}
/// Local state only. Neither presence nor syntactic validity verifies a key remotely.
#[derive(Debug, Serialize)]
pub struct ApiKeyStatus {
    pub domain: String,
    pub alias: String,
    pub state: ApiKeyState,
    pub revision: u64,
    pub generation: u64,
    pub recovery_required: bool,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum KeyKind {
    ApiKey,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct KeyRecord {
    kind: KeyKind,
    domain: String,
    alias: String,
    record_id: String,
    epoch: String,
    revision: u64,
    generation: u64,
    state: ApiKeyState,
    secret: Option<Secret>,
}
fn identity(value: &str) -> bool {
    value.len() == 32
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
impl StoredDocument for KeyRecord {
    fn validate_binding(&self, domain: &str, alias: &str) -> Result<(), Error> {
        if valid_profile(domain).is_err()
            || valid_alias(alias).is_err()
            || self.domain != domain
            || self.alias != alias
            || !identity(&self.record_id)
            || !identity(&self.epoch)
            || self.revision == 0
            || self.generation == 0
            || self.generation > self.revision
            || match (&self.state, &self.secret) {
                (ApiKeyState::Enabled | ApiKeyState::Disabled, Some(secret)) => !secret.valid(),
                (ApiKeyState::Removed, None) => false,
                _ => true,
            }
        {
            return Err(Error::Storage);
        }
        Ok(())
    }
}
impl KeyRecord {
    fn material_digest(&self) -> Option<[u8; 32]> {
        self.secret
            .as_ref()
            .map(|key| Sha256::digest(key.expose().as_bytes()).into())
    }
    fn status(&self, recovery_required: bool) -> ApiKeyStatus {
        ApiKeyStatus {
            domain: self.domain.clone(),
            alias: self.alias.clone(),
            state: self.state,
            revision: self.revision,
            generation: self.generation,
            recovery_required,
        }
    }
    fn advance(&mut self, material: bool) -> Result<(), Error> {
        self.revision = self.revision.checked_add(1).ok_or(Error::Storage)?;
        if material {
            self.generation = self.generation.checked_add(1).ok_or(Error::Storage)?;
        }
        // Disabling must invalidate old bindings even after a later re-enable.
        self.epoch = random_id()?;
        Ok(())
    }
}

/// Pins domain, record and epoch. Replacing a key requires explicit rebinding;
/// an alias is never evidence that the new key shares an upstream principal.
#[derive(Clone)]
pub struct ApiKeyAccess {
    manager: CredentialManager,
    domain: String,
    alias: String,
    record_id: String,
    epoch: String,
    generation: u64,
    material: Option<[u8; 32]>,
}
impl std::fmt::Debug for ApiKeyAccess {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ApiKeyAccess([private])")
    }
}
impl ApiKeyAccess {
    pub fn domain(&self) -> &str {
        &self.domain
    }
    pub(crate) fn provenance(&self) -> String {
        // Internal only: changed material cannot inherit an old replay scope even
        // if an out-of-band file edit failed to advance lifecycle metadata.
        let material: String = self
            .material
            .iter()
            .flatten()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        format!(
            "api-key:{}:{}:{}:{}",
            self.domain, self.record_id, self.epoch, material
        )
    }
    pub fn borrow(&self) -> Result<Secret, Error> {
        let tx = self.manager.keys.transaction()?;
        let (record, recovery) = tx.load_document::<KeyRecord>(&self.domain, &self.alias)?;
        let record = record.ok_or(Error::KeyUnavailable)?;
        if recovery
            || record.state != ApiKeyState::Enabled
            || record.record_id != self.record_id
            || record.epoch != self.epoch
            || record.generation != self.generation
            || record.material_digest() != self.material
        {
            return Err(Error::KeyUnavailable);
        }
        record.secret.ok_or(Error::KeyUnavailable)
    }
}
impl CredentialManager {
    pub fn add_api_key(
        &self,
        domain: &str,
        alias: &str,
        secret: Secret,
    ) -> Result<ApiKeyStatus, Error> {
        valid_profile(domain)?;
        valid_alias(alias)?;
        // Hold the key-store transaction while bounding/creating a new domain.
        {
            let tx = self.keys.transaction()?;
            let domains = tx.namespaces()?;
            if !domains.iter().any(|d| d == domain) && domains.len() >= 64 {
                return Err(Error::PoolFull);
            }
            tx.ensure_namespace(domain)?;
        }
        let _lock = self.keys.account_lock(domain, alias)?;
        let tx = self.keys.transaction()?;
        let (existing, recovery) = tx.load_document::<KeyRecord>(domain, alias)?;
        if recovery {
            return Err(Error::PersistenceUncertain);
        }
        if existing.is_some() {
            return Err(Error::KeyExists);
        }
        let record = KeyRecord {
            kind: KeyKind::ApiKey,
            domain: domain.into(),
            alias: alias.into(),
            record_id: random_id()?,
            epoch: random_id()?,
            revision: 1,
            generation: 1,
            state: ApiKeyState::Enabled,
            secret: Some(secret),
        };
        tx.publish_document(domain, alias, &record)?;
        Ok(record.status(false))
    }
    pub fn replace_api_key(
        &self,
        domain: &str,
        alias: &str,
        revision: u64,
        secret: Secret,
    ) -> Result<ApiKeyStatus, Error> {
        self.mutate_key(domain, alias, revision, |record, _| {
            record.advance(true)?;
            record.secret = Some(secret);
            record.state = ApiKeyState::Enabled;
            Ok(())
        })
    }
    pub fn set_api_key_enabled(
        &self,
        domain: &str,
        alias: &str,
        revision: u64,
        enabled: bool,
    ) -> Result<ApiKeyStatus, Error> {
        self.mutate_key(domain, alias, revision, |record, recovery| {
            if recovery {
                return Err(Error::PersistenceUncertain);
            }
            if record.secret.is_none() {
                return Err(Error::KeyUnavailable);
            }
            record.advance(false)?;
            record.state = if enabled {
                ApiKeyState::Enabled
            } else {
                ApiKeyState::Disabled
            };
            Ok(())
        })
    }
    /// Removes local material, retaining a tombstone and revision. Never remote revocation.
    pub fn remove_api_key(
        &self,
        domain: &str,
        alias: &str,
        revision: u64,
    ) -> Result<ApiKeyStatus, Error> {
        let _lock = self.existing_key_lock(domain, alias)?;
        let tx = self.keys.transaction()?;
        let (record, recovery) = tx.load_document::<KeyRecord>(domain, alias)?;
        if record.is_none() && recovery && revision == 0 {
            tx.clear_unpublished(domain, alias)?;
            return Ok(ApiKeyStatus {
                domain: domain.into(),
                alias: alias.into(),
                state: ApiKeyState::Removed,
                revision: 0,
                generation: 0,
                recovery_required: false,
            });
        }
        update(
            &tx,
            domain,
            alias,
            revision,
            record,
            recovery,
            |record, _| {
                record.advance(true)?;
                record.secret = None;
                record.state = ApiKeyState::Removed;
                Ok(())
            },
        )
    }
    fn existing_key_lock(
        &self,
        domain: &str,
        alias: &str,
    ) -> Result<super::storage_fs::FileLock, Error> {
        {
            let tx = self.keys.transaction()?;
            let (record, recovery) = tx.load_document::<KeyRecord>(domain, alias)?;
            if record.is_none() && !recovery {
                return Err(Error::KeyUnavailable);
            }
        }
        self.keys.account_lock(domain, alias)
    }
    fn mutate_key(
        &self,
        domain: &str,
        alias: &str,
        revision: u64,
        change: impl FnOnce(&mut KeyRecord, bool) -> Result<(), Error>,
    ) -> Result<ApiKeyStatus, Error> {
        let _lock = self.existing_key_lock(domain, alias)?;
        let tx = self.keys.transaction()?;
        let (record, recovery) = tx.load_document::<KeyRecord>(domain, alias)?;
        update(&tx, domain, alias, revision, record, recovery, change)
    }
    pub fn list_api_keys(&self, domain: Option<&str>) -> Result<Vec<ApiKeyStatus>, Error> {
        let tx = self.keys.transaction()?;
        let domains = match domain {
            Some(domain) => {
                valid_profile(domain)?;
                vec![domain.to_owned()]
            }
            None => tx.namespaces()?,
        };
        let mut statuses = Vec::new();
        for domain in domains {
            for alias in tx.aliases(&domain)? {
                let (record, recovery) = tx.load_document::<KeyRecord>(&domain, &alias)?;
                if let Some(record) = record {
                    statuses.push(record.status(recovery));
                } else if recovery {
                    statuses.push(ApiKeyStatus {
                        domain: domain.clone(),
                        alias,
                        state: ApiKeyState::Removed,
                        revision: 0,
                        generation: 0,
                        recovery_required: true,
                    });
                }
            }
        }
        Ok(statuses)
    }
    pub(super) fn bind_pool_key(&self, domain: &str, alias: &str) -> Result<ApiKeyAccess, Error> {
        self.key_access(domain, alias, false)
    }
    pub fn bind_api_key(&self, domain: &str, alias: &str) -> Result<ApiKeyAccess, Error> {
        self.key_access(domain, alias, true)
    }
    fn key_access(
        &self,
        domain: &str,
        alias: &str,
        require_enabled: bool,
    ) -> Result<ApiKeyAccess, Error> {
        let tx = self.keys.transaction()?;
        let (record, recovery) = tx.load_document::<KeyRecord>(domain, alias)?;
        let record = record.ok_or(Error::KeyUnavailable)?;
        if require_enabled && (recovery || record.state != ApiKeyState::Enabled) {
            return Err(Error::KeyUnavailable);
        }
        let material = record.material_digest();
        Ok(ApiKeyAccess {
            material,
            manager: self.clone(),
            domain: domain.into(),
            alias: alias.into(),
            record_id: record.record_id,
            epoch: record.epoch,
            generation: record.generation,
        })
    }
}
fn update(
    tx: &Transaction<'_>,
    domain: &str,
    alias: &str,
    revision: u64,
    record: Option<KeyRecord>,
    recovery: bool,
    change: impl FnOnce(&mut KeyRecord, bool) -> Result<(), Error>,
) -> Result<ApiKeyStatus, Error> {
    let mut record = record.ok_or(Error::KeyUnavailable)?;
    if record.revision != revision {
        return Err(Error::Superseded);
    }
    change(&mut record, recovery)?;
    tx.publish_document(domain, alias, &record)?;
    Ok(record.status(false))
}
