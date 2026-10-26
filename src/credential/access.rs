//! Pinned identities: network-free inference snapshots and explicit renewal maintenance.
use crate::credential::{
    CredentialError as Error, CredentialManager, Secret, VerifiedIdentity, model,
};
use sha2::{Digest, Sha256};

#[derive(Clone)]
pub struct AccessBinding {
    manager: CredentialManager,
    profile: String,
    alias: String,
    client: String,
    identity: VerifiedIdentity,
}
impl std::fmt::Debug for AccessBinding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("AccessBinding([private])")
    }
}
#[derive(Debug)]
pub struct AccessGrant {
    pub access: Secret,
    pub identity: VerifiedIdentity,
}
impl CredentialManager {
    /// Pin an explicit local identity, not a token generation. Expiry is checked
    /// on each attempt so startup neither refreshes nor freezes access material.
    pub fn bind_access(&self, profile: &str, alias: &str) -> Result<AccessBinding, Error> {
        let tx = self.store.transaction()?;
        let account = self
            .load(&tx, profile, alias)?
            .account
            .ok_or(Error::LoginRequired)?;
        Ok(AccessBinding {
            manager: self.clone(),
            profile: profile.into(),
            alias: alias.into(),
            client: account.client_id,
            identity: account.identity.ok_or(Error::LoginRequired)?,
        })
    }
}
impl AccessBinding {
    /// Storage-operation identity, not replay provenance. Cloned managers share
    /// the same store; aliases in unrelated stores must not be coalesced.
    pub(crate) fn same_account(&self, other: &Self) -> bool {
        std::sync::Arc::ptr_eq(&self.manager.store, &other.manager.store)
            && self.profile == other.profile
            && self.alias == other.alias
            && self.client == other.client
            && self.identity == other.identity
    }
    pub(super) fn check_identity(&self, account: &model::Account) -> Result<(), Error> {
        if account.client_id != self.client || account.identity.as_ref() != Some(&self.identity) {
            return Err(Error::IdentityMismatch);
        }
        Ok(())
    }
    /// Explicit maintenance, never called by inference borrowing. Eligibility
    /// and pinned identity are rechecked under the manager's account lock.
    pub(crate) async fn refresh_if_due(&self, ahead: u64) -> Result<bool, Error> {
        self.manager
            .refresh_account(&self.profile, &self.alias, Some((self, ahead)))
            .await
            .map(|status| status.is_some())
    }
    pub fn profile(&self) -> &str {
        &self.profile
    }
    /// Private provenance input for the gateway's keyed replay scope. Token
    /// rotation preserves it; changing principal, profile or client does not.
    pub(crate) fn provenance(&self) -> String {
        let mut hash = Sha256::new();
        for value in [
            self.profile.as_str(),
            &self.client,
            &self.identity.subject,
            self.identity.scope.as_deref().unwrap_or(""),
        ] {
            hash.update((value.len() as u64).to_le_bytes());
            hash.update(value.as_bytes());
        }
        hash.finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }
    /// Local logout/refresh affects subsequent borrows, not an already dispatched
    /// request. No filesystem lock survives this bounded read or spans network I/O.
    pub fn borrow(&self) -> Result<AccessGrant, Error> {
        let tx = self.manager.store.transaction()?;
        let account = self
            .manager
            .load(&tx, &self.profile, &self.alias)?
            .account
            .ok_or(Error::LoginRequired)?;
        self.check_identity(&account)?;
        let credential = account.credential.ok_or(Error::LoginRequired)?;
        self.manager
            .driver(&self.profile)?
            .check_access(&credential)?;
        let now = model::now()?;
        if credential.expires_at.is_some_and(|expiry| expiry <= now) {
            return Err(Error::Expired);
        }
        // Unknown expiry remains unknown; the authority owns final validity.
        Ok(AccessGrant {
            access: credential.access,
            identity: self.identity.clone(),
        })
    }
}
