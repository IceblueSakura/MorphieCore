//! Pure fixed-route advancement: transport and downstream visibility own observations.
use super::DeliveryState;
use crate::{
    provider::ErrorClass,
    topology::{FallbackPolicy, RoutePolicy},
};
/// Same-Provider credentials do not imply independent quotas. Unknown-scope
/// 429s, auth and permission failures must not trigger credential rotation.
pub fn may_advance_credential(
    enabled: bool,
    state: DeliveryState,
    error: ErrorClass,
    budget_remaining: bool,
) -> bool {
    enabled
        && state == DeliveryState::Uncommitted
        && budget_remaining
        && matches!(
            error,
            ErrorClass::CredentialUnavailable
                | ErrorClass::Server
                | ErrorClass::Timeout
                | ErrorClass::Upstream
        )
}
pub fn may_advance(
    policy: &RoutePolicy,
    state: DeliveryState,
    error: ErrorClass,
    attempts: usize,
    budget_remaining: bool,
) -> bool {
    policy.fallback == FallbackPolicy::BeforeCommit
        && state == DeliveryState::Uncommitted
        && attempts < policy.max_attempts
        && budget_remaining
        && matches!(
            error,
            ErrorClass::RateLimit | ErrorClass::Server | ErrorClass::Timeout | ErrorClass::Upstream
        )
}
