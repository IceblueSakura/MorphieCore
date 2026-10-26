use super::*;
use crate::{gateway::Limits, topology::FallbackPolicy};

fn candidates() -> Vec<Arc<BoundCandidate>> {
    let gate = crate::gateway::tests::support::gateway(Limits::default());
    let template = &gate.state.entries[&(
        crate::protocol::ClientProtocol::Chat,
        "fixture-model".into(),
    )]
        .candidates[0];
    [
        ("a", "a"),
        ("a", "a"),
        ("b", "b"),
        ("a-other", "a"),
        ("c", "c"),
    ]
    .into_iter()
    .map(|(endpoint, provider)| {
        let mut candidate = template.as_ref().clone();
        candidate.endpoint.id = EndpointId::new(endpoint).unwrap();
        candidate.endpoint.provider = crate::provider::ProviderId::new(provider).unwrap();
        candidate.credential_fallback = true;
        candidate.credential_attempts = 2;
        Arc::new(candidate)
    })
    .collect()
}

#[test]
fn stable_suffix_retains_all_failure_restrictions_without_charging_skips() {
    let candidates = candidates();
    let policy = RoutePolicy {
        fallback: FallbackPolicy::BeforeCommit,
        max_attempts: 3,
        ..Default::default()
    };
    let mut chain = Chain::new(&candidates, &policy);
    chain.begin(0);
    assert!(chain.could_follow(0, 2));
    chain.failed(0, RecoveryClass::RateLimit);
    assert_eq!(chain.skip(1), Some("failure_scope"));
    assert_eq!(chain.skip(2), None);
    chain.begin(2);
    chain.failed(2, RecoveryClass::Server);
    assert_eq!(chain.skip(3), Some("failure_scope"));
    assert_eq!(chain.skip(4), None);
    assert_eq!(chain.attempts, 2);
    chain.begin(4);
    assert_eq!(chain.skip(1), Some("route_budget"));
}

#[test]
fn endpoint_pool_budget_is_runtime_and_includes_local_resolution() {
    let mut candidates = candidates();
    Arc::make_mut(&mut candidates[0]).credential_attempts = 1;
    Arc::make_mut(&mut candidates[1]).credential_attempts = 1;
    let policy = RoutePolicy {
        fallback: FallbackPolicy::BeforeCommit,
        ..Default::default()
    };
    let mut chain = Chain::new(&candidates, &policy);
    // Either retained member can be first, but the endpoint gets one attempt.
    assert_eq!(chain.skip(1), None);
    chain.begin(1);
    chain.failed(1, RecoveryClass::CredentialUnavailable);
    assert_eq!(chain.skip(0), Some("pool_budget"));
    // Existing local-credential transition rules do not gain Provider retry.
    assert_eq!(chain.skip(2), Some("failure_scope"));
    assert_eq!(chain.attempts, 1);
}
