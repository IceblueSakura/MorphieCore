//! One structured document per provider namespace; short transactions merge records.
use super::{
    CredentialError as Error,
    model::{Account, valid_alias, valid_profile},
    pool::PoolStatus,
    storage_fs::{Directory, FileLock},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    sync::Arc,
};
const DOCUMENT_BYTES: u64 = 8 * 1024 * 1024;
const RECORDS: usize = 64;

pub(super) trait StoredDocument: Serialize + serde::de::DeserializeOwned {
    fn validate_binding(&self, namespace: &str, alias: &str) -> Result<(), Error>;
}
impl StoredDocument for Account {
    fn validate_binding(&self, namespace: &str, alias: &str) -> Result<(), Error> {
        self.validate(namespace, alias)
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ProviderDocument {
    pub provider: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host_id: Option<String>,
    #[serde(default)]
    pub revision: u64,
    #[serde(default, deserialize_with = "oauth_records")]
    pub oauth: BTreeMap<String, Value>,
    #[serde(default, deserialize_with = "key_records")]
    pub api_keys: BTreeMap<String, Value>,
    #[serde(
        default,
        deserialize_with = "serde_with::rust::maps_duplicate_key_is_error::deserialize"
    )]
    pub pools: BTreeMap<String, PoolStatus>,
}
// Typed intake rejects duplicate/unknown grant fields before Value is used as a
// transient serialization container for merging one independently owned record.
fn typed_records<'de, D: serde::Deserializer<'de>, T: Deserialize<'de> + Serialize>(
    d: D,
) -> Result<BTreeMap<String, Value>, D::Error> {
    let records: BTreeMap<String, T> =
        serde_with::rust::maps_duplicate_key_is_error::deserialize(d)?;
    records
        .into_iter()
        .map(|(key, value)| {
            serde_json::to_value(value)
                .map(|value| (key, value))
                .map_err(serde::de::Error::custom)
        })
        .collect()
}
fn oauth_records<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<BTreeMap<String, Value>, D::Error> {
    typed_records::<D, Account>(d)
}
fn key_records<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<BTreeMap<String, Value>, D::Error> {
    typed_records::<D, super::api_key::KeyRecord>(d)
}
impl ProviderDocument {
    fn empty(provider: &str) -> Self {
        Self {
            provider: provider.into(),
            host_id: None,
            revision: 0,
            oauth: BTreeMap::new(),
            api_keys: BTreeMap::new(),
            pools: BTreeMap::new(),
        }
    }
    fn validate(&self, provider: &str) -> Result<(), Error> {
        valid_profile(provider)?;
        if self.provider != provider
            || self.host_id.as_ref().is_some_and(|id| !valid_host_id(id))
            || self.oauth.len() > RECORDS
            || self.api_keys.len() > RECORDS
            || self.pools.len() > RECORDS
        {
            return Err(Error::Storage);
        }
        for (alias, value) in &self.oauth {
            let account: Account =
                serde_json::from_value(value.clone()).map_err(|_| Error::Storage)?;
            account.validate(provider, alias)?;
        }
        for (alias, value) in &self.api_keys {
            let key: super::api_key::KeyRecord =
                serde_json::from_value(value.clone()).map_err(|_| Error::Storage)?;
            key.validate_binding(provider, alias)?;
        }
        for (name, pool) in &self.pools {
            valid_alias(name)?;
            if pool.revision == 0 {
                return Err(Error::Storage);
            }
            pool.config.validate()?;
        }
        Ok(())
    }
}
#[derive(Clone, Copy)]
enum Partition {
    OAuth,
    ApiKey,
}
impl Partition {
    fn name(self) -> &'static str {
        match self {
            Self::OAuth => "oauth",
            Self::ApiKey => "key",
        }
    }
}
pub(super) struct Store {
    root: Arc<Directory>,
    partition: Partition,
    #[cfg(test)]
    fault: std::sync::atomic::AtomicU8,
}
pub(super) struct Transaction<'a> {
    store: &'a Store,
    _lock: FileLock,
}
pub(super) struct Loaded {
    pub account: Option<Account>,
    pub recovery: bool,
}
#[derive(Clone, Copy)]
#[repr(u8)]
pub(super) enum PublishStep {
    Write = 1,
    FileSync,
    Rename,
    DirectorySync,
    Cleanup,
    MarkerSync,
    MarkerDirectorySync,
    CleanupDirectorySync,
}
impl Store {
    pub fn open(root: &Path) -> Result<Arc<Self>, Error> {
        let store = Arc::new(Self {
            root: Arc::new(Directory::open(root)?),
            partition: Partition::OAuth,
            #[cfg(test)]
            fault: std::sync::atomic::AtomicU8::new(0),
        });
        let names = store.root.names()?;
        if names.iter().any(|n| n == "accounts.json") {
            return Err(Error::LegacyStore);
        }
        let _ = store.transaction()?;
        Ok(store)
    }
    pub fn key_partition(&self) -> Arc<Self> {
        Arc::new(Self {
            root: self.root.clone(),
            partition: Partition::ApiKey,
            #[cfg(test)]
            fault: std::sync::atomic::AtomicU8::new(0),
        })
    }
    #[cfg(test)]
    pub fn test_config(&self, value: &Value) {
        self.root
            .write("gateway.json", &serde_json::to_vec(value).unwrap(), || {
                Ok(())
            })
            .unwrap();
    }
    pub fn transaction(&self) -> Result<Transaction<'_>, Error> {
        Ok(Transaction {
            store: self,
            _lock: self.root.lock("store.lock")?,
        })
    }
    pub fn account_lock(&self, profile: &str, alias: &str) -> Result<FileLock, Error> {
        valid_profile(profile)?;
        valid_alias(alias)?;
        let _tx = self.transaction()?;
        // Stable operation locks never move with the replaced provider document.
        self.root
            .lock(&format!("{profile}.{}.{alias}.lock", self.partition.name()))
    }
    #[cfg(test)]
    pub fn fail_at(&self, fault: PublishStep) {
        self.fault
            .store(fault as u8, std::sync::atomic::Ordering::SeqCst);
    }
    fn checkpoint(&self, stage: PublishStep) -> Result<(), Error> {
        #[cfg(test)]
        if self
            .fault
            .compare_exchange(
                stage as u8,
                0,
                std::sync::atomic::Ordering::SeqCst,
                std::sync::atomic::Ordering::SeqCst,
            )
            .is_ok()
        {
            return Err(Error::Storage);
        }
        let _ = stage;
        Ok(())
    }
}
impl Transaction<'_> {
    pub fn host_id(&self, profile: &str) -> Result<String, Error> {
        let mut document = self.provider(profile)?;
        if let Some(id) = document.host_id {
            return Ok(id);
        }
        let mut bytes = [0u8; 16];
        getrandom::fill(&mut bytes).map_err(|_| Error::Storage)?;
        bytes[6] = (bytes[6] & 0x0f) | 0x40;
        bytes[8] = (bytes[8] & 0x3f) | 0x80;
        let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
        let id = format!(
            "urn:uuid:{}-{}-{}-{}-{}",
            &hex[..8],
            &hex[8..12],
            &hex[12..16],
            &hex[16..20],
            &hex[20..]
        );
        document.host_id = Some(id.clone());
        self.publish_provider(&mut document)?;
        Ok(id)
    }
    pub fn provider(&self, provider: &str) -> Result<ProviderDocument, Error> {
        valid_profile(provider)?;
        if matches!(provider, "accounts" | "gateway") {
            return Err(Error::InvalidInput);
        }
        let document = match self
            .store
            .root
            .read(&format!("{provider}.json"), DOCUMENT_BYTES)?
        {
            Some(bytes) => {
                serde_json::from_slice::<ProviderDocument>(&bytes).map_err(|_| Error::Storage)?
            }
            None => ProviderDocument::empty(provider),
        };
        document.validate(provider)?;
        Ok(document)
    }
    pub fn namespaces(&self) -> Result<Vec<String>, Error> {
        let mut names = BTreeSet::new();
        for name in self.store.root.names()? {
            if name == "gateway.json" {
                continue;
            }
            if let Some(namespace) = name.strip_suffix(".json") {
                valid_profile(namespace)?;
                names.insert(namespace.to_owned());
            } else if name.ends_with(".pending")
                && let Some((namespace, _)) = name.split_once('.')
            {
                valid_profile(namespace)?;
                names.insert(namespace.to_owned());
            }
        }
        if names.len() > RECORDS {
            return Err(Error::PoolFull);
        }
        Ok(names.into_iter().collect())
    }
    pub fn ensure_namespace(&self, namespace: &str) -> Result<(), Error> {
        let mut document = self.provider(namespace)?;
        if document.revision == 0 {
            self.publish_provider(&mut document)?;
        }
        Ok(())
    }
    fn records<'a>(&self, document: &'a mut ProviderDocument) -> &'a mut BTreeMap<String, Value> {
        match self.store.partition {
            Partition::OAuth => &mut document.oauth,
            Partition::ApiKey => &mut document.api_keys,
        }
    }
    fn marker(&self, provider: &str, alias: &str) -> String {
        format!("{provider}.{}.{alias}.pending", self.store.partition.name())
    }
    fn recovery(&self, provider: &str, alias: &str) -> Result<bool, Error> {
        match self.store.root.read(&self.marker(provider, alias), 16)? {
            None => Ok(false),
            Some(bytes) => match bytes.as_slice() {
                b"true" => Ok(true),
                b"false" => Ok(false),
                _ => Err(Error::Storage),
            },
        }
    }
    pub fn aliases(&self, provider: &str) -> Result<Vec<String>, Error> {
        let mut document = self.provider(provider)?;
        let mut aliases: BTreeSet<_> = self.records(&mut document).keys().cloned().collect();
        let prefix = format!("{provider}.{}.", self.store.partition.name());
        for name in self.store.root.names()? {
            if let Some(alias) = name
                .strip_prefix(&prefix)
                .and_then(|s| s.strip_suffix(".pending"))
            {
                valid_alias(alias)?;
                if self.recovery(provider, alias)? {
                    aliases.insert(alias.to_owned());
                }
            }
        }
        if aliases.len() > RECORDS {
            return Err(Error::PoolFull);
        }
        Ok(aliases.into_iter().collect())
    }
    pub fn load(&self, profile: &str, alias: &str) -> Result<Loaded, Error> {
        let (account, recovery) = self.load_document(profile, alias)?;
        Ok(Loaded { account, recovery })
    }
    pub fn load_document<T: StoredDocument>(
        &self,
        profile: &str,
        alias: &str,
    ) -> Result<(Option<T>, bool), Error> {
        valid_alias(alias)?;
        let mut document = self.provider(profile)?;
        let record = self
            .records(&mut document)
            .remove(alias)
            .map(|v| serde_json::from_value::<T>(v).map_err(|_| Error::Storage))
            .transpose()?;
        if let Some(record) = &record {
            record.validate_binding(profile, alias)?;
        }
        Ok((record, self.recovery(profile, alias)?))
    }
    #[cfg(test)]
    pub fn test_marker(&self, profile: &str, alias: &str, pending: bool) {
        self.store
            .root
            .write(
                &self.marker(profile, alias),
                if pending { b"true" } else { b"false" },
                || Ok(()),
            )
            .unwrap();
    }
    pub fn clear_unpublished(&self, profile: &str, alias: &str) -> Result<(), Error> {
        valid_alias(alias)?;
        if self
            .records(&mut self.provider(profile)?)
            .contains_key(alias)
            || !self.recovery(profile, alias)?
        {
            return Err(Error::Storage);
        }
        self.store
            .root
            .write(&self.marker(profile, alias), b"false", || Ok(()))
    }
    pub fn publish(&self, account: &Account) -> Result<(), Error> {
        self.publish_document(&account.profile, &account.alias, account)
    }
    pub fn publish_document<T: StoredDocument>(
        &self,
        profile: &str,
        alias: &str,
        record: &T,
    ) -> Result<(), Error> {
        record.validate_binding(profile, alias)?;
        let mut document = self.provider(profile)?;
        let values = self.records(&mut document);
        if !values.contains_key(alias) && values.len() >= RECORDS {
            return Err(Error::PoolFull);
        }
        let value = serde_json::to_value(record).map_err(|_| Error::Storage)?;
        if serde_json::to_vec(&value)
            .map_err(|_| Error::Storage)?
            .len()
            > 128 * 1024
        {
            return Err(Error::Storage);
        }
        values.insert(alias.into(), value);
        let before_recovery = self.recovery(profile, alias)?;
        let marker = self.marker(profile, alias);
        self.store.root.write(&marker, b"true", || Ok(()))?;
        for step in [PublishStep::MarkerSync, PublishStep::MarkerDirectorySync] {
            self.store
                .checkpoint(step)
                .map_err(|_| Error::PersistenceUncertain)?;
        }
        let result = self.publish_provider_with(&mut document, || {
            for step in [
                PublishStep::Write,
                PublishStep::FileSync,
                PublishStep::Rename,
            ] {
                self.store.checkpoint(step)?;
            }
            Ok(())
        });
        if let Err(error) = result {
            if error == Error::Storage
                && !before_recovery
                && self.store.root.write(&marker, b"false", || Ok(())).is_ok()
            {
                return Err(Error::Storage);
            }
            return Err(Error::PersistenceUncertain);
        }
        for step in [PublishStep::DirectorySync, PublishStep::Cleanup] {
            self.store
                .checkpoint(step)
                .map_err(|_| Error::PersistenceUncertain)?;
        }
        self.store
            .root
            .write(&marker, b"false", || Ok(()))
            .map_err(|_| Error::PersistenceUncertain)?;
        let _ = self.store.checkpoint(PublishStep::CleanupDirectorySync);
        Ok(())
    }
    pub fn publish_provider(&self, document: &mut ProviderDocument) -> Result<(), Error> {
        self.publish_provider_with(document, || Ok(()))
    }
    fn publish_provider_with(
        &self,
        document: &mut ProviderDocument,
        before_commit: impl FnOnce() -> Result<(), Error>,
    ) -> Result<(), Error> {
        document.validate(&document.provider)?;
        if document.revision == 0 && self.namespaces()?.len() >= RECORDS {
            return Err(Error::PoolFull);
        }
        document.revision = document.revision.checked_add(1).ok_or(Error::Storage)?;
        let bytes = super::SecretBytes::new(
            serde_json::to_vec_pretty(document).map_err(|_| Error::Storage)?,
        );
        if bytes.len() as u64 > DOCUMENT_BYTES {
            return Err(Error::Storage);
        }
        self.store.root.write(
            &format!("{}.json", document.provider),
            &bytes,
            before_commit,
        )
    }
}
pub(super) fn valid_host_id(id: &str) -> bool {
    let Some(uuid) = id.strip_prefix("urn:uuid:") else {
        return false;
    };
    uuid.len() == 36
        && uuid.bytes().enumerate().all(|(i, c)| {
            if matches!(i, 8 | 13 | 18 | 23) {
                c == b'-'
            } else {
                c.is_ascii_hexdigit()
            }
        })
        && uuid.as_bytes()[14] == b'4'
        && b"89ab".contains(&uuid.as_bytes()[19])
}
