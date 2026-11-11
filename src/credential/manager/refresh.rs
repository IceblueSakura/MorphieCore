//! Caller-owned renewal with off-worker, cancellation-drained filesystem phases.
use super::*;
use crate::credential::{
    AccessBinding, Credential, Grant, Renewal,
    storage::{fs::FileLock, store::Transaction},
};
use std::sync::{Mutex, MutexGuard, PoisonError};

#[derive(Default)]
struct StorageState {
    cancelled: bool,
    committing: bool,
    account_lock: Option<FileLock>,
}
#[derive(Default)]
struct RefreshStorage(Arc<Mutex<StorageState>>);
fn state(shared: &Mutex<StorageState>) -> MutexGuard<'_, StorageState> {
    // A panicking phase leaves no partial in-memory state worth rejecting;
    // the store itself owns publication recovery.
    shared.lock().unwrap_or_else(PoisonError::into_inner)
}
impl RefreshStorage {
    /// Runs a phase that precedes Provider I/O; it is skipped once the caller is gone.
    async fn prepare<T: Send + 'static>(
        &self,
        phase: impl FnOnce(&mut Option<FileLock>) -> Result<T, Error> + Send + 'static,
    ) -> Result<T, Error> {
        self.run(false, phase).await
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
        phase: impl FnOnce(&mut Option<FileLock>) -> Result<T, Error> + Send + 'static,
    ) -> Result<T, Error> {
        let shared = self.0.clone();
        if commit {
            state(&shared).committing = true;
        }
        tokio::task::spawn_blocking(move || {
            let mut state = state(&shared);
            if state.cancelled && !commit {
                return Err(Error::Superseded);
            }
            let result = phase(&mut state.account_lock);
            state.committing = false;
            result
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
        let mut state = state(&self.0);
        state.cancelled = true;
        if !state.committing {
            state.account_lock.take();
        }
    }
}
enum Prepared {
    Skip,
    Pending(Account),
    Renew(Account, Credential),
}
impl CredentialManager {
    pub async fn refresh(&self, profile: &str, alias: &str) -> Result<AccountStatus, Error> {
        self.refresh_account(profile, alias, None)
            .await?
            .ok_or(Error::LoginRequired)
    }
    pub(in crate::credential) async fn refresh_account(
        &self,
        profile: &str,
        alias: &str,
        automatic: Option<(&AccessBinding, u64)>,
    ) -> Result<Option<AccountStatus>, Error> {
        let driver = self.driver(profile)?;
        if !driver.can_refresh() {
            return if automatic.is_some() {
                Ok(None)
            } else {
                Err(Error::Unsupported)
            };
        }
        let storage = RefreshStorage::default();
        let manager = self.clone();
        let profile = profile.to_owned();
        let alias = alias.to_owned();
        let automatic = automatic.map(|(binding, ahead)| (binding.clone(), ahead));
        let prepared = storage
            .prepare(move |lock| {
                *lock = Some(manager.store.account_lock(&profile, &alias)?);
                manager.prepare_refresh(&profile, &alias, automatic.as_ref())
            })
            .await?;
        let (mut account, previous) = match prepared {
            Prepared::Skip => return Ok(None),
            Prepared::Pending(account) => {
                return self
                    .complete_renewal(driver, account, &storage)
                    .await
                    .map(Some);
            }
            Prepared::Renew(account, previous) => (account, previous),
        };
        let expected = account.identity.as_ref().ok_or(Error::Storage)?;
        let renewed = driver
            .renew(&account.client_id, expected, &previous)
            .await?;
        let grant = match renewed {
            Renewal::Verified(grant) => grant,
            Renewal::Pending(pending) => {
                let manager = self.clone();
                let account = storage
                    .commit(move || {
                        let tx = manager.store.transaction()?;
                        manager.check_current(&tx, &account)?;
                        account.pending_renewal = Some(pending);
                        account.advance()?;
                        tx.publish(account)
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
        {
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
            if let Some(account) = loaded.account
                && account.pending_renewal.is_some()
            {
                return Ok(Prepared::Pending(account));
            }
        }
        let tx = self.store.transaction()?;
        let mut account = self
            .load(&tx, profile, alias)?
            .account
            .ok_or(Error::LoginRequired)?;
        let previous = account.credential.take().ok_or(Error::LoginRequired)?;
        if previous.refresh.is_none() {
            return Err(Error::LoginRequired);
        }
        account.replace_credential(None)?;
        account.state = AccountState::NeedsReauthorization;
        account.login_attempt = None;
        account.advance()?;
        Ok(Prepared::Renew(tx.publish(account)?, previous))
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
        let account = tx.publish(account)?;
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
            let requires_manual = !matches!(error, Error::Network | Error::Timeout);
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
                        tx.publish(account)
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
                let account = tx.publish(account)?;
                manager.status(&account, false)
            })
            .await
    }
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
