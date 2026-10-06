//! Short-lived inference snapshots. No refresh, account selection or authority I/O.
use super::{CredentialError as Error, CredentialManager, Secret, VerifiedIdentity, model};
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
        if account.client_id != self.client || account.identity.as_ref() != Some(&self.identity) {
            return Err(Error::IdentityMismatch);
        }
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
