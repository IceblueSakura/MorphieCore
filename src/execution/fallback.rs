//! Pure fixed-route advancement: transport and downstream visibility own observations.
use super::DeliveryState;
use crate::{
    provider::RecoveryClass,
    topology::{Endpoint, FallbackPolicy, RoutePolicy},
};
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Transition {
    Credential { enabled: bool },
    SameProvider,
    Provider,
}
impl Transition {
    pub fn between(current: &Endpoint, next: &Endpoint, credential_fallback: bool) -> Self {
        if current.id == next.id {
            Self::Credential {
                enabled: credential_fallback,
            }
        } else if current.provider == next.provider {
            Self::SameProvider
        } else {
            Self::Provider
        }
    }
    pub fn may_advance(
        self,
        policy: &RoutePolicy,
        state: DeliveryState,
        error: RecoveryClass,
        attempts: usize,
        budget_remaining: bool,
    ) -> bool {
        if state != DeliveryState::Uncommitted
            || attempts >= policy.max_attempts
            || !budget_remaining
        {
            return false;
        }
        match self {
            // Unknown-scope 429s, auth and permission failures never rotate keys.
            Self::Credential { enabled } => {
                enabled
                    && matches!(
                        error,
                        RecoveryClass::CredentialUnavailable
                            | RecoveryClass::Quota
                            | RecoveryClass::Server
                            | RecoveryClass::Timeout
                            | RecoveryClass::Upstream
                    )
            }
            Self::SameProvider
                if matches!(error, RecoveryClass::RateLimit | RecoveryClass::Quota) =>
            {
                false
            }
            Self::SameProvider | Self::Provider => {
                policy.fallback == FallbackPolicy::BeforeCommit
                    && matches!(
                        error,
                        RecoveryClass::RateLimit
                            | RecoveryClass::Quota
                            | RecoveryClass::Server
                            | RecoveryClass::Timeout
                            | RecoveryClass::Upstream
                    )
            }
        }
    }
}
