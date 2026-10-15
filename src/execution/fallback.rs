//! Pure fixed-route advancement: transport and downstream visibility own observations.
use super::DeliveryState;
use crate::{
    provider::ErrorClass,
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
        error: ErrorClass,
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
                        ErrorClass::CredentialUnavailable
                            | ErrorClass::Quota
                            | ErrorClass::Server
                            | ErrorClass::Timeout
                            | ErrorClass::Upstream
                    )
            }
            Self::SameProvider if matches!(error, ErrorClass::RateLimit | ErrorClass::Quota) => {
                false
            }
            Self::SameProvider | Self::Provider => {
                policy.fallback == FallbackPolicy::BeforeCommit
                    && matches!(
                        error,
                        ErrorClass::RateLimit
                            | ErrorClass::Quota
                            | ErrorClass::Server
                            | ErrorClass::Timeout
                            | ErrorClass::Upstream
                    )
            }
        }
    }
}
