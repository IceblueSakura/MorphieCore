//! Independent persistence fault boundaries; no authority traffic.
use crate::credential::{
    model::{Account, AccountState},
    storage::store::{PublishStep, Store},
    tests::support::Directory,
    *,
};
fn active() -> Account {
    let mut account = Account::new("synthetic", "one", "synthetic-client").unwrap();
    account.identity = Some(VerifiedIdentity {
        subject: "synthetic-person".into(),
        scope: None,
    });
    account
        .replace_credential(Some(Credential {
            access: Secret::new("old-access".into()).unwrap(),
            refresh: Some(Secret::new("old-refresh".into()).unwrap()),
            id_token: None,
            expires_at: None,
            scopes: None,
        }))
        .unwrap();
    account.state = AccountState::Active;
    account.advance().unwrap();
    account
}
#[test]
fn provider_document_merges_independent_records_without_losing_other_credentials() {
    let dir = Directory::new();
    let manager = CredentialManager::new(&dir.path, vec![]).unwrap();
    manager
        .store
        .transaction()
        .unwrap()
        .publish(active())
        .unwrap();
    manager
        .add_api_key(
            "synthetic",
            "backup",
            Secret::new("synthetic-key".into()).unwrap(),
        )
        .unwrap();
    {
        let tx = manager.store.transaction().unwrap();
        let mut account = tx.load("synthetic", "one").unwrap().account.unwrap();
        assert_eq!(
            account.credential.as_ref().unwrap().access.expose(),
            "old-access"
        );
        account.credential.as_mut().unwrap().access = Secret::new("next-access".into()).unwrap();
        account.advance().unwrap();
        tx.publish(account).unwrap();
    }
    let file = dir.path.join("synthetic.json");
    let doc: serde_json::Value = serde_json::from_slice(&std::fs::read(file).unwrap()).unwrap();
    assert_eq!(doc["provider"], "synthetic");
    assert_eq!(doc["oauth"]["one"]["credential"]["access"], "next-access");
    assert_eq!(doc["api_keys"]["backup"]["secret"], "synthetic-key");
    assert!(doc.get("pools").is_none());
}
#[test]
fn malformed_records_in_either_partition_reject_before_credentials_are_available() {
    let dir = Directory::new();
    let manager = CredentialManager::new(&dir.path, vec![]).unwrap();
    manager
        .store
        .transaction()
        .unwrap()
        .publish(active())
        .unwrap();
    manager
        .add_api_key(
            "synthetic",
            "backup",
            Secret::new("synthetic-key".into()).unwrap(),
        )
        .unwrap();
    let path = dir.path.join("synthetic.json");
    let valid: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let oauth = valid["oauth"]["one"].to_string();
    let key = valid["api_keys"]["backup"].to_string();
    for (oauth_records, key_records) in [
        (
            format!(r#"{{"one":{oauth},"one":{oauth}}}"#),
            format!(r#"{{"backup":{key}}}"#),
        ),
        (
            format!(r#"{{"one":{oauth}}}"#),
            format!(r#"{{"backup":{key},"backup":{key}}}"#),
        ),
        (
            format!(
                r#"{{"one":{}}}"#,
                oauth.replace(
                    r#""access":"old-access""#,
                    r#""access":"old-access","access":"other-access""#
                )
            ),
            format!(r#"{{"backup":{key}}}"#),
        ),
        (
            format!(r#"{{"one":{oauth}}}"#),
            format!(
                r#"{{"backup":{}}}"#,
                key.replace(
                    r#""secret":"synthetic-key""#,
                    r#""secret":"synthetic-key","secret":"other-key""#
                )
            ),
        ),
    ] {
        let raw = format!(
            r#"{{"version":1,"provider":"synthetic","oauth":{oauth_records},"api_keys":{key_records}}}"#
        );
        std::fs::write(&path, raw).unwrap();
        {
            let tx = manager.store.transaction().unwrap();
            assert!(matches!(
                tx.load("synthetic", "one"),
                Err(CredentialError::Storage)
            ));
        }
        let tx = manager.keys.transaction().unwrap();
        assert!(matches!(
            tx.load_key("synthetic", "backup"),
            Err(CredentialError::Storage)
        ));
    }
}
#[test]
fn publication_faults_distinguish_unchanged_data_from_uncertain_replacement() {
    for fault in [
        PublishStep::Write,
        PublishStep::FileSync,
        PublishStep::Rename,
        PublishStep::DirectorySync,
        PublishStep::Cleanup,
        PublishStep::MarkerSync,
        PublishStep::MarkerDirectorySync,
        PublishStep::CleanupDirectorySync,
    ] {
        let dir = Directory::new();
        let store = Store::open(&dir.path).unwrap();
        let account = active();
        let mut account = store.transaction().unwrap().publish(account).unwrap();
        account.credential.as_mut().unwrap().access = Secret::new("new-access".into()).unwrap();
        account.advance().unwrap();
        store.fail_at(fault);
        let result = store.transaction().unwrap().publish(account);
        let reopened = Store::open(&dir.path).unwrap();
        let loaded = reopened
            .transaction()
            .unwrap()
            .load("synthetic", "one")
            .unwrap();
        let uncertain = matches!(
            fault,
            PublishStep::DirectorySync
                | PublishStep::Cleanup
                | PublishStep::MarkerSync
                | PublishStep::MarkerDirectorySync
        );
        let replaced = matches!(
            fault,
            PublishStep::DirectorySync | PublishStep::Cleanup | PublishStep::CleanupDirectorySync
        );
        if matches!(fault, PublishStep::CleanupDirectorySync) {
            assert!(result.is_ok());
        } else {
            assert_eq!(
                result.err().unwrap(),
                if uncertain {
                    CredentialError::PersistenceUncertain
                } else {
                    CredentialError::Storage
                }
            );
        }
        assert_eq!(loaded.recovery, uncertain);
        assert_eq!(
            loaded.account.unwrap().credential.unwrap().access.expose(),
            if replaced { "new-access" } else { "old-access" }
        );
        assert!(std::fs::read_dir(&dir.path).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .ends_with(".tmp")
        }));
    }
}
#[cfg(unix)]
#[test]
fn account_file_binding_permissions_links_and_size_are_enforced() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let dir = Directory::new();
    let store = Store::open(&dir.path).unwrap();
    store.transaction().unwrap().publish(active()).unwrap();
    let path = dir.path.join("synthetic.json");
    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(
        store
            .transaction()
            .unwrap()
            .load("synthetic", "one")
            .is_err()
    );
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    let link = dir.path.join("hardlink");
    std::fs::hard_link(&path, &link).unwrap();
    assert!(
        store
            .transaction()
            .unwrap()
            .load("synthetic", "one")
            .is_err()
    );
    std::fs::remove_file(link).unwrap();
    std::fs::rename(&path, dir.path.join("other.json")).unwrap();
    assert!(store.transaction().unwrap().load("other", "one").is_err());
    symlink(dir.path.join("other.json"), &path).unwrap();
    assert!(
        store
            .transaction()
            .unwrap()
            .load("synthetic", "one")
            .is_err()
    );
    std::fs::remove_file(&path).unwrap();
    use std::os::unix::fs::OpenOptionsExt;
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .unwrap()
        .set_len(8 * 1024 * 1024 + 1)
        .unwrap();
    assert!(
        store
            .transaction()
            .unwrap()
            .load("synthetic", "one")
            .is_err()
    );
}
#[test]
fn an_interrupted_first_publication_remains_visible_as_recovery_not_an_empty_store() {
    let dir = Directory::new();
    let store = Store::open(&dir.path).unwrap();
    drop(store.account_lock("synthetic", "one").unwrap());
    store
        .transaction()
        .unwrap()
        .test_marker("synthetic", "one", true);
    let tx = store.transaction().unwrap();
    assert_eq!(tx.aliases("synthetic").unwrap(), vec!["one"]);
    let loaded = tx.load("synthetic", "one").unwrap();
    assert!(loaded.recovery && loaded.account.is_none());
}

#[test]
fn first_publication_admits_last_namespace_without_poisoning_capacity() {
    let dir = Directory::new();
    let store = Store::open(&dir.path).unwrap();
    {
        let tx = store.transaction().unwrap();
        for index in 0..63 {
            tx.ensure_namespace(&format!("namespace-{index}")).unwrap();
        }
        tx.publish(active()).unwrap();
        assert_eq!(tx.namespaces().unwrap().len(), 64);
        let mut rejected = active();
        rejected.profile = "overflow".into();
        assert_eq!(
            tx.publish(rejected).err().unwrap(),
            CredentialError::Capacity
        );
        assert!(tx.load("overflow", "one").unwrap().account.is_none());
    }
    assert!(!dir.path.join("overflow.oauth.one.pending").exists());
    let reopened = Store::open(&dir.path).unwrap();
    let tx = reopened.transaction().unwrap();
    assert_eq!(tx.namespaces().unwrap().len(), 64);
    assert!(!tx.load("synthetic", "one").unwrap().recovery);
}

#[test]
fn first_publication_recovers_its_reserved_namespace_at_capacity() {
    let dir = Directory::new();
    let store = Store::open(&dir.path).unwrap();
    {
        let tx = store.transaction().unwrap();
        for index in 0..63 {
            tx.ensure_namespace(&format!("namespace-{index}")).unwrap();
        }
        tx.test_marker("synthetic", "one", true);
        assert_eq!(tx.namespaces().unwrap().len(), 64);
    }
    let reopened = Store::open(&dir.path).unwrap();
    let tx = reopened.transaction().unwrap();
    tx.publish(active()).unwrap();
    let loaded = tx.load("synthetic", "one").unwrap();
    assert!(!loaded.recovery);
    assert_eq!(
        loaded.account.unwrap().credential.unwrap().access.expose(),
        "old-access"
    );
    assert_eq!(tx.namespaces().unwrap().len(), 64);
}
