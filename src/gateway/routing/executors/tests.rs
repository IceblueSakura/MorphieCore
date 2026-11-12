use super::*;
use crate::gateway::Limits;
use std::sync::Arc;

fn candidates() -> (BoundCandidate, BoundCandidate) {
    let gate = crate::gateway::tests::support::gateway(Limits::default());
    let mut a = gate.state.entries[&(
        crate::protocol::ClientProtocol::Chat,
        "fixture-model".into(),
    )]
        .candidates[0]
        .as_ref()
        .clone();
    a.health = Scopes::bind(&[0; 32], &a.endpoint, "synthetic-a");
    let mut b = a.clone();
    b.endpoint.bound_replay_scope =
        Some(crate::semantic::value::ReplayOrigin::new("synthetic-b").unwrap());
    b.health = Scopes::bind(&[0; 32], &b.endpoint, "synthetic-b");
    (a, b)
}
fn other_model(a: &BoundCandidate) -> BoundCandidate {
    let mut candidate = a.clone();
    candidate.endpoint.upstream_model = "other-model".into();
    candidate.health = Scopes::bind(&[0; 32], &candidate.endpoint, "synthetic-a");
    candidate
}
fn fail(
    state: &Executors,
    candidate: &BoundCandidate,
    error: ApiError,
    at: Instant,
    material_revision: Option<u64>,
) -> Option<Observation> {
    state.record_failures(
        candidate,
        &[FailureObservation {
            error,
            at,
            material_revision,
        }],
        at,
    )
}

#[test]
fn preferences_are_bounded_success_updated_and_expire_without_lookup_renewal() {
    let (a, b) = candidates();
    let state = Executors::new(Duration::from_secs(10), 1, Default::default());
    let now = Instant::now();
    let key = state
        .observe(Some("agent"), "model", Some("session"))
        .unwrap();
    assert_eq!(state.state.lock().unwrap().affinity.len(), 0);
    state.completed(&a, Some(&key), now);
    state.completed(&a, Some(&key), now + Duration::from_secs(4));
    let mut choices = [Arc::new(b.clone()), Arc::new(a.clone())];
    assert_eq!(
        state.order(Some(&key), &mut choices, now + Duration::from_secs(13)),
        "affinity"
    );
    let mut choices = [Arc::new(b.clone()), Arc::new(a.clone())];
    assert_eq!(
        state.order(Some(&key), &mut choices, now + Duration::from_secs(14)),
        "compiled"
    );
    assert_eq!(choices[0].cache_scope(), b.cache_scope());
    // Completion needs no reservation; even an expired key can earn a new preference.
    state.completed(&a, Some(&key), now + Duration::from_secs(15));
    let other = state
        .observe(Some("agent"), "model", Some("other"))
        .unwrap();
    state.completed(&b, Some(&other), now + Duration::from_secs(16));
    assert_eq!(state.state.lock().unwrap().affinity.len(), 1);
    let mut choices = [Arc::new(b.clone()), Arc::new(a.clone())];
    assert_eq!(
        state.order(Some(&key), &mut choices, now + Duration::from_secs(16)),
        "compiled"
    );
    for state in [
        Executors::new(Duration::ZERO, 10, Default::default()),
        Executors::new(Duration::from_secs(10), 0, Default::default()),
    ] {
        state.completed(&a, Some(&key), now);
        assert_eq!(state.state.lock().unwrap().affinity.len(), 0);
    }
}

#[test]
fn account_affinity_is_agent_model_and_explicit_carrier_scoped() {
    let (a, b) = candidates();
    let state = Executors::default();
    let now = Instant::now();
    let initial = state.observe(Some("agent"), "model", Some("session"));
    state.completed(&a, initial.as_ref(), now);
    for (agent, model, conversation) in [
        (Some("other-agent"), "model", Some("session")),
        (Some("agent"), "other-model", Some("session")),
        (Some("agent"), "model", Some("other-session")),
        (None, "model", Some("session")),
        (Some("agent"), "model", None),
    ] {
        let key = state.observe(agent, model, conversation);
        let mut choices = [Arc::new(b.clone()), Arc::new(a.clone())];
        assert_eq!(state.order(key.as_ref(), &mut choices, now), "compiled");
        assert_eq!(choices[0].cache_scope(), b.cache_scope());
    }
}

#[test]
fn configured_order_is_default_and_successful_failover_earns_a_preference() {
    let (a, b) = candidates();
    let state = Executors::default();
    let now = Instant::now();
    let key = state.observe(Some("agent"), "model", Some("session"));
    let ordered = || [Arc::new(a.clone()), Arc::new(b.clone())];
    let mut choices = ordered();
    state.order(key.as_ref(), &mut choices, now);
    assert_eq!(choices[0].cache_scope(), a.cache_scope());
    fail(&state, &a, ApiError::failure(503, b"", None), now, None);
    let mut choices = ordered();
    state.order(key.as_ref(), &mut choices, now);
    assert_eq!(choices[0].cache_scope(), b.cache_scope());
    state.completed(&b, key.as_ref(), now);
    let later = now + Duration::from_secs(1);
    state.completed(&a, None, later);
    let mut choices = ordered();
    state.order(key.as_ref(), &mut choices, later);
    assert_eq!(choices[0].cache_scope(), b.cache_scope());
    let mut choices = ordered();
    state.order(None, &mut choices, later);
    assert_eq!(choices[0].cache_scope(), a.cache_scope());
    // Concurrent completions need no arbitration: the last successful one wins.
    state.completed(&a, key.as_ref(), later);
    let mut choices = ordered();
    state.order(key.as_ref(), &mut choices, later);
    assert_eq!(choices[0].cache_scope(), a.cache_scope());
}

#[test]
fn health_domains_follow_account_identity_and_exact_target() {
    let (a, b) = candidates();
    let sibling = other_model(&a);
    let now = Instant::now();
    let mut origin = a.clone();
    origin.endpoint.target.origin =
        crate::provider::TrustedOrigin::parse("https://other.example").unwrap();
    origin.health = Scopes::bind(&[0; 32], &origin.endpoint, "synthetic-a");
    let mut operation = a.clone();
    operation.endpoint.protocol = crate::topology::GenerationProtocol::OpenAiResponses;
    operation.health = Scopes::bind(&[0; 32], &operation.endpoint, "synthetic-a");
    for (status, whole_account) in [
        (401, true),
        (402, true),
        (403, false),
        (429, false),
        (503, false),
    ] {
        let state = Executors::default();
        fail(&state, &a, ApiError::failure(status, b"", None), now, None).unwrap();
        assert!(state.unavailable(&a.health, now).is_some());
        for candidate in [&sibling, &origin, &operation] {
            assert_eq!(
                state.unavailable(&candidate.health, now).is_some(),
                whole_account,
                "status {status}"
            );
        }
        assert!(state.unavailable(&b.health, now).is_none());
    }
}

#[test]
fn recovery_backoff_is_bounded_and_success_resets_expired_failures() {
    let (a, _) = candidates();
    let state = Executors::default();
    let mut now = Instant::now();
    for seconds in [1, 2, 4, 8, 16, 32, 60, 60] {
        assert_eq!(
            fail(&state, &a, ApiError::failure(503, b"", None), now, None)
                .unwrap()
                .retry_after_seconds,
            Some(seconds)
        );
        assert!(state.unavailable(&a.health, now).is_some());
        now += Duration::from_secs(seconds);
        assert!(state.unavailable(&a.health, now).is_none());
    }
    state.completed(&a, None, now);
    assert_eq!(
        fail(&state, &a, ApiError::failure(503, b"", None), now, None)
            .unwrap()
            .retry_after_seconds,
        Some(1)
    );
}

#[test]
fn merged_failures_retain_scopes_and_original_observation_times() {
    let (a, b) = candidates();
    let now = Instant::now();
    let state = Executors::default();
    state.record_failures(
        &a,
        &[
            FailureObservation {
                error: ApiError::failure(503, b"", None),
                at: now,
                material_revision: None,
            },
            FailureObservation {
                error: ApiError::failure(402, b"", None),
                at: now + Duration::from_secs(10),
                material_revision: None,
            },
        ],
        now + Duration::from_secs(20),
    );
    assert_eq!(
        state
            .unavailable(&other_model(&a).health, now + Duration::from_secs(20))
            .unwrap()
            .retry_after_seconds,
        Some(1790)
    );
    assert!(state.unavailable(&b.health, now).is_none());
    let state = Executors::default();
    state.record_failures(
        &a,
        &[
            FailureObservation {
                error: ApiError::failure(503, b"", None),
                at: now,
                material_revision: None,
            },
            FailureObservation {
                error: ApiError::failure(429, b"", None),
                at: now + Duration::from_secs(1),
                material_revision: None,
            },
        ],
        now + Duration::from_secs(20),
    );
    let next = now + Duration::from_secs(61);
    assert_eq!(
        fail(&state, &a, ApiError::failure(503, b"", None), next, None)
            .unwrap()
            .retry_after_seconds,
        Some(2)
    );
}

#[test]
fn concurrent_success_does_not_erase_live_cooldown_or_escalate_its_backoff() {
    let (a, _) = candidates();
    let now = Instant::now();
    let state = Executors::default();
    fail(&state, &a, ApiError::failure(503, b"", None), now, None);
    state.completed(&a, None, now);
    assert!(state.unavailable(&a.health, now).is_some());
    fail(
        &state,
        &a,
        ApiError::failure(503, b"", None),
        now + Duration::from_millis(500),
        None,
    );
    let later = now + Duration::from_millis(1500);
    assert_eq!(
        fail(&state, &a, ApiError::failure(503, b"", None), later, None)
            .unwrap()
            .retry_after_seconds,
        Some(2)
    );
}

#[test]
fn verified_material_releases_only_old_auth_and_ignores_stale_401() {
    let (a, _) = candidates();
    let state = Executors::default();
    let now = Instant::now();
    state.verified_material(&a.health, 1);
    fail(&state, &a, ApiError::failure(401, b"", None), now, Some(1));
    assert!(state.unavailable(&a.health, now).is_some());
    state.verified_material(&a.health, 2);
    assert!(state.unavailable(&a.health, now).is_none());
    assert!(fail(&state, &a, ApiError::failure(401, b"", None), now, Some(1)).is_none());
    fail(&state, &a, ApiError::failure(401, b"", None), now, Some(2));
    state.completed(&a, None, now);
    assert!(state.unavailable(&a.health, now).is_some());
    fail(&state, &a, ApiError::failure(402, b"", None), now, Some(2));
    state.verified_material(&a.health, 3);
    assert!(
        state.unavailable(&a.health, now).is_some(),
        "rotation does not clear quota"
    );
}

#[test]
fn provider_policy_caps_hints_and_lifecycle_vetoes_do_not_create_health() {
    let (a, _) = candidates();
    let now = Instant::now();
    let policy = RecoveryPolicy {
        rate_limit_reset_max_ms: 7_000,
        quota_reset_max_ms: 9_000,
        auth_cooldown_ms: 11_000,
        ..RecoveryPolicy::default()
    };
    for (status, expected) in [(429, 7), (402, 9), (401, 11)] {
        let state = Executors::new(
            Duration::from_secs(10),
            1,
            BTreeMap::from([(a.provider.id.as_str().into(), policy.clone())]),
        );
        assert_eq!(
            fail(
                &state,
                &a,
                ApiError::failure(status, b"", Some(99_999)),
                now,
                None
            )
            .unwrap()
            .retry_after_seconds,
            Some(expected)
        );
    }
    let state = Executors::default();
    for error in [
        ApiError::new(
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            crate::gateway::error::ErrorKind::CredentialUnavailable,
        ),
        ApiError::upstream(),
        ApiError::timeout(),
    ] {
        assert!(fail(&state, &a, error, now, None).is_none());
    }
}
