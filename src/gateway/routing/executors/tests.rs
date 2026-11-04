use super::*;
use crate::gateway::Limits;
use std::sync::Arc;

#[test]
fn evicted_and_recreated_absence_cannot_restore_an_old_binding() {
    let (a, b) = candidates();
    let state = Executors::new(Duration::from_secs(10), 1, Default::default());
    let now = Instant::now();
    let older = state
        .observe(Some("agent"), "model", Some("session"), now)
        .unwrap();
    let winner = state
        .observe(Some("agent"), "model", Some("session"), now)
        .unwrap();
    assert_eq!(
        state
            .acquire(&a, now)
            .unwrap()
            .completed(Some(&winner), now)
            .1,
        AffinityCommit::Committed
    );
    let other = state
        .observe(Some("agent"), "model", Some("other"), now)
        .unwrap();
    assert_eq!(
        state
            .acquire(&b, now)
            .unwrap()
            .completed(Some(&other), now + Duration::from_secs(1))
            .1,
        AffinityCommit::Committed
    );
    let recreated = state
        .observe(
            Some("agent"),
            "model",
            Some("session"),
            now + Duration::from_secs(2),
        )
        .unwrap();
    let result = state
        .acquire(&a, now)
        .unwrap()
        .completed(Some(&older), now + Duration::from_secs(3));
    assert!(result.0);
    assert_eq!(result.1, AffinityCommit::Stale);
    assert_eq!(state.state.lock().unwrap().affinity.len(), 2);
    drop((older, winner, recreated, other));
    assert_eq!(state.state.lock().unwrap().affinity.len(), 1);
}

#[test]
fn in_flight_expiry_revokes_commit_without_requiring_a_lookup() {
    let (a, _) = candidates();
    let state = Executors::new(Duration::from_secs(10), 1, Default::default());
    let now = Instant::now();
    let initial = state
        .observe(Some("agent"), "model", Some("session"), now)
        .unwrap();
    state
        .acquire(&a, now)
        .unwrap()
        .completed(Some(&initial), now);
    let observed = state
        .observe(
            Some("agent"),
            "model",
            Some("session"),
            now + Duration::from_secs(1),
        )
        .unwrap();
    let result = state
        .acquire(&a, now)
        .unwrap()
        .completed(Some(&observed), now + Duration::from_secs(10));
    assert!(result.0);
    assert_eq!(result.1, AffinityCommit::Stale);
    assert_eq!(state.state.lock().unwrap().affinity.len(), 0);
}

#[test]
fn stale_same_target_completion_and_lookups_do_not_renew_idle_ttl() {
    let (a, b) = candidates();
    let state = Executors::new(Duration::from_secs(10), 4, Default::default());
    let now = Instant::now();
    let initial = state
        .observe(Some("agent"), "model", Some("session"), now)
        .unwrap();
    state
        .acquire(&a, now)
        .unwrap()
        .completed(Some(&initial), now);
    let first = state
        .observe(
            Some("agent"),
            "model",
            Some("session"),
            now + Duration::from_secs(3),
        )
        .unwrap();
    let stale = state
        .observe(
            Some("agent"),
            "model",
            Some("session"),
            now + Duration::from_secs(3),
        )
        .unwrap();
    assert_eq!(
        state
            .acquire(&a, now)
            .unwrap()
            .completed(Some(&first), now + Duration::from_secs(4))
            .1,
        AffinityCommit::Committed
    );
    assert_eq!(
        state
            .acquire(&a, now)
            .unwrap()
            .completed(Some(&stale), now + Duration::from_secs(9))
            .1,
        AffinityCommit::Stale
    );
    let mut choices = [Arc::new(b.clone()), Arc::new(a.clone())];
    assert_eq!(
        state.order(Some(&stale), &mut choices, now + Duration::from_secs(13)),
        "affinity"
    );
    assert_eq!(choices[0].cache_scope(), a.cache_scope());
    let mut choices = [Arc::new(b), Arc::new(a)];
    assert_eq!(
        state.order(Some(&stale), &mut choices, now + Duration::from_secs(14)),
        "compiled"
    );
    assert_eq!(choices[0].cache_scope().as_str(), "synthetic-b");
}

#[test]
fn account_affinity_is_agent_model_and_explicit_carrier_scoped() {
    let (a, b) = candidates();
    let state = Executors::default();
    let now = Instant::now();
    let initial = state
        .observe(Some("agent"), "model", Some("session"), now)
        .unwrap();
    state
        .acquire(&a, now)
        .unwrap()
        .completed(Some(&initial), now);
    for (agent, model, conversation) in [
        (Some("other-agent"), "model", Some("session")),
        (Some("agent"), "other-model", Some("session")),
        (Some("agent"), "model", Some("other-session")),
        (None, "model", Some("session")),
        (Some("agent"), "model", None),
    ] {
        let observed = state.observe(agent, model, conversation, now);
        let mut choices = [Arc::new(b.clone()), Arc::new(a.clone())];
        assert_eq!(
            state.order(observed.as_ref(), &mut choices, now),
            "compiled"
        );
        assert_eq!(choices[0].cache_scope(), b.cache_scope());
    }
}

#[test]
fn absent_observation_slots_are_reclaimed_and_zero_retention_stores_nothing() {
    let state = Executors::default();
    let now = Instant::now();
    for i in 0..1000 {
        let id = format!("synthetic-{i}");
        let first = state
            .observe(Some("agent"), "model", Some(&id), now)
            .unwrap();
        let second = state
            .observe(Some("agent"), "model", Some(&id), now)
            .unwrap();
        drop(first);
        assert_eq!(state.state.lock().unwrap().affinity.len(), 1);
        drop(second);
        assert_eq!(state.state.lock().unwrap().affinity.len(), 0);
    }
    for state in [
        Executors::new(Duration::ZERO, 10, Default::default()),
        Executors::new(Duration::from_secs(10), 0, Default::default()),
    ] {
        assert!(
            state
                .observe(Some("agent"), "model", Some("session"), now)
                .is_none()
        );
        assert_eq!(state.state.lock().unwrap().affinity.len(), 0);
    }
}

#[test]
fn concurrent_absent_observers_cannot_overwrite_the_first_successful_binding() {
    let (a, b) = candidates();
    let state = Executors::default();
    let now = Instant::now();
    let older_binding = state
        .observe(Some("agent"), "model", Some("session"), now)
        .unwrap();
    let winner_binding = state
        .observe(Some("agent"), "model", Some("session"), now)
        .unwrap();
    let older = state.acquire(&a, now).unwrap();
    let winner = state.acquire(&b, now).unwrap();
    assert_eq!(
        winner.completed(Some(&winner_binding), now).1,
        AffinityCommit::Committed
    );
    let result = older.completed(Some(&older_binding), now + Duration::from_secs(1));
    assert!(result.0);
    assert_eq!(result.1, AffinityCommit::Stale);
    drop(older);
    drop(winner);
    let mut ordered = [Arc::new(a), Arc::new(b)];
    state.order(
        Some(&older_binding),
        &mut ordered,
        now + Duration::from_secs(1),
    );
    assert_eq!(ordered[0].cache_scope().as_str(), "synthetic-b");
}

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

#[test]
fn health_domains_follow_account_identity_and_exact_target_not_pools() {
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
        state
            .acquire(&a, now)
            .unwrap()
            .failed(ApiError::failure(status, b"", None), now, None)
            .unwrap();
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
fn account_and_target_probes_are_atomic_generation_fenced_and_cancel_safe() {
    let (a, b) = candidates();
    let sibling = other_model(&a);
    let state = Executors::default();
    let now = Instant::now();
    let older = state.acquire(&a, now).unwrap();
    let failure = state.acquire(&a, now).unwrap();
    failure
        .failed(ApiError::failure(401, b"", None), now, None)
        .unwrap();
    failure
        .failed(ApiError::failure(429, b"", None), now, None)
        .unwrap();
    assert!(!older.completed(None, now).0);
    drop(older);
    drop(failure);
    assert!(state.acquire(&a, now + Duration::from_secs(59)).is_err());
    // An expired target scope cannot reserve the still-cooling account.
    let partial = now + Duration::from_secs(60);
    assert!(state.acquire(&a, partial).is_err());
    assert!(state.acquire(&b, partial).is_ok());
    let later = now + Duration::from_secs(1800);
    let probe = state.acquire(&a, later).unwrap();
    assert!(probe.probing);
    assert_eq!(
        state.acquire(&sibling, later).err().unwrap().reason,
        "probing"
    );
    assert!(
        state
            .acquire(&sibling, later)
            .err()
            .unwrap()
            .retry_after_seconds
            .is_none()
    );
    drop(probe);
    let probe = state.acquire(&sibling, later).unwrap();
    assert!(probe.probing);
    assert!(probe.completed(None, later).0);
    drop(probe);
    let target_probe = state.acquire(&a, later).unwrap();
    assert!(target_probe.probing);
    assert!(state.acquire(&sibling, later).is_ok());
    assert!(target_probe.completed(None, later).0);
    drop(target_probe);
    assert!(!state.acquire(&a, later).unwrap().probing);
}
#[test]
fn account_probe_waits_for_sibling_target_probe_in_either_material_state() {
    let (a, _) = candidates();
    let sibling = other_model(&a);
    for replacement in [false, true] {
        let state = Executors::default();
        let now = Instant::now();
        let in_flight = state.acquire(&sibling, now).unwrap();
        state
            .acquire(&a, now)
            .unwrap()
            .failed(ApiError::failure(503, b"", None), now, None);
        let later = now + Duration::from_secs(1);
        let target_probe = state.acquire(&a, later).unwrap();
        assert!(target_probe.probing);
        // Target probes alone do not exclude healthy sibling-model traffic.
        assert!(state.acquire(&sibling, later).is_ok());
        in_flight.failed(
            ApiError::failure(401, b"", None),
            later,
            replacement.then_some(1),
        );
        drop(in_flight);
        let eligible = if replacement {
            state.verified_material(&sibling.health, 2);
            later
        } else {
            later + Duration::from_secs(1800)
        };
        assert_eq!(
            state.acquire(&sibling, eligible).err().unwrap().reason,
            "probing"
        );
        drop(target_probe);
        let account_probe = state.acquire(&sibling, eligible).unwrap();
        assert!(account_probe.probing);
        assert!(state.acquire(&a, eligible).is_err());
        drop(account_probe);
        assert!(state.acquire(&sibling, eligible).is_ok());
    }
}

#[test]
fn recovery_backoff_is_bounded_and_success_resets_it() {
    let (a, _) = candidates();
    let state = Executors::default();
    let mut now = Instant::now();
    for seconds in [1, 2, 4, 8, 16, 32, 60, 60] {
        let lease = state.acquire(&a, now).unwrap();
        assert_eq!(
            lease
                .failed(
                    ApiError::transport_fact(crate::transport::http::TransportError::Interrupted),
                    now,
                    None
                )
                .unwrap()
                .retry_after_seconds,
            Some(seconds)
        );
        drop(lease);
        assert!(state.acquire(&a, now).is_err());
        now += Duration::from_secs(seconds);
    }
    state.acquire(&a, now).unwrap().completed(None, now);
    assert_eq!(
        state
            .acquire(&a, now)
            .unwrap()
            .failed(
                ApiError::transport_fact(crate::transport::http::TransportError::Interrupted),
                now,
                None
            )
            .unwrap()
            .retry_after_seconds,
        Some(1)
    );
}
#[test]
fn merged_failures_keep_each_scope_and_advance_target_backoff_once() {
    let (a, b) = candidates();
    let now = Instant::now();
    let state = Executors::default();
    let lease = state.acquire(&a, now).unwrap();
    lease
        .resolve_failures(
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
        )
        .unwrap();
    drop(lease);
    assert_eq!(
        state
            .unavailable(&other_model(&a).health, now + Duration::from_secs(20))
            .unwrap()
            .retry_after_seconds,
        Some(1790)
    );
    assert!(state.unavailable(&b.health, now).is_none());
    let probe = state.acquire(&a, now + Duration::from_secs(1810)).unwrap();
    assert!(probe.probing);
    probe.completed(None, now + Duration::from_secs(1810));
    drop(probe);

    let state = Executors::default();
    let lease = state.acquire(&a, now).unwrap();
    lease.resolve_failures(
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
        now + Duration::from_secs(1),
    );
    drop(lease);
    let next = now + Duration::from_secs(61);
    assert_eq!(
        state
            .acquire(&a, next)
            .unwrap()
            .failed(ApiError::failure(503, b"", None), next, None)
            .unwrap()
            .retry_after_seconds,
        Some(2)
    );
}
#[test]
fn delayed_cleanup_uses_observation_time_and_open_cooldowns_do_not_escalate() {
    let (a, _) = candidates();
    let state = Executors::default();
    let now = Instant::now();
    let first = state.acquire(&a, now).unwrap();
    let concurrent = state.acquire(&a, now).unwrap();
    first.failed(ApiError::failure(503, b"", None), now, None);
    concurrent.failed(
        ApiError::failure(503, b"", None),
        now + Duration::from_millis(500),
        None,
    );
    drop(first);
    drop(concurrent);
    let later = now + Duration::from_millis(1500);
    assert_eq!(
        state
            .acquire(&a, later)
            .unwrap()
            .failed(ApiError::failure(503, b"", None), later, None)
            .unwrap()
            .retry_after_seconds,
        Some(2)
    );
    let state = Executors::default();
    let lease = state.acquire(&a, now).unwrap();
    lease.resolve_failures(
        &[FailureObservation {
            error: ApiError::failure(503, b"", None),
            at: now,
            material_revision: None,
        }],
        now + Duration::from_secs(20),
    );
    drop(lease);
    let probe = state.acquire(&a, now + Duration::from_secs(20)).unwrap();
    assert!(probe.probing);
}
#[test]
fn verified_material_only_releases_old_auth_for_a_probe_and_fences_stale_401() {
    let (a, _) = candidates();
    let sibling = other_model(&a);
    let state = Executors::default();
    let now = Instant::now();
    state.verified_material(&a.health, 1);
    let older = state.acquire(&a, now).unwrap();
    older.sent_material(Some(1));
    state
        .acquire(&a, now)
        .unwrap()
        .failed(ApiError::failure(401, b"", None), now, Some(1));
    assert!(state.acquire(&a, now).is_err());
    state.verified_material(&a.health, 2);
    let probe = state.acquire(&a, now).unwrap();
    assert!(probe.probing);
    probe.sent_material(Some(2));
    assert!(state.acquire(&sibling, now).is_err());
    assert!(
        older
            .failed(
                ApiError::failure(401, b"", None),
                now + Duration::from_secs(1),
                Some(1)
            )
            .is_none()
    );
    assert!(probe.completed(None, now).0);
    drop(probe);
    drop(older);
    assert!(!state.acquire(&sibling, now).unwrap().probing);

    state.acquire(&a, now).unwrap().resolve_failures(
        &[
            FailureObservation {
                error: ApiError::failure(401, b"", None),
                at: now,
                material_revision: Some(2),
            },
            FailureObservation {
                error: ApiError::failure(402, b"", None),
                at: now,
                material_revision: Some(2),
            },
        ],
        now,
    );
    state.verified_material(&a.health, 3);
    assert!(
        state.acquire(&a, now).is_err(),
        "replacement does not clear quota"
    );
}
#[test]
fn old_material_success_cannot_certify_replacement_auth_health() {
    let (a, _) = candidates();
    let state = Executors::default();
    let now = Instant::now();
    state.verified_material(&a.health, 1);
    state
        .acquire(&a, now)
        .unwrap()
        .failed(ApiError::failure(401, b"", None), now, Some(1));
    let later = now + Duration::from_secs(1800);
    let old_probe = state.acquire(&a, later).unwrap();
    old_probe.sent_material(Some(1));
    state.verified_material(&a.health, 2);
    assert!(old_probe.completed(None, later).0);
    drop(old_probe);
    let replacement = state.acquire(&a, later).unwrap();
    assert!(replacement.probing);
    replacement.sent_material(Some(2));
    replacement.completed(None, later);
    drop(replacement);
    assert!(!state.acquire(&a, later).unwrap().probing);
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
            state
                .acquire(&a, now)
                .unwrap()
                .failed(ApiError::failure(status, b"", Some(99_999)), now, None)
                .unwrap()
                .retry_after_seconds,
            Some(expected)
        );
    }
    let state = Executors::default();
    let lease = state.acquire(&a, now).unwrap();
    assert!(
        lease
            .failed(
                ApiError::new(
                    axum::http::StatusCode::SERVICE_UNAVAILABLE,
                    crate::gateway::error::ErrorKind::CredentialUnavailable
                ),
                now,
                None
            )
            .is_none()
    );
    assert!(lease.failed(ApiError::upstream(), now, None).is_none());
    assert!(lease.failed(ApiError::timeout(), now, None).is_none());
}

#[test]
fn successful_failover_sticks_and_older_completion_cannot_restore_failure() {
    let (a, b) = candidates();
    let state = Executors::default();
    let now = Instant::now();
    let old = state.acquire(&a, now).unwrap();
    let old_binding = state
        .observe(Some("agent"), "model", Some("session"), now)
        .unwrap();
    assert_eq!(
        old.completed(Some(&old_binding), now).1,
        AffinityCommit::Committed
    );
    let failing = state.acquire(&a, now).unwrap();
    failing
        .failed(
            ApiError::transport_fact(crate::transport::http::TransportError::Interrupted),
            now,
            None,
        )
        .unwrap();
    drop(failing);
    let good_binding = state
        .observe(Some("agent"), "model", Some("session"), now)
        .unwrap();
    let good = state.acquire(&b, now).unwrap();
    assert_eq!(
        good.completed(Some(&good_binding), now).1,
        AffinityCommit::Committed
    );
    drop(good);
    assert!(!old.completed(Some(&old_binding), now).0);
    drop(old);
    // Recover A on an independent request; the established session stays on B.
    let recovered = state.acquire(&a, now + Duration::from_secs(1)).unwrap();
    assert!(recovered.completed(None, now + Duration::from_secs(1)).0);
    drop(recovered);
    let mut ordered = vec![Arc::new(a), Arc::new(b)];
    state.order(
        Some(&good_binding),
        &mut ordered,
        now + Duration::from_secs(1),
    );
    assert_eq!(ordered[0].cache_scope().as_str(), "synthetic-b");
    state.order(None, &mut ordered, now + Duration::from_secs(1)); // stable input order when no preference
    assert_eq!(ordered[0].cache_scope().as_str(), "synthetic-b");
}
