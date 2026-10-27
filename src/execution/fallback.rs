//! Pure fixed-route advancement: transport and downstream visibility own observations.
//!
//! # Advancement Invariants
//!
//! - **Publication Freeze**: Candidate advancement is permitted only while delivery state is
//!   [`DeliveryState::Uncommitted`]. Once frames have been flushed to the downstream client,
//!   failover advancement is strictly forbidden.
//! - **Attempt & Budget Bounds**: Advancement stops if `attempts >= policy.max_attempts` or
//!   if the task timeout/budget is exhausted.
//! - **Fail-Closed on Terminal Errors**: Authentication failures, context overflows,
//!   and semantic projection failures never advance.
use super::DeliveryState;
use crate::{
    provider::RecoveryClass,
    topology::{Endpoint, FallbackPolicy, RoutePolicy},
};

/// Type of route transition between candidates.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Transition {
    /// Rotating credentials on the same endpoint.
    Credential { enabled: bool },
    /// Advancing to an alternative endpoint hosted by the same provider.
    SameProvider,
    /// Advancing to an alternative provider.
    Provider,
}

impl Transition {
    /// Determines the transition type between current and next candidate endpoints.
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

    /// Evaluates whether advancement to the next candidate is permitted given current delivery
    /// state, error recovery class, and remaining attempt budget.
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
