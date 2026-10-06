//! Independent persistence fault boundaries; no authority traffic.
use super::{
    model::{Account, AccountState},
    store::{PublishStep, Store},
    test_support::Directory,
    *,
};
fn active() -> Account {
    let mut account = Account::new("synthetic", "one", "synthetic-client");
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
        .publish(&active())
        .unwrap();
    manager
        .add_api_key(
            "synthetic",
            "backup",
            Secret::new("synthetic-key".into()).unwrap(),
        )
        .unwrap();
    let file = dir.path.join("synthetic.json");
    let doc: serde_json::Value = serde_json::from_slice(&std::fs::read(file).unwrap()).unwrap();
    assert_eq!(doc["provider"], "synthetic");
    assert_eq!(doc["oauth"]["one"]["credential"]["access"], "old-access");
    assert_eq!(doc["api_keys"]["backup"]["secret"], "synthetic-key");
    assert!(doc["pools"].as_object().unwrap().is_empty());
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
        let mut account = active();
        store.transaction().unwrap().publish(&account).unwrap();
        account.credential.as_mut().unwrap().access = Secret::new("new-access".into()).unwrap();
        account.advance().unwrap();
        store.fail_at(fault);
        let result = store.transaction().unwrap().publish(&account);
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
                result.unwrap_err(),
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
    store.transaction().unwrap().publish(&active()).unwrap();
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
