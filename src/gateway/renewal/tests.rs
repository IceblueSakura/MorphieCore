//! Synthetic listener maintenance; no Provider credentials or network access.
use crate::{
    credential::*,
    gateway::{Gateway, Limits, tests::support::gateway},
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
        _: RegistrationContext<'a>,
    ) -> DriverFuture<'a, Grant> {
        Box::pin(async { Ok(grant()) })
    }
    fn can_refresh(&self) -> bool {
        true
    }
    fn renew<'a>(
        &'a self,
        _: &'a str,
        _: &'a VerifiedIdentity,
        _: &'a Credential,
    ) -> DriverFuture<'a, Renewal> {
        Box::pin(async move {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.entered.notify_one();
            if self.hold {
                std::future::pending::<()>().await;
            }
            Ok(Renewal::Verified(grant()))
        })
    }
}
async fn setup(
    hold: bool,
    limits: Limits,
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
    let mut gate = gateway(limits);
    // Registration maintenance is independent of generation candidates.
    let mut credentials = crate::gateway::Credentials::new();
    for id in ["first", "duplicate"] {
        credentials.insert_account(
            crate::provider::CredentialBindingId::new(id).unwrap(),
            manager.bind_access("synthetic", "one").unwrap(),
        );
    }
    Arc::get_mut(&mut gate.state).unwrap().oauth_accounts = credentials.oauth_accounts;
    (directory, gate, manager, calls, entered)
}
async fn poll_maintenance(future: &mut std::pin::Pin<Box<impl std::future::Future<Output = ()>>>) {
    tokio::select! {
        biased;
        _ = future => panic!("maintenance should remain active"),
        _ = tokio::task::yield_now() => {},
    }
}
async fn scan_maintenance(
    gate: &Gateway,
    future: &mut std::pin::Pin<Box<impl std::future::Future<Output = ()>>>,
) {
    let scanned = gate.state.oauth_scan_complete.notified();
    tokio::pin!(scanned);
    scanned.as_mut().enable();
    let started = std::time::Instant::now();
    loop {
        tokio::select! {
            biased;
            _ = &mut *future => panic!("maintenance should remain active"),
            _ = &mut scanned => return,
            _ = tokio::task::yield_now() => {},
        }
        // Yield without advancing the paused clock while filesystem workers run.
        assert!(started.elapsed() < std::time::Duration::from_secs(3));
    }
}
#[tokio::test(start_paused = true)]
async fn configured_scan_and_success_spacing_control_rotation_without_requests() {
    use std::time::Duration;
    let (_dir, gate, _, calls, _) = setup(
        false,
        Limits {
            oauth_scan_interval: Duration::from_secs(2),
            oauth_minimum_renewal_interval: Duration::from_secs(6),
            ..Limits::default()
        },
    )
    .await;
    let mut maintenance = Box::pin(gate.renew_credentials());
    scan_maintenance(&gate, &mut maintenance).await;
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    tokio::time::advance(Duration::from_secs(5)).await;
    poll_maintenance(&mut maintenance).await;
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    tokio::time::advance(Duration::from_secs(1)).await;
    scan_maintenance(&gate, &mut maintenance).await;
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    gate.shutdown();
    maintenance.await;
}

#[tokio::test(start_paused = true)]
async fn zero_scan_is_startup_only_and_zero_renewal_budget_never_consumes_material() {
    use std::time::Duration;
    for (limits, expected) in [
        (
            Limits {
                oauth_scan_interval: Duration::ZERO,
                ..Limits::default()
            },
            1,
        ),
        (
            Limits {
                oauth_renewal_timeout: Duration::ZERO,
                ..Limits::default()
            },
            0,
        ),
    ] {
        let (_dir, gate, manager, calls, _) = setup(false, limits).await;
        let mut maintenance = Box::pin(gate.renew_credentials());
        if expected == 1 {
            scan_maintenance(&gate, &mut maintenance).await;
        } else {
            poll_maintenance(&mut maintenance).await;
        }
        assert_eq!(calls.load(Ordering::SeqCst), expected);
        tokio::time::advance(Duration::from_secs(3600)).await;
        poll_maintenance(&mut maintenance).await;
        assert_eq!(calls.load(Ordering::SeqCst), expected);
        assert_eq!(manager.list(None).unwrap()[0].state, AccountState::Active);
        gate.shutdown();
        maintenance.await;
    }
}

#[tokio::test(start_paused = true)]
async fn refresh_ahead_is_not_an_implicit_always_renew_window() {
    let (_dir, gate, manager, calls, _) = setup(
        false,
        Limits {
            oauth_refresh_ahead_seconds: 0,
            ..Limits::default()
        },
    )
    .await;
    let original = manager
        .bind_access("synthetic", "one")
        .unwrap()
        .borrow()
        .unwrap()
        .material_revision;
    let mut maintenance = Box::pin(gate.renew_credentials());
    scan_maintenance(&gate, &mut maintenance).await;
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_eq!(
        manager
            .bind_access("synthetic", "one")
            .unwrap()
            .borrow()
            .unwrap()
            .material_revision,
        original
    );
    gate.shutdown();
    maintenance.await;
}
#[tokio::test(start_paused = true)]
async fn duplicate_bindings_renew_without_requests_and_short_grants_are_rate_bounded() {
    let (_dir, gate, manager, calls, _) = setup(false, Limits::default()).await;
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    let _router = gate.router();
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    let binding = manager.bind_access("synthetic", "one").unwrap();
    let provenance = binding.provenance();
    let original_material = binding.borrow().unwrap().material_revision;
    let mut candidate = gate.state.entries.values().next().unwrap().candidates[0]
        .as_ref()
        .clone();
    candidate.secret = crate::gateway::startup::credentials::Source::Account(binding.clone());
    let now = tokio::time::Instant::now();
    let stale = gate.state.executors.acquire(&candidate, now).unwrap();
    let failure = gate.state.executors.acquire(&candidate, now).unwrap();
    failure.failed(
        crate::gateway::ApiError::failure(401, b"", None),
        now,
        Some(original_material),
    );
    drop(failure);
    assert!(gate.state.executors.acquire(&candidate, now).is_err());
    let mut maintenance = Box::pin(gate.renew_credentials());
    scan_maintenance(&gate, &mut maintenance).await;
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let renewed = binding.borrow().unwrap();
    assert!(renewed.material_revision > original_material);
    assert_eq!(
        renewed.material_revision,
        manager.list(None).unwrap()[0].generation
    );
    assert_eq!(
        binding.borrow().unwrap().material_revision,
        renewed.material_revision
    );
    assert_eq!(binding.provenance(), provenance);
    let revision = candidate
        .secret
        .availability_for_request()
        .await
        .unwrap()
        .unwrap();
    assert_eq!(revision, renewed.material_revision);
    gate.state
        .executors
        .verified_material(&candidate.health, revision);
    let probe = gate
        .state
        .executors
        .acquire(&candidate, tokio::time::Instant::now())
        .unwrap();
    assert!(probe.probing);
    assert!(
        gate.state
            .executors
            .acquire(&candidate, tokio::time::Instant::now())
            .is_err()
    );
    drop(probe);
    stale.failed(
        crate::gateway::ApiError::failure(401, b"", None),
        tokio::time::Instant::now(),
        Some(original_material),
    );
    drop(stale);
    let probe = gate
        .state
        .executors
        .acquire(&candidate, tokio::time::Instant::now())
        .unwrap();
    probe.failed(
        crate::gateway::ApiError::failure(402, b"", None),
        tokio::time::Instant::now(),
        Some(revision),
    );
    drop(probe);
    // Another durable rotation cannot release independent account quota.
    manager.refresh("synthetic", "one").await.unwrap();
    let revision = candidate
        .secret
        .availability_for_request()
        .await
        .unwrap()
        .unwrap();
    gate.state
        .executors
        .verified_material(&candidate.health, revision);
    assert!(
        gate.state
            .executors
            .acquire(&candidate, tokio::time::Instant::now())
            .is_err()
    );
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    tokio::time::advance(std::time::Duration::from_secs(45)).await;
    poll_maintenance(&mut maintenance).await;
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    tokio::time::advance(std::time::Duration::from_secs(15)).await;
    scan_maintenance(&gate, &mut maintenance).await;
    assert_eq!(calls.load(Ordering::SeqCst), 3);
    gate.shutdown();
    maintenance.await;
}
#[tokio::test]
async fn contended_maintenance_storage_does_not_stall_listener_requests() {
    use std::time::{Duration, Instant};
    let (dir, gate, manager, _, _) = setup(false, Limits::default()).await;
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(dir.path().join("credentials/store.lock"))
        .unwrap();
    lock.lock().unwrap();
    let (release, released) = std::sync::mpsc::channel();
    let holder = std::thread::spawn(move || {
        let _ = released.recv_timeout(Duration::from_secs(2));
        drop(lock);
    });
    let owner = gate.clone();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(gate.serve(listener, std::future::pending()));
    let started = Instant::now();
    // The first server poll also starts the maintenance scan.
    tokio::task::yield_now().await;
    let response = reqwest::Client::new()
        .get(format!("http://{address}/v1/models"))
        .bearer_auth(crate::gateway::tests::support::KEY)
        .timeout(Duration::from_millis(500))
        .send()
        .await;
    release.send(()).unwrap();
    holder.join().unwrap();
    assert_eq!(response.unwrap().status(), 200);
    assert!(started.elapsed() < Duration::from_millis(500));
    owner.shutdown();
    server.await.unwrap().unwrap();
    // Shutdown has drained storage and released renewal ownership.
    manager.logout("synthetic", "one", false).await.unwrap();
    assert_eq!(
        manager.list(None).unwrap()[0].state,
        AccountState::SignedOut
    );
}

#[tokio::test]
async fn listener_cancellation_stops_inflight_renewal_without_reusing_consumed_material() {
    for graceful in [false, true] {
        let (_dir, gate, manager, calls, entered) = setup(true, Limits::default()).await;
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
    let (_dir, mut gate, manager, calls, entered) = setup(
        true,
        Limits {
            oauth_renewal_timeout: std::time::Duration::from_secs(2),
            ..Limits::default()
        },
    )
    .await;
    let (sink, exporter) = crate::gateway::diagnostics::Sink::memory();
    let state = Arc::get_mut(&mut gate.state).unwrap();
    state.diagnostics = Some(sink);
    state.oauth_accounts[0].name = Some("configured-account".into());
    let mut maintenance = Box::pin(gate.renew_credentials());
    tokio::select! {
        _ = &mut maintenance => panic!("maintenance should remain active"),
        _ = entered.notified() => {},
    }
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    tokio::time::advance(gate.state.limits.oauth_renewal_timeout).await;
    poll_maintenance(&mut maintenance).await;
    let logs = exporter.get_emitted_logs().unwrap();
    assert_eq!(logs.len(), 1);
    assert_eq!(
        logs[0].record.severity_number(),
        Some(opentelemetry::logs::Severity::Warn)
    );
    assert!(
        logs[0]
            .record
            .attributes_iter()
            .any(|(key, value)| key.as_str() == "account"
                && *value == opentelemetry::logs::AnyValue::from("configured-account"))
    );
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
