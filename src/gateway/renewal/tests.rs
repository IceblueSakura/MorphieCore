//! Synthetic listener maintenance; no Provider credentials or network access.
use crate::{
    credential::*,
    gateway::{Gateway, Limits, startup::credentials::Source, tests::support::gateway},
};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use tokio::sync::Notify;

struct Driver {
    calls: Arc<AtomicUsize>,
    entered: Arc<Notify>,
    hold: bool,
}
fn grant() -> Grant {
    Grant {
        identity: VerifiedIdentity {
            subject: "synthetic-subject".into(),
            scope: None,
        },
        credential: Credential {
            access: Secret::new("synthetic-access".into()).unwrap(),
            refresh: Some(Secret::new("synthetic-refresh".into()).unwrap()),
            id_token: None,
            expires_at: Some(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs()
                    + 10,
            ),
            scopes: None,
        },
    }
}
impl AuthDriver for Driver {
    fn profile(&self) -> &'static str {
        "synthetic"
    }
    fn login_client(&self, _: &LoginOptions) -> Result<String, CredentialError> {
        Ok("synthetic-client".into())
    }
    fn validate(
        &self,
        _: &str,
        _: Option<&VerifiedIdentity>,
        _: Option<&Credential>,
    ) -> Result<(), CredentialError> {
        Ok(())
    }
    fn login<'a>(
        &'a self,
        _: &'a str,
        _: &'a LoginOptions,
        _: LoginObserver<'a>,
    ) -> DriverFuture<'a, Grant> {
        Box::pin(async { Ok(grant()) })
    }
    fn can_refresh(&self) -> bool {
        true
    }
    fn refresh<'a>(
        &'a self,
        _: &'a str,
        _: &'a VerifiedIdentity,
        _: &'a Credential,
    ) -> DriverFuture<'a, Grant> {
        Box::pin(async move {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.entered.notify_one();
            if self.hold {
                std::future::pending::<()>().await;
            }
            Ok(grant())
        })
    }
}
async fn setup(
    hold: bool,
) -> (
    tempfile::TempDir,
    Gateway,
    CredentialManager,
    Arc<AtomicUsize>,
    Arc<Notify>,
) {
    let directory = tempfile::tempdir().unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let entered = Arc::new(Notify::new());
    let manager = CredentialManager::new(
        directory.path().join("credentials"),
        vec![Arc::new(Driver {
            calls: calls.clone(),
            entered: entered.clone(),
            hold,
        })],
    )
    .unwrap();
    manager
        .login("synthetic", "one", LoginOptions::default(), |_| {})
        .await
        .unwrap();
    let mut gate = gateway(Limits::default());
    for entry in Arc::get_mut(&mut gate.state).unwrap().entries.values_mut() {
        for candidate in &mut Arc::get_mut(entry).unwrap().candidates {
            // Separate bind_access calls, not merely clones, cover deduplication.
            Arc::get_mut(candidate).unwrap().secret =
                Source::Account(manager.bind_access("synthetic", "one").unwrap());
        }
    }
    (directory, gate, manager, calls, entered)
}
async fn poll_maintenance(future: &mut std::pin::Pin<Box<impl std::future::Future<Output = ()>>>) {
    tokio::select! {
        biased;
        _ = future => panic!("maintenance should remain active"),
        _ = tokio::task::yield_now() => {},
    }
}
#[tokio::test(start_paused = true)]
async fn duplicate_bindings_renew_without_requests_and_short_grants_are_rate_bounded() {
    let (_dir, gate, manager, calls, _) = setup(false).await;
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    let _router = gate.router();
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    let binding = manager.bind_access("synthetic", "one").unwrap();
    let provenance = binding.provenance();
    let mut maintenance = Box::pin(gate.renew_credentials());
    poll_maintenance(&mut maintenance).await;
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(binding.borrow().is_ok());
    assert_eq!(binding.provenance(), provenance);
    tokio::time::advance(std::time::Duration::from_secs(45)).await;
    poll_maintenance(&mut maintenance).await;
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    tokio::time::advance(std::time::Duration::from_secs(15)).await;
    poll_maintenance(&mut maintenance).await;
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    gate.shutdown();
    maintenance.await;
}
#[tokio::test]
async fn listener_cancellation_stops_inflight_renewal_without_reusing_consumed_material() {
    for graceful in [false, true] {
        let (_dir, gate, manager, calls, entered) = setup(true).await;
        let owner = gate.clone();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let server = tokio::spawn(gate.serve(listener, std::future::pending()));
        tokio::time::timeout(std::time::Duration::from_secs(2), entered.notified())
            .await
            .unwrap();
        let duplicate_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        assert!(
            owner
                .clone()
                .serve(duplicate_listener, std::future::pending())
                .await
                .is_err()
        );
        assert!(!owner.state.shutdown.is_cancelled());
        if graceful {
            owner.shutdown();
            server.await.unwrap().unwrap();
        } else {
            server.abort();
            assert!(server.await.unwrap_err().is_cancelled());
        }
        assert!(owner.state.shutdown.is_cancelled());
        assert_eq!(
            manager.list(None).unwrap()[0].state,
            AccountState::NeedsReauthorization
        );
        assert!(
            !manager
                .bind_access("synthetic", "one")
                .unwrap()
                .refresh_if_due(120)
                .await
                .unwrap()
        );
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        // Cancellation released the operation lock as well as the future.
        assert_eq!(
            manager.logout("synthetic", "one", false).await.unwrap(),
            LogoutOutcome::LocalOnly
        );
    }
}
#[tokio::test(start_paused = true)]
async fn renewal_timeout_is_bounded_and_never_retried() {
    let (_dir, gate, manager, calls, _) = setup(true).await;
    let mut maintenance = Box::pin(gate.renew_credentials());
    poll_maintenance(&mut maintenance).await;
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    tokio::time::advance(super::RENEWAL_TIMEOUT).await;
    poll_maintenance(&mut maintenance).await;
    assert_eq!(
        manager.list(None).unwrap()[0].state,
        AccountState::NeedsReauthorization
    );
    tokio::time::advance(std::time::Duration::from_secs(120)).await;
    poll_maintenance(&mut maintenance).await;
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    gate.shutdown();
    maintenance.await;
}
