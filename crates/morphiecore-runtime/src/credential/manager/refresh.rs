//! Caller-owned renewal with off-worker, cancellation-drained filesystem phases.
use super::*;
use crate::credential::{
    AccessBinding, Grant, Renewal,
    storage::{fs::FileLock, store::Transaction},
};
use std::sync::{Mutex, MutexGuard, PoisonError};
use tokio_util::task::TaskTracker;

type Restore = Box<dyn FnOnce() -> Result<(), Error> + Send>;
#[derive(Default)]
struct StorageState {
    cancelled: bool,
    account_lock: Option<FileLock>,
    /// Reactivates published quarantine while no exchange has been dispatched.
    restore: Option<Restore>,
}
struct RefreshStorage {
    state: Arc<Mutex<StorageState>>,
    tasks: TaskTracker,
}
fn state(shared: &Mutex<StorageState>) -> MutexGuard<'_, StorageState> {
    // A panicking phase leaves no partial in-memory state worth rejecting;
    // the store itself owns publication recovery.
    shared.lock().unwrap_or_else(PoisonError::into_inner)
}
impl RefreshStorage {
    /// Runs a phase that precedes Provider I/O; it is skipped once the caller is gone.
    async fn prepare<T: Send + 'static>(
        &self,
        phase: impl FnOnce(&mut StorageState) -> Result<T, Error> + Send + 'static,
    ) -> Result<T, Error> {
        self.run(false, phase).await
    }
    /// Ends the pre-dispatch window: once polled, an exchange may reach the authority.
    fn dispatch(&self) {
        state(&self.state).restore = None;
    }
    /// Publishes material the Provider may already have rotated. It completes
    /// after caller cancellation, and the account lock lives until it ends.
    async fn commit<T: Send + 'static>(
        &self,
        phase: impl FnOnce() -> Result<T, Error> + Send + 'static,
    ) -> Result<T, Error> {
        self.run(true, |_| phase()).await
    }
    async fn run<T: Send + 'static>(
        &self,
        commit: bool,
        phase: impl FnOnce(&mut StorageState) -> Result<T, Error> + Send + 'static,
    ) -> Result<T, Error> {
        let shared = self.state.clone();
        let dispatch = tracing::dispatcher::get_default(Clone::clone);
        let parent = tracing::Span::current();
        let span = if parent.is_disabled() {
            tracing::Span::none()
        } else {
            tracing::info_span!(target: "morphiecore.credential", parent: &parent,
                "credential.storage", otel.kind = "internal",
                morphiecore.storage.phase = if commit { "commit" } else { "prepare" },
                morphiecore.storage.published = tracing::field::Empty,
                morphiecore.outcome = tracing::field::Empty)
        };
        self.tasks
            .spawn_blocking(move || {
                tracing::dispatcher::with_default(&dispatch, || {
                    let result = span.in_scope(|| {
                        let mut state = state(&shared);
                        if state.cancelled && !commit {
                            span.record("morphiecore.outcome", "skipped");
                            return Err(Error::Superseded);
                        }
                        let result = phase(&mut state);
                        span.record(
                            "morphiecore.outcome",
                            if result.is_ok() { "complete" } else { "failed" },
                        );
                        result
                    });
                    drop(span);
                    result
                })
            })
            .await
            .map_err(|_| Error::Storage)?
    }
}
impl Drop for RefreshStorage {
    fn drop(&mut self) {
        // An atomic filesystem replacement cannot be interrupted. Drain any
        // started phase before releasing exclusivity or acknowledging cancellation;
        // queued preparation must never run after the caller has gone away. A
        // queued commit keeps the shared lock until its blocking task drops it.
        let restore = {
            let mut state = state(&self.state);
            state.cancelled = true;
            state.restore.take()
        };
        // Preparation published quarantine, but its caller left before any
        // exchange; the shared state keeps the account lock until restoration.
        if let Some(restore) = restore {
            let shared = self.state.clone();
            let run = move || {
                let _ = restore();
                drop(shared);
            };
            if tokio::runtime::Handle::try_current().is_ok() {
                self.tasks.spawn_blocking(run);
            } else {
                run();
            }
        }
    }
}
enum Prepared {
    Skip,
    Pending(Account),
    /// Quarantined account whose credential still holds the refresh material.
    Renew(Account),
}
impl CredentialManager {
    pub async fn refresh(&self, profile: &str, alias: &str) -> Result<AccountStatus, Error> {
        self.refresh_account(profile, alias, None, &TaskTracker::new())
            .await?
            .ok_or(Error::LoginRequired)
    }
    pub(in crate::credential) async fn refresh_account(
        &self,
        profile: &str,
        alias: &str,
        automatic: Option<(&AccessBinding, u64)>,
        tasks: &TaskTracker,
    ) -> Result<Option<AccountStatus>, Error> {
        let driver = self.driver(profile)?;
        if !driver.can_refresh() {
            return if automatic.is_some() {
                Ok(None)
            } else {
                Err(Error::Unsupported)
            };
        }
        let storage = RefreshStorage {
            state: Arc::default(),
            tasks: tasks.clone(),
        };
        let manager = self.clone();
        let profile = profile.to_owned();
        let alias = alias.to_owned();
        let automatic = automatic.map(|(binding, ahead)| (binding.clone(), ahead));
        let prepared = storage
            .prepare(move |state| {
                state.account_lock = Some(manager.store.account_lock(&profile, &alias)?);
                let prepared = manager.prepare_refresh(&profile, &alias, automatic.as_ref())?;
                if let Prepared::Renew(account) = &prepared {
                    let revision = account.revision;
                    state.restore = Some(Box::new(move || {
                        manager.restore_refresh(&profile, &alias, revision)
                    }));
                }
                Ok(prepared)
            })
            .await?;
        let mut account = match prepared {
            Prepared::Skip => return Ok(None),
            Prepared::Pending(account) => {
                return self
                    .complete_renewal(driver, account, &storage)
                    .await
                    .map(Some);
            }
            Prepared::Renew(account) => account,
        };
        storage.dispatch();
        let expected = account.identity.as_ref().ok_or(Error::Storage)?;
        let previous = account.credential.as_ref().ok_or(Error::Storage)?;
        let renewed = match driver.renew(&account.client_id, expected, previous).await {
            Ok(renewed) => renewed,
            Err(Error::Unreachable) => {
                // Nothing reached the authority, so the old refresh is still unspent.
                let manager = self.clone();
                let (profile, alias, revision) = (
                    account.profile.clone(),
                    account.alias.clone(),
                    account.revision,
                );
                storage
                    .commit(move || manager.restore_refresh(&profile, &alias, revision))
                    .await?;
                return Err(Error::Unreachable);
            }
            Err(error) => return Err(error),
        };
        let grant = match renewed {
            Renewal::Verified(grant) => grant,
            Renewal::Pending(pending) => {
                let manager = self.clone();
                let account = storage
                    .commit(move || {
                        let tx = manager.store.transaction()?;
                        manager.check_current(&tx, &account)?;
                        account.replace_credential(None)?;
                        account.pending_renewal = Some(pending);
                        account.advance()?;
                        publish_renewal(&tx, account)
                    })
                    .await?;
                return self
                    .complete_renewal(driver, account, &storage)
                    .await
                    .map(Some);
            }
        };
        self.validate_grant(driver, &account.client_id, &grant)?;
        if &grant.identity != expected {
            return Err(Error::IdentityMismatch);
        }
        let manager = self.clone();
        storage
            .commit(move || manager.publish_refresh(account, grant))
            .await
            .map(Some)
    }
    fn prepare_refresh(
        &self,
        profile: &str,
        alias: &str,
        automatic: Option<&(AccessBinding, u64)>,
    ) -> Result<Prepared, Error> {
        let tx = self.store.transaction()?;
        let loaded = self.load(&tx, profile, alias)?;
        if let Some((binding, ahead)) = automatic {
            let Some(account) = loaded.account.as_ref() else {
                return Ok(Prepared::Skip);
            };
            binding.check_identity(account)?;
            if let Some(pending) = &account.pending_renewal {
                // Verification needs no token exchange, so a transient
                // failure may be retried; anything else waits for an operator.
                return Ok(
                    if loaded.recovery
                        || account.login_attempt.is_some()
                        || pending.requires_manual_verification
                    {
                        Prepared::Skip
                    } else {
                        Prepared::Pending(loaded.account.expect("loaded account"))
                    },
                );
            }
            let deadline = super::super::model::now()?.saturating_add(*ahead);
            if loaded.recovery
                || account.state != AccountState::Active
                || account.login_attempt.is_some()
                || !account.credential.as_ref().is_some_and(|credential| {
                    credential.refresh.is_some()
                        && credential
                            .expires_at
                            .is_some_and(|expiry| expiry <= deadline)
                })
            {
                return Ok(Prepared::Skip);
            }
        }
        let mut account = loaded.account.ok_or(Error::LoginRequired)?;
        if account.pending_renewal.is_some() {
            return Ok(Prepared::Pending(account));
        }
        // A quarantined refresh may already be spent and is never resent.
        if account.state != AccountState::Active
            || !account
                .credential
                .as_ref()
                .is_some_and(|credential| credential.refresh.is_some())
        {
            return Err(Error::LoginRequired);
        }
        // Quarantine only stops refresh reuse; unexpired access stays borrowable.
        account.state = AccountState::NeedsReauthorization;
        account.login_attempt = None;
        account.advance()?;
        Ok(Prepared::Renew(tx.publish(account)?))
    }
    /// Reactivates a quarantined refresh that provably never reached the authority.
    fn restore_refresh(&self, profile: &str, alias: &str, revision: u64) -> Result<(), Error> {
        let tx = self.store.transaction()?;
        let loaded = self.load(&tx, profile, alias)?;
        let mut account = loaded.account.ok_or(Error::Superseded)?;
        if loaded.recovery
            || account.revision != revision
            || account.state != AccountState::NeedsReauthorization
            || account.credential.is_none()
        {
            return Err(Error::Superseded);
        }
        account.state = AccountState::Active;
        account.advance()?;
        publish_renewal(&tx, account).map(|_| ())
    }
    /// Post-network publication applies only to the record this renewal prepared.
    fn check_current(&self, tx: &Transaction<'_>, account: &Account) -> Result<(), Error> {
        let loaded = self.load(tx, &account.profile, &account.alias)?;
        let current = loaded.account.ok_or(Error::Superseded)?;
        if loaded.recovery
            || current.revision != account.revision
            || current.client_id != account.client_id
            || current.identity != account.identity
            || current.login_attempt.is_some()
        {
            return Err(Error::Superseded);
        }
        Ok(())
    }
    fn publish_refresh(&self, mut account: Account, grant: Grant) -> Result<AccountStatus, Error> {
        let tx = self.store.transaction()?;
        self.check_current(&tx, &account)?;
        account.replace_credential(Some(grant.credential))?;
        account.state = AccountState::Active;
        account.advance()?;
        let account = publish_renewal(&tx, account)?;
        self.status(&account, false)
    }
    async fn complete_renewal(
        &self,
        driver: &dyn AuthDriver,
        account: Account,
        storage: &RefreshStorage,
    ) -> Result<AccountStatus, Error> {
        let pending = account.pending_renewal.as_ref().ok_or(Error::Storage)?;
        let manual = pending.requires_manual_verification;
        if let Err(error) = verify_pending(driver, &account).await {
            let requires_manual =
                !matches!(error, Error::Network | Error::Unreachable | Error::Timeout);
            if requires_manual != manual {
                let manager = self.clone();
                storage
                    .commit(move || {
                        let mut account = account;
                        let tx = manager.store.transaction()?;
                        manager.check_current(&tx, &account)?;
                        account
                            .pending_renewal
                            .as_mut()
                            .ok_or(Error::Storage)?
                            .requires_manual_verification = requires_manual;
                        account.advance()?;
                        publish_renewal(&tx, account)
                    })
                    .await?;
            }
            return Err(error);
        }
        let manager = self.clone();
        storage
            .commit(move || {
                let mut account = account;
                let tx = manager.store.transaction()?;
                manager.check_current(&tx, &account)?;
                let pending = account.pending_renewal.take().ok_or(Error::Storage)?;
                account.replace_credential(Some(pending.credential))?;
                account.state = AccountState::Active;
                account.advance()?;
                let account = publish_renewal(&tx, account)?;
                manager.status(&account, false)
            })
            .await
    }
}
fn publish_renewal(tx: &Transaction<'_>, account: Account) -> Result<Account, Error> {
    let account = tx.publish(account)?;
    tracing::Span::current().record("morphiecore.storage.published", true);
    Ok(account)
}

async fn verify_pending(driver: &dyn AuthDriver, account: &Account) -> Result<(), Error> {
    let expected = account.identity.as_ref().ok_or(Error::Storage)?;
    let pending = account.pending_renewal.as_ref().ok_or(Error::Storage)?;
    let identity = driver
        .verify_renewal(&account.client_id, expected, pending)
        .await?;
    if &identity != expected {
        return Err(Error::IdentityMismatch);
    }
    driver.validate(
        &account.client_id,
        Some(expected),
        Some(&pending.credential),
    )
}
