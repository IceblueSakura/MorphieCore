//! Profile-neutral lifecycle coordination; network waits never hold store transactions.
mod refresh;
use crate::credential::{
    AuthDriver, CredentialError as Error, LoginOptions, LoginPrompt,
    model::{
        AccessState, Account, AccountState, AccountStatus, Grant, LogoutOutcome, random_id,
        valid_alias, valid_client, valid_profile,
    },
    storage::store::{Loaded, Store, Transaction},
};
use std::{collections::BTreeMap, path::Path, sync::Arc};

#[derive(Clone)]
pub struct CredentialManager {
    pub(super) store: Arc<Store>,
    pub(super) keys: Arc<Store>,
    drivers: Arc<BTreeMap<String, Arc<dyn AuthDriver>>>,
}
impl CredentialManager {
    pub fn new(root: impl AsRef<Path>, drivers: Vec<Arc<dyn AuthDriver>>) -> Result<Self, Error> {
        if drivers.len() > 16 {
            return Err(Error::InvalidInput);
        }
        let mut registry = BTreeMap::new();
        for driver in drivers {
            valid_profile(driver.profile())?;
            if driver.profile() == "api-keys" {
                return Err(Error::InvalidInput);
            }
            if registry
                .insert(driver.profile().to_owned(), driver)
                .is_some()
            {
                return Err(Error::InvalidInput);
            }
        }
        let store = Store::open(root.as_ref())?;
        let keys = store.key_partition();
        Ok(Self {
            store,
            keys,
            drivers: Arc::new(registry),
        })
    }
    pub(super) fn driver(&self, profile: &str) -> Result<&dyn AuthDriver, Error> {
        self.drivers
            .get(profile)
            .map(AsRef::as_ref)
            .ok_or(Error::UnknownProfile)
    }
    fn status(&self, account: &Account, recovery: bool) -> Result<AccountStatus, Error> {
        let mut status = account.status(recovery)?;
        let driver = self.driver(&account.profile)?;
        if driver.dynamic_registration() {
            status.plan_usage_enabled = Some(
                !recovery
                    && account
                        .credential
                        .as_ref()
                        .is_some_and(|c| driver.check_access(c).is_ok()),
            );
        }
        Ok(status)
    }
    pub(super) fn load(
        &self,
        tx: &Transaction<'_>,
        profile: &str,
        alias: &str,
    ) -> Result<Loaded, Error> {
        let mut loaded = tx.load(profile, alias)?;
        if let Some(account) = &mut loaded.account {
            if loaded.recovery && account.state != AccountState::Removed {
                account.credential = None;
                account.pending_renewal = None;
                account.state = AccountState::NeedsReauthorization;
                account.login_attempt = None;
            }
            self.driver(profile)?.validate(
                &account.client_id,
                account.identity.as_ref(),
                account.credential.as_ref(),
            )?;
        }
        Ok(loaded)
    }
    pub fn list(&self, selected: Option<&str>) -> Result<Vec<AccountStatus>, Error> {
        if let Some(profile) = selected {
            self.driver(profile)?;
        }
        let tx = self.store.transaction()?;
        let mut statuses = Vec::new();
        for profile in self
            .drivers
            .keys()
            .filter(|id| selected.is_none_or(|s| s == *id))
        {
            for alias in tx.aliases(profile)? {
                let loaded = self.load(&tx, profile, &alias)?;
                if let Some(account) = loaded.account {
                    statuses.push(self.status(&account, loaded.recovery)?);
                } else if loaded.recovery {
                    statuses.push(AccountStatus {
                        renewal_pending: false,
                        plan_usage_enabled: None,
                        profile: profile.clone(),
                        account: alias,
                        state: AccountState::NeedsReauthorization,
                        access: AccessState::Unavailable,
                        revision: 0,
                        generation: 0,
                        expires_at: None,
                        login_pending: false,
                        recovery_required: true,
                    });
                }
            }
        }
        Ok(statuses)
    }
    pub async fn login(
        &self,
        profile: &str,
        alias: &str,
        options: LoginOptions,
        notify: impl Fn(&LoginPrompt) + Send + Sync,
    ) -> Result<AccountStatus, Error> {
        valid_alias(alias)?;
        let driver = self.driver(profile)?;
        let initial_client = driver.login_client(&options)?;
        let client = initial_client.clone();
        valid_client(&client)?;
        let host_id = if driver.dynamic_registration() {
            self.store.transaction()?.host_id(profile)?
        } else {
            String::new()
        };
        let mut ticket = {
            let _account_lock = self.store.account_lock(profile, alias)?;
            let tx = self.store.transaction()?;
            let mut account = match self.load(&tx, profile, alias)?.account {
                Some(mut account) => {
                    if account.state == AccountState::Removed {
                        account.record_id = random_id()?;
                        account.client_id = client.clone();
                        account.state = AccountState::SignedOut;
                    }
                    account
                }
                None => Account::new(profile, alias, &client)?,
            };
            if account.client_id != client && !driver.dynamic_registration() {
                return Err(Error::IdentityMismatch);
            }
            if account.pending_renewal.is_some() || account.login_attempt.is_some() {
                return Err(Error::Busy);
            }
            if account.state == AccountState::Active {
                return Err(Error::AlreadyActive);
            }
            let client = account.client_id.clone();
            let id = random_id()?;
            account.login_attempt = Some(id.clone());
            account.advance()?;
            tx.publish(account)?;
            LoginTicket {
                store: self.store.clone(),
                profile: profile.into(),
                alias: alias.into(),
                client,
                id,
                complete: false,
            }
        };
        let retain_client = |issued: &str| -> Result<(), Error> {
            valid_client(issued)?;
            let _lock = self.store.account_lock(profile, alias)?;
            let tx = self.store.transaction()?;
            let loaded = self.load(&tx, profile, alias)?;
            let mut account = loaded.account.ok_or(Error::Superseded)?;
            if loaded.recovery || account.login_attempt.as_deref() != Some(&ticket.id) {
                return Err(Error::Superseded);
            }
            if account.client_id == issued {
                return Ok(());
            }
            if !driver.dynamic_registration()
                || ticket.client != initial_client
                || account.client_id != initial_client
                || account.identity.is_some()
                || issued == initial_client
            {
                return Err(Error::IdentityMismatch);
            }
            driver.validate(issued, None, None)?;
            account.client_id = issued.into();
            account.advance()?;
            tx.publish(account).map(|_| ())
        };
        let grant = driver
            .login(
                &ticket.client,
                &options,
                &notify,
                super::RegistrationContext {
                    host_id: &host_id,
                    retain_client: &retain_client,
                },
            )
            .await?;
        let _account_lock = self.store.account_lock(profile, alias)?;
        let tx = self.store.transaction()?;
        let loaded = self.load(&tx, profile, alias)?;
        let mut account = loaded.account.ok_or(Error::Superseded)?;
        if loaded.recovery || account.login_attempt.as_deref() != Some(&ticket.id) {
            return Err(Error::Superseded);
        }
        if (!driver.dynamic_registration() && account.client_id != ticket.client)
            || account
                .identity
                .as_ref()
                .is_some_and(|i| i != &grant.identity)
        {
            return Err(Error::IdentityMismatch);
        }
        self.validate_grant(driver, &account.client_id, &grant)?;
        for other in tx.aliases(profile)? {
            if other != alias
                && let Some(other) = self.load(&tx, profile, &other)?.account
                && other
                    .identity
                    .as_ref()
                    .is_some_and(|identity| identity.subject == grant.identity.subject)
            {
                return Err(Error::IdentityMismatch);
            }
        }
        account.identity = Some(grant.identity);
        account.replace_credential(Some(grant.credential))?;
        account.state = AccountState::Active;
        account.login_attempt = None;
        account.advance()?;
        let account = tx.publish(account)?;
        ticket.complete = true;
        self.status(&account, false)
    }
    fn validate_grant(
        &self,
        driver: &dyn AuthDriver,
        client: &str,
        grant: &Grant,
    ) -> Result<(), Error> {
        if !grant.identity.valid() || !grant.credential.valid() {
            return Err(Error::Protocol);
        }
        driver.validate(client, Some(&grant.identity), Some(&grant.credential))
    }
    pub async fn logout(
        &self,
        profile: &str,
        alias: &str,
        revoke: bool,
    ) -> Result<LogoutOutcome, Error> {
        let driver = self.driver(profile)?;
        let _account_lock = self.store.account_lock(profile, alias)?;
        let (client, previous) = {
            let tx = self.store.transaction()?;
            let loaded = self.load(&tx, profile, alias)?;
            let Some(mut account) = loaded.account else {
                if loaded.recovery {
                    tx.clear_unpublished(profile, alias)?;
                    return Ok(if revoke {
                        LogoutOutcome::RevocationUnconfirmed
                    } else {
                        LogoutOutcome::LocalOnly
                    });
                }
                return Ok(LogoutOutcome::AlreadySignedOut);
            };
            if account.state == AccountState::Removed {
                return Ok(LogoutOutcome::AlreadySignedOut);
            }
            let previous = account
                .credential
                .take()
                .or_else(|| account.pending_renewal.take().map(|p| p.credential));
            account.replace_credential(None)?;
            account.state = AccountState::SignedOut;
            account.login_attempt = None;
            account.advance()?;
            let account = tx.publish(account)?;
            (account.client_id, previous)
        };
        if !revoke {
            return Ok(LogoutOutcome::LocalOnly);
        }
        let Some(previous) = previous else {
            return Ok(LogoutOutcome::RevocationUnconfirmed);
        };
        Ok(if driver.revoke(&client, &previous).await.is_ok() {
            LogoutOutcome::Revoked
        } else {
            LogoutOutcome::RevocationUnconfirmed
        })
    }
    /// Local removal releases the subject reservation; it does not revoke remotely.
    pub fn remove_account(
        &self,
        profile: &str,
        alias: &str,
        revision: u64,
    ) -> Result<AccountStatus, Error> {
        self.driver(profile)?;
        let _lock = self.store.account_lock(profile, alias)?;
        let tx = self.store.transaction()?;
        let loaded = self.load(&tx, profile, alias)?;
        let mut account = loaded.account.ok_or(Error::LoginRequired)?;
        if account.revision != revision {
            return Err(Error::Superseded);
        }
        account.replace_credential(None)?;
        account.pending_renewal = None;
        account.login_attempt = None;
        account.identity = None;
        account.state = AccountState::Removed;
        account.advance()?;
        let account = tx.publish(account)?;
        self.status(&account, false)
    }
}
struct LoginTicket {
    store: Arc<Store>,
    profile: String,
    alias: String,
    client: String,
    id: String,
    complete: bool,
}
impl Drop for LoginTicket {
    fn drop(&mut self) {
        if self.complete {
            return;
        }
        // Best effort, only this ticket; never clear a crash/publication quarantine.
        let Ok(_account_lock) = self.store.account_lock(&self.profile, &self.alias) else {
            return;
        };
        let Ok(tx) = self.store.transaction() else {
            return;
        };
        let Ok(loaded) = tx.load(&self.profile, &self.alias) else {
            return;
        };
        if !loaded.recovery
            && let Some(mut account) = loaded.account
            && account.login_attempt.as_deref() == Some(&self.id)
        {
            account.login_attempt = None;
            if account.advance().is_ok() {
                let _ = tx.publish(account);
            }
        }
    }
}
