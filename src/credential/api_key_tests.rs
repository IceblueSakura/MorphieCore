//! Local key lifecycle and pinned replay identity; no authority or Gateway.
use super::{store::PublishStep, test_support::Directory, *};

fn secret(value: &str) -> Secret {
    Secret::new(value.into()).unwrap()
}
fn manager(dir: &Directory) -> CredentialManager {
    CredentialManager::new(&dir.path, vec![]).unwrap()
}
#[test]
fn keys_have_separate_lifecycle_and_rotation_never_reuses_a_bound_epoch() {
    let dir = Directory::new();
    let manager = manager(&dir);
    let first = manager
        .add_api_key("alpha", "one", secret("synthetic-first"))
        .unwrap();
    assert_eq!(first.state, ApiKeyState::Enabled);
    let access = manager.bind_api_key("alpha", "one").unwrap();
    let snapshot = access.borrow().unwrap();
    let provenance = access.provenance();
    assert!(manager.bind_api_key("beta", "one").is_err());
    assert!(
        manager
            .add_api_key("alpha", "one", secret("duplicate"))
            .is_err()
    );
    let next = manager
        .replace_api_key("alpha", "one", first.revision, secret("synthetic-next"))
        .unwrap();
    assert!(access.borrow().is_err());
    assert_eq!(snapshot.expose(), "synthetic-first");
    assert_eq!(next.generation, first.generation + 1);
    let fresh = manager.bind_api_key("alpha", "one").unwrap();
    assert_ne!(fresh.provenance(), provenance);
    assert_eq!(fresh.borrow().unwrap().expose(), "synthetic-next");
    assert_eq!(
        manager
            .remove_api_key("alpha", "one", first.revision)
            .unwrap_err(),
        CredentialError::Superseded
    );
    let disabled = manager
        .set_api_key_enabled("alpha", "one", next.revision, false)
        .unwrap();
    assert!(fresh.borrow().is_err());
    assert!(manager.bind_api_key("alpha", "one").is_err());
    let enabled = manager
        .set_api_key_enabled("alpha", "one", disabled.revision, true)
        .unwrap();
    assert!(fresh.borrow().is_err());
    let fresh = manager.bind_api_key("alpha", "one").unwrap();
    let removed = manager
        .remove_api_key("alpha", "one", enabled.revision)
        .unwrap();
    assert_eq!(removed.state, ApiKeyState::Removed);
    assert!(fresh.borrow().is_err());
    assert!(
        manager
            .set_api_key_enabled("alpha", "one", removed.revision, true)
            .is_err()
    );
    let record: serde_json::Value = serde_json::from_slice::<serde_json::Value>(
        &std::fs::read(dir.path.join("alpha.json")).unwrap(),
    )
    .unwrap()["api_keys"]["one"]
        .take();
    assert!(record["secret"].is_null());
    assert!(record.get("client_id").is_none());
    assert!(record.get("identity").is_none());
    let restored = manager
        .replace_api_key(
            "alpha",
            "one",
            removed.revision,
            secret("synthetic-restored"),
        )
        .unwrap();
    assert_eq!(restored.state, ApiKeyState::Enabled);
    assert!(fresh.borrow().is_err());
    let public = serde_json::to_string(&manager.inventory().unwrap()).unwrap();
    assert!(public.contains("api_key"));
    for forbidden in ["synthetic", "secret", "record_id", "epoch"] {
        assert!(!public.contains(forbidden), "{forbidden}");
    }
    assert!(!format!("{fresh:?}").contains("alpha"));
}

#[test]
fn key_publication_faults_quarantine_uncertainty_and_allow_explicit_replacement() {
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
        let manager = manager(&dir);
        let first = manager
            .add_api_key("alpha", "one", secret("synthetic-old"))
            .unwrap();
        let binding = manager.bind_api_key("alpha", "one").unwrap();
        manager.keys.fail_at(fault);
        let result =
            manager.replace_api_key("alpha", "one", first.revision, secret("synthetic-new"));
        let status = manager.list_api_keys(Some("alpha")).unwrap().remove(0);
        let uncertain = matches!(
            fault,
            PublishStep::DirectorySync
                | PublishStep::Cleanup
                | PublishStep::MarkerSync
                | PublishStep::MarkerDirectorySync
        );
        assert_eq!(status.recovery_required, uncertain);
        if uncertain {
            assert_eq!(result.unwrap_err(), CredentialError::PersistenceUncertain);
            assert!(binding.borrow().is_err());
            assert!(manager.bind_api_key("alpha", "one").is_err());
            manager
                .replace_api_key(
                    "alpha",
                    "one",
                    status.revision,
                    secret("synthetic-recovery"),
                )
                .unwrap();
            assert_eq!(
                manager
                    .bind_api_key("alpha", "one")
                    .unwrap()
                    .borrow()
                    .unwrap()
                    .expose(),
                "synthetic-recovery"
            );
        } else if matches!(fault, PublishStep::CleanupDirectorySync) {
            result.unwrap();
            assert!(binding.borrow().is_err());
        } else {
            assert_eq!(result.unwrap_err(), CredentialError::Storage);
            assert_eq!(binding.borrow().unwrap().expose(), "synthetic-old");
        }
    }
}

#[test]
fn unpublished_keys_require_explicit_recovery_and_missing_mutations_create_nothing() {
    let dir = Directory::new();
    let manager = manager(&dir);
    assert!(manager.remove_api_key("missing", "one", 0).is_err());
    assert!(!dir.path.join("missing.json").exists());
    manager.keys.fail_at(PublishStep::MarkerSync);
    assert_eq!(
        manager
            .add_api_key("alpha", "one", secret("synthetic"))
            .unwrap_err(),
        CredentialError::PersistenceUncertain
    );
    let status = manager.list_api_keys(None).unwrap().remove(0);
    assert!(status.recovery_required);
    assert_eq!(status.revision, 0);
    assert!(manager.bind_api_key("alpha", "one").is_err());
    assert!(
        manager
            .add_api_key("alpha", "one", secret("synthetic"))
            .is_err()
    );
    manager.remove_api_key("alpha", "one", 0).unwrap();
    assert!(manager.list_api_keys(None).unwrap().is_empty());
    manager
        .add_api_key("alpha", "one", secret("synthetic-recovered"))
        .unwrap();
}
#[test]
fn secret_json_cannot_bypass_validation_or_echo_invalid_material() {
    for input in ["", "synthetic secret", "synthetic\nsecret", "非 ASCII"] {
        let error =
            serde_json::from_str::<Secret>(&serde_json::to_string(input).unwrap()).unwrap_err();
        assert!(!error.to_string().contains("synthetic"));
    }
    let secret: Secret = serde_json::from_str(r#""synthetic-file-secret""#).unwrap();
    assert_eq!(secret.expose(), "synthetic-file-secret");
    assert!(!format!("{secret:?}").contains("synthetic-file-secret"));
    assert_eq!(
        serde_json::to_string(&secret).unwrap(),
        r#""synthetic-file-secret""#
    );
}
#[test]
fn out_of_band_material_edit_cannot_reuse_a_pinned_replay_identity() {
    let dir = Directory::new();
    let manager = manager(&dir);
    manager
        .add_api_key("alpha", "one", secret("synthetic-before"))
        .unwrap();
    let before = manager.bind_api_key("alpha", "one").unwrap();
    let path = dir.path.join("alpha.json");
    let mut document: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    document["api_keys"]["one"]["secret"] = serde_json::json!("synthetic-after");
    std::fs::write(path, serde_json::to_vec(&document).unwrap()).unwrap();
    assert!(before.borrow().is_err());
    let after = manager.bind_api_key("alpha", "one").unwrap();
    assert_ne!(before.provenance(), after.provenance());
    assert_eq!(after.borrow().unwrap().expose(), "synthetic-after");
}
#[test]
fn key_documents_reject_wrong_kind_identity_and_inconsistent_material() {
    let dir = Directory::new();
    let manager = manager(&dir);
    manager
        .add_api_key("alpha", "one", secret("synthetic"))
        .unwrap();
    let path = dir.path.join("alpha.json");
    let original: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    for (field, value) in [
        ("kind", serde_json::json!("oauth")),
        ("record_id", serde_json::json!("")),
        ("domain", serde_json::json!("beta")),
        ("alias", serde_json::json!("other")),
        ("state", serde_json::json!("removed")),
        ("secret", serde_json::json!(null)),
        ("generation", serde_json::json!(2)),
        ("client_id", serde_json::json!("not-an-oauth-account")),
    ] {
        let mut corrupted = original.clone();
        corrupted["api_keys"]["one"][field] = value;
        std::fs::write(&path, serde_json::to_vec(&corrupted).unwrap()).unwrap();
        assert!(manager.bind_api_key("alpha", "one").is_err(), "{field}");
    }
}
#[test]
fn local_locks_and_record_binding_are_shared_but_domains_are_isolated() {
    let dir = Directory::new();
    let manager = manager(&dir);
    let one = manager
        .add_api_key("alpha", "one", secret("synthetic-one"))
        .unwrap();
    manager
        .add_api_key("beta", "one", secret("synthetic-two"))
        .unwrap();
    let lock = manager.keys.account_lock("alpha", "one").unwrap();
    assert_eq!(
        manager
            .remove_api_key("alpha", "one", one.revision)
            .unwrap_err(),
        CredentialError::Busy
    );
    manager
        .add_api_key("alpha", "two", secret("synthetic-three"))
        .unwrap();
    drop(lock);
    std::fs::rename(dir.path.join("alpha.json"), dir.path.join("other.json")).unwrap();
    assert!(manager.bind_api_key("alpha", "other").is_err());
    assert!(
        manager
            .add_api_key("../escape", "one", secret("synthetic"))
            .is_err()
    );
    assert!(manager.list_api_keys(None).is_err());
}
