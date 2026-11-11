//! Caller-owned renewal with off-worker, cancellation-drained filesystem phases.
use super::*;
use crate::credential::{AccessBinding, Credential, Grant, Renewal, storage::fs::FileLock};
use std::sync::Mutex;

#[derive(Default)]
struct StorageState {
    cancelled: bool,
    account_lock: Option<FileLock>,
}
#[derive(Default)]
struct RefreshStorage(Arc<Mutex<StorageState>>);
impl RefreshStorage {
    async fn run<T: Send + 'static>(
        &self,
        phase: impl FnOnce(&mut Option<FileLock>) -> Result<T, Error> + Send + 'static,
    ) -> Result<T, Error> {
        let state = self.0.clone();
        tokio::task::spawn_blocking(move || {
            let mut state = state.lock().expect("refresh storage phase");
            if state.cancelled {
                return Err(Error::Superseded);
            }
            phase(&mut state.account_lock)
        })
        .await
        .map_err(|_| Error::Storage)?
    }
}
impl Drop for RefreshStorage {
    fn drop(&mut self) {
        // An atomic filesystem replacement cannot be interrupted. Drain any
        // started phase before releasing exclusivity or acknowledging cancellation;
        // queued phases must never publish after the caller has gone away.
        let mut state = self.0.lock().expect("refresh storage phase");
        state.cancelled = true;
        state.account_lock.take();
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
            .run(move |lock| {
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
                    .run(move |_| {
                        account.pending_renewal = Some(pending);
                        account.advance()?;
                        manager.store.transaction()?.publish(account)
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
            .run(move |_| manager.publish_refresh(account, grant))
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
                let deadline = super::super::model::now()?.saturating_add(*ahead);
                if loaded.recovery
                    || account.state != AccountState::Active
                    || account.login_attempt.is_some()
                    || account.pending_renewal.is_some()
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
    fn publish_refresh(&self, mut account: Account, grant: Grant) -> Result<AccountStatus, Error> {
        let tx = self.store.transaction()?;
        let loaded = self.load(&tx, &account.profile, &account.alias)?;
        let current = loaded.account.ok_or(Error::Superseded)?;
        if loaded.recovery
            || current.revision != account.revision
            || current.client_id != account.client_id
            || current.identity != account.identity
            || current.login_attempt.is_some()
        {
            return Err(Error::Superseded);
        }
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
        )?;
        let manager = self.clone();
        storage
            .run(move |_| {
                let mut account = account;
                let tx = manager.store.transaction()?;
                let loaded = manager.load(&tx, &account.profile, &account.alias)?;
                if loaded.recovery
                    || loaded
                        .account
                        .as_ref()
                        .is_none_or(|current| current.revision != account.revision)
                {
                    return Err(Error::Superseded);
                }
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
