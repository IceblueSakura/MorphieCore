//! Classification and commit/attempt budgets, not Provider quality, govern fallback.
use morphiecore::{
    execution::{DeliveryState, fallback::may_advance},
    provider::ErrorClass,
    topology::{FallbackPolicy, RoutePolicy},
};
#[test]
fn fixed_route_policy_never_replays_visible_or_exhausted_attempts() {
    let policy = RoutePolicy {
        fallback: FallbackPolicy::BeforeCommit,
        max_attempts: 2,
        ..RoutePolicy::default()
    };
    for error in [
        ErrorClass::RateLimit,
        ErrorClass::Server,
        ErrorClass::Timeout,
        ErrorClass::Upstream,
    ] {
        assert!(may_advance(
            &policy,
            DeliveryState::Uncommitted,
            error,
            1,
            true
        ));
        assert!(!may_advance(
            &policy,
            DeliveryState::Committed,
            error,
            1,
            true
        ));
        assert!(!may_advance(
            &policy,
            DeliveryState::Terminal,
            error,
            1,
            true
        ));
        assert!(!may_advance(
            &policy,
            DeliveryState::Uncommitted,
            error,
            2,
            true
        ));
        assert!(!may_advance(
            &policy,
            DeliveryState::Uncommitted,
            error,
            1,
            false
        ));
        assert!(!may_advance(
            &RoutePolicy::default(),
            DeliveryState::Uncommitted,
            error,
            1,
            true
        ));
    }
    let redirect = match morphiecore::provider::classify_status(302) {
        morphiecore::provider::StatusClass::Failure(error) => error,
        _ => panic!("redirect is not success"),
    };
    assert!(!may_advance(
        &policy,
        DeliveryState::Uncommitted,
        redirect,
        1,
        true
    ));
    for error in [
        ErrorClass::Authentication,
        ErrorClass::PermissionDenied,
        ErrorClass::InvalidRequest,
        ErrorClass::NotFound,
    ] {
        assert!(!may_advance(
            &policy,
            DeliveryState::Uncommitted,
            error,
            1,
            true
        ));
    }
}
