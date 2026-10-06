//! Trusted authorization adapter boundary; drivers never publish or select accounts.
use super::{
    Credential, CredentialError as Error, Grant, LoginOptions, LoginPrompt, VerifiedIdentity,
};
use std::{future::Future, pin::Pin};

pub type DriverFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, Error>> + Send + 'a>>;
pub type LoginObserver<'a> = &'a (dyn Fn(&LoginPrompt) + Send + Sync);
pub type RegistrationObserver<'a> = &'a (dyn Fn(&str) -> Result<(), Error> + Send + Sync);
pub struct RegistrationContext<'a> {
    pub host_id: &'a str,
    pub retain_client: RegistrationObserver<'a>,
}

pub trait AuthDriver: Send + Sync {
    /// The bootstrap client is not an issued registration or an inference credential.
    fn dynamic_registration(&self) -> bool {
        false
    }
    fn login_registered<'a>(
        &'a self,
        client: &'a str,
        options: &'a LoginOptions,
        notify: LoginObserver<'a>,
        _registration: RegistrationContext<'a>,
    ) -> DriverFuture<'a, Grant> {
        self.login(client, options, notify)
    }
    fn check_access(&self, _credential: &Credential) -> Result<(), Error> {
        Ok(())
    }
    fn renew<'a>(
        &'a self,
        client: &'a str,
        identity: &'a VerifiedIdentity,
        previous: &'a Credential,
    ) -> DriverFuture<'a, super::Renewal> {
        Box::pin(async move {
            self.refresh(client, identity, previous)
                .await
                .map(super::Renewal::Verified)
        })
    }
    fn verify_renewal<'a>(
        &'a self,
        _client: &'a str,
        _identity: &'a VerifiedIdentity,
        _pending: &'a super::PendingRenewal,
    ) -> DriverFuture<'a, VerifiedIdentity> {
        Box::pin(async { Err(Error::Unsupported) })
    }
    /// Stable registration key, not an inference Provider or authority URL.
    fn profile(&self) -> &'static str;
    /// Validate method-specific options before starting a local login ticket.
    fn login_client(&self, options: &LoginOptions) -> Result<String, Error>;
    /// Pure validation of trusted persisted state and freshly returned grants.
    /// Network identity proof belongs to login/refresh, never storage reload.
    fn validate(
        &self,
        client: &str,
        identity: Option<&VerifiedIdentity>,
        credential: Option<&Credential>,
    ) -> Result<(), Error>;
    fn login<'a>(
        &'a self,
        client: &'a str,
        options: &'a LoginOptions,
        notify: LoginObserver<'a>,
    ) -> DriverFuture<'a, Grant>;
    fn can_refresh(&self) -> bool {
        false
    }
    fn refresh<'a>(
        &'a self,
        _client: &'a str,
        _identity: &'a VerifiedIdentity,
        _previous: &'a Credential,
    ) -> DriverFuture<'a, Grant> {
        Box::pin(async { Err(Error::Unsupported) })
    }
    fn revoke<'a>(&'a self, _client: &'a str, _credential: &'a Credential) -> DriverFuture<'a, ()> {
        Box::pin(async { Err(Error::Unsupported) })
    }
}
