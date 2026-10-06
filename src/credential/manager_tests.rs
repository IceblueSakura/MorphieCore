//! Profile-neutral lifecycle tests: the third driver has no product wire dependencies.
use super::*;
use std::{sync::Arc, time::Duration};
use tokio::sync::Notify;

struct ThirdDriver {
    entered: Arc<Notify>,
    release: Arc<Notify>,
}
impl AuthDriver for ThirdDriver {
    fn profile(&self) -> &'static str {
        "third"
    }
    fn login_client(&self, options: &LoginOptions) -> Result<String, CredentialError> {
        if options.method != LoginMethod::Device {
            return Err(CredentialError::Unsupported);
        }
        Ok(options
            .client_id
            .clone()
            .unwrap_or_else(|| "third-client".into()))
    }
    fn validate(
        &self,
        client: &str,
        identity: Option<&VerifiedIdentity>,
        credential: Option<&Credential>,
    ) -> Result<(), CredentialError> {
        if !client.starts_with("third-")
            || identity.is_some_and(|i| i.scope.is_some())
            || credential.is_some_and(|c| c.id_token.is_some())
        {
            return Err(CredentialError::Protocol);
        }
        Ok(())
    }
    fn login<'a>(
        &'a self,
        _client: &'a str,
        _options: &'a LoginOptions,
        _notify: LoginObserver<'a>,
    ) -> DriverFuture<'a, Grant> {
        Box::pin(async { Ok(grant()) })
    }
    fn can_refresh(&self) -> bool {
        true
    }
    fn refresh<'a>(
        &'a self,
        _client: &'a str,
        _identity: &'a VerifiedIdentity,
        _previous: &'a Credential,
    ) -> DriverFuture<'a, Grant> {
        Box::pin(async move {
            self.entered.notify_one();
            self.release.notified().await;
            Ok(grant())
        })
    }
}
#[tokio::test]
async fn inference_binding_reloads_and_rejects_expired_logout_or_identity_change() {
    let dir = test_support::Directory::new();
    let (manager, _, _) = manager(&dir);
    assert!(manager.bind_access("third", "one").is_err());
    manager
        .login("third", "one", LoginOptions::default(), |_| {})
        .await
        .unwrap();
    let binding = manager.bind_access("third", "one").unwrap();
    let provenance = binding.provenance();
    assert_eq!(
        binding.borrow().unwrap().access.expose(),
        "synthetic-third-access"
    );
    assert!(!format!("{binding:?}").contains("third-subject"));
    {
        let tx = manager.store.transaction().unwrap();
        let mut account = tx.load("third", "one").unwrap().account.unwrap();
        account.credential.as_mut().unwrap().access = Secret::new("rotated-access".into()).unwrap();
        tx.publish(&account).unwrap();
    }
    assert_eq!(binding.borrow().unwrap().access.expose(), "rotated-access");
    assert_eq!(
        manager.bind_access("third", "one").unwrap().provenance(),
        provenance
    );
    {
        let tx = manager.store.transaction().unwrap();
        let mut account = tx.load("third", "one").unwrap().account.unwrap();
        account.credential.as_mut().unwrap().expires_at = Some(1);
        tx.publish(&account).unwrap();
    }
    assert!(matches!(binding.borrow(), Err(CredentialError::Expired)));
    manager.logout("third", "one", false).await.unwrap();
    assert!(matches!(
        binding.borrow(),
        Err(CredentialError::LoginRequired)
    ));
    manager
        .login("third", "one", LoginOptions::default(), |_| {})
        .await
        .unwrap();
    {
        let tx = manager.store.transaction().unwrap();
        let mut account = tx.load("third", "one").unwrap().account.unwrap();
        account.identity.as_mut().unwrap().subject = "another-principal".into();
        tx.publish(&account).unwrap();
    }
    assert!(matches!(
        binding.borrow(),
        Err(CredentialError::IdentityMismatch)
    ));
    let rebound = manager.bind_access("third", "one").unwrap();
    assert_ne!(rebound.provenance(), provenance);
    manager
        .store
        .fail_at(store::PublishStep::MarkerDirectorySync);
    {
        let tx = manager.store.transaction().unwrap();
        let account = tx.load("third", "one").unwrap().account.unwrap();
        assert!(tx.publish(&account).is_err());
    }
    assert!(matches!(
        rebound.borrow(),
        Err(CredentialError::LoginRequired)
    ));
}

fn grant() -> Grant {
    Grant {
        identity: VerifiedIdentity {
            subject: "third-subject".into(),
            scope: None,
        },
        credential: Credential {
            access: Secret::new("synthetic-third-access".into()).unwrap(),
            refresh: Some(Secret::new("synthetic-third-refresh".into()).unwrap()),
            id_token: None,
            expires_at: None,
            scopes: None,
        },
    }
}
fn manager(dir: &test_support::Directory) -> (CredentialManager, Arc<Notify>, Arc<Notify>) {
    let entered = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let manager = CredentialManager::new(
        &dir.path,
        vec![Arc::new(ThirdDriver {
            entered: entered.clone(),
            release: release.clone(),
        })],
    )
    .unwrap();
    (manager, entered, release)
}
#[tokio::test]
async fn independent_driver_uses_the_same_lifecycle_and_readable_account_file() {
    let dir = test_support::Directory::new();
    let (manager, _, _) = manager(&dir);
    let first = manager
        .login("third", "personal", LoginOptions::default(), |_| {})
        .await
        .unwrap();
    assert_eq!(first.generation, 1);
    assert_eq!(first.access, AccessState::Unknown);
    let document = test_support::read_account(&dir.path, "third", "personal");
    assert_eq!(document["profile"], "third");
    let reopened = CredentialManager::new(
        &dir.path,
        vec![Arc::new(ThirdDriver {
            entered: Arc::new(Notify::new()),
            release: Arc::new(Notify::new()),
        })],
    )
    .unwrap();
    assert_eq!(reopened.list(None).unwrap()[0].generation, 1);
    assert_eq!(
        manager.logout("third", "personal", false).await.unwrap(),
        LogoutOutcome::LocalOnly
    );
    assert_eq!(
        reopened.list(None).unwrap()[0].state,
        AccountState::SignedOut
    );
}
#[tokio::test]
async fn refreshing_one_account_does_not_hold_the_store_transaction_lock() {
    let dir = test_support::Directory::new();
    let (manager, entered, release) = manager(&dir);
    manager
        .login("third", "one", LoginOptions::default(), |_| {})
        .await
        .unwrap();
    let mut refresh = Box::pin(manager.refresh("third", "one"));
    tokio::select! { _ = &mut refresh => panic!("must wait"), _ = entered.notified() => {} }
    assert_eq!(
        manager.refresh("third", "one").await.unwrap_err(),
        CredentialError::Busy
    );
    let statuses = manager.list(None).unwrap();
    assert_eq!(statuses[0].state, AccountState::NeedsReauthorization);
    // Independent writes must survive the first account's eventual publication.
    manager
        .login(
            "third",
            "other",
            LoginOptions {
                client_id: Some("third-secondary".into()),
                ..LoginOptions::default()
            },
            |_| {},
        )
        .await
        .unwrap();
    assert_eq!(
        manager.logout("third", "other", false).await.unwrap(),
        LogoutOutcome::LocalOnly
    );
    release.notify_one();
    assert_eq!(refresh.await.unwrap().state, AccountState::Active);
    assert_eq!(
        manager.list(None).unwrap()[1].state,
        AccountState::SignedOut
    );
}
#[tokio::test]
async fn uncertain_replacement_is_quarantined_after_reopen_and_can_be_cleared() {
    let dir = test_support::Directory::new();
    let (manager, _, _) = manager(&dir);
    manager
        .login("third", "one", LoginOptions::default(), |_| {})
        .await
        .unwrap();
    manager.store.fail_at(store::PublishStep::DirectorySync);
    assert_eq!(
        manager.logout("third", "one", false).await.unwrap_err(),
        CredentialError::PersistenceUncertain
    );
    let (reopened, _, _) = self::manager(&dir);
    assert!(reopened.list(None).unwrap()[0].recovery_required);
    assert_eq!(
        reopened.refresh("third", "one").await.unwrap_err(),
        CredentialError::LoginRequired
    );
    reopened.logout("third", "one", false).await.unwrap();
    assert!(!reopened.list(None).unwrap()[0].recovery_required);
}
#[tokio::test]
async fn duplicate_identity_is_not_published_under_another_alias() {
    let dir = test_support::Directory::new();
    let (manager, _, _) = manager(&dir);
    manager
        .login("third", "one", LoginOptions::default(), |_| {})
        .await
        .unwrap();
    assert_eq!(
        manager
            .login("third", "two", LoginOptions::default(), |_| {})
            .await
            .unwrap_err(),
        CredentialError::IdentityMismatch
    );
    assert_eq!(manager.list(None).unwrap()[0].state, AccountState::Active);
}
#[tokio::test]
async fn local_status_queries_do_not_change_credential_generation() {
    let dir = test_support::Directory::new();
    let (manager, _, _) = manager(&dir);
    let first = manager
        .login("third", "one", LoginOptions::default(), |_| {})
        .await
        .unwrap();
    let status = manager.list(None).unwrap().remove(0);
    assert_eq!(first.generation, status.generation);
    assert!(
        tokio::time::timeout(
            Duration::from_secs(1),
            manager.logout("third", "one", false)
        )
        .await
        .unwrap()
        .is_ok()
    );
}

#[tokio::test]
async fn uncertain_rotated_candidate_is_never_reused_after_restart() {
    let dir = test_support::Directory::new();
    let (manager, entered, release) = manager(&dir);
    manager
        .login("third", "one", LoginOptions::default(), |_| {})
        .await
        .unwrap();
    let mut refresh = Box::pin(manager.refresh("third", "one"));
    tokio::select! { _ = &mut refresh => panic!("must await rotation"), _ = entered.notified() => {} }
    manager.store.fail_at(store::PublishStep::DirectorySync);
    release.notify_one();
    assert_eq!(
        refresh.await.unwrap_err(),
        CredentialError::PersistenceUncertain
    );
    let (reopened, _, _) = self::manager(&dir);
    let status = reopened.list(None).unwrap().remove(0);
    assert_eq!(status.state, AccountState::NeedsReauthorization);
    assert_eq!(status.access, AccessState::Unavailable);
    assert!(status.recovery_required);
    assert_eq!(
        reopened.refresh("third", "one").await.unwrap_err(),
        CredentialError::LoginRequired
    );
    let status = reopened
        .login("third", "one", LoginOptions::default(), |_| {})
        .await
        .unwrap();
    assert_eq!(status.state, AccountState::Active);
    assert!(!status.recovery_required);
}

#[tokio::test]
async fn incomplete_first_publication_can_be_cleared_without_claiming_remote_revocation() {
    let dir = test_support::Directory::new();
    let (manager, _, _) = manager(&dir);
    drop(manager.store.account_lock("third", "one").unwrap());
    manager
        .store
        .transaction()
        .unwrap()
        .test_marker("third", "one", true);
    assert!(manager.list(None).unwrap()[0].recovery_required);
    assert_eq!(
        manager.logout("third", "one", true).await.unwrap(),
        LogoutOutcome::RevocationUnconfirmed
    );
    assert!(manager.list(None).unwrap().is_empty());
}

#[tokio::test]
async fn freshness_and_client_binding_are_not_inferred_from_lifecycle_or_alias() {
    let dir = test_support::Directory::new();
    let (manager, _, _) = manager(&dir);
    manager
        .login("third", "one", LoginOptions::default(), |_| {})
        .await
        .unwrap();
    assert_eq!(
        manager
            .login(
                "third",
                "one",
                LoginOptions {
                    client_id: Some("third-other".into()),
                    ..LoginOptions::default()
                },
                |_| {}
            )
            .await
            .unwrap_err(),
        CredentialError::IdentityMismatch
    );
    {
        let tx = manager.store.transaction().unwrap();
        let mut account = tx.load("third", "one").unwrap().account.unwrap();
        account.credential.as_mut().unwrap().expires_at = Some(1);
        tx.publish(&account).unwrap();
    }
    let status = manager.list(None).unwrap().remove(0);
    assert_eq!(status.state, AccountState::Active);
    assert_eq!(status.access, AccessState::Expired);
    assert_eq!(
        manager.list(Some("unknown")).unwrap_err(),
        CredentialError::UnknownProfile
    );
}
