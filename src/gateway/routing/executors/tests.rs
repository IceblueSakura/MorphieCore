use super::*;
use crate::gateway::Limits;
use std::sync::Arc;

#[test]
fn evicted_and_recreated_absence_cannot_restore_an_old_binding() {
    let (a, b) = candidates();
    let state = Executors::new(Duration::from_secs(10), 1);
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
    let state = Executors::new(Duration::from_secs(10), 1);
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
    let state = Executors::new(Duration::from_secs(10), 4);
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
    assert_eq!(choices[0].cache_scope, a.cache_scope);
    let mut choices = [Arc::new(b), Arc::new(a)];
    assert_eq!(
        state.order(Some(&stale), &mut choices, now + Duration::from_secs(14)),
        "compiled"
    );
    assert_eq!(choices[0].cache_scope.as_str(), "synthetic-b");
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
        assert_eq!(choices[0].cache_scope, b.cache_scope);
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
        Executors::new(Duration::ZERO, 10),
        Executors::new(Duration::from_secs(10), 0),
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
    assert_eq!(ordered[0].cache_scope.as_str(), "synthetic-b");
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
    let pool = vec!["synthetic-a".into(), "synthetic-b".into()];
    a.health = Scopes::bind(&[0; 32], &a.endpoint, "synthetic-a", &pool);
    let mut b = a.clone();
    b.cache_scope = crate::semantic::value::ReplayOrigin::new("synthetic-b").unwrap();
    b.health = Scopes::bind(&[0; 32], &b.endpoint, "synthetic-b", &pool);
    (a, b)
}

#[test]
fn cooling_is_exclusive_generation_fenced_and_cancel_safe() {
    let (a, b) = candidates();
    let state = Executors::default();
    let now = Instant::now();
    let old = state.acquire(&a, now).unwrap();
    let concurrent = state.acquire(&a, now).unwrap();
    old.failed(ApiError::failure(429, b"", Some(600)), now)
        .unwrap();
    concurrent
        .failed(ApiError::failure(429, b"", Some(0)), now)
        .unwrap();
    assert_eq!(
        state
            .unavailable(&b.health, now)
            .unwrap()
            .retry_after_seconds,
        600
    );
    assert!(!old.completed(None, now).0);
    drop(old);
    drop(concurrent);
    assert!(state.acquire(&a, now + Duration::from_secs(599)).is_err());
    let later = now + Duration::from_secs(600);
    let probe = state.acquire(&a, later).unwrap();
    assert!(probe.probing);
    assert_eq!(state.acquire(&b, later).err().unwrap().reason, "probing");
    // Cancellation/drop releases ownership, not the need for recovery.
    drop(probe);
    let probe = state.acquire(&b, later).unwrap();
    assert!(probe.probing);
    assert!(probe.completed(None, later).0);
    drop(probe);
    assert!(state.unavailable(&a.health, later).is_none());
    assert!(!state.acquire(&a, later).unwrap().probing);

    // Check-and-acquire is atomic across scopes, not just for one rate limit.
    let state = Executors::default();
    let old = state.acquire(&a, now).unwrap();
    let concurrent = state.acquire(&a, now).unwrap();
    let old_sibling = state.acquire(&b, now).unwrap();
    old.failed(ApiError::failure(401, b"", Some(1)), now)
        .unwrap();
    concurrent
        .failed(ApiError::failure(429, b"", Some(2)), now)
        .unwrap();
    drop(old);
    drop(concurrent);
    let mut other_model = a.clone();
    other_model.endpoint.upstream_model = "other-model".into();
    other_model.health = Scopes::bind(
        &[0; 32],
        &other_model.endpoint,
        "synthetic-a",
        &["synthetic-a".into(), "synthetic-b".into()],
    );
    let partial = now + Duration::from_secs(1);
    assert!(state.acquire(&a, partial).is_err());
    // Failed acquisition must not reserve the already-expired credential.
    drop(state.acquire(&other_model, partial).unwrap());
    let both = now + Duration::from_secs(2);
    let probe = state.acquire(&a, both).unwrap();
    assert!(state.acquire(&other_model, both).is_err());
    assert!(state.acquire(&b, both).is_err());
    old_sibling
        .failed(ApiError::failure(429, b"", Some(100)), both)
        .unwrap();
    assert!(!probe.completed(None, both).0);
    drop(probe);
    drop(old_sibling);
    assert_eq!(
        state
            .unavailable(&b.health, both)
            .unwrap()
            .retry_after_seconds,
        100
    );
    drop(state.acquire(&other_model, both).unwrap());
}

#[test]
fn health_domains_preserve_credential_target_and_pool_boundaries() {
    let (a, b) = candidates();
    let now = Instant::now();
    let mut other_model = a.clone();
    other_model.endpoint.upstream_model = "other-model".into();
    other_model.health = Scopes::bind(
        &[0; 32],
        &other_model.endpoint,
        "synthetic-a",
        &["synthetic-a".into(), "synthetic-b".into()],
    );
    let mut other_pool = b.clone();
    other_pool.health = Scopes::bind(
        &[0; 32],
        &b.endpoint,
        "synthetic-c",
        &["synthetic-c".into()],
    );
    for (status, blocks_sibling, blocks_other_model) in [
        (401, false, true),
        (403, false, false),
        (402, false, false),
        (429, true, false),
        (503, false, false),
    ] {
        let state = Executors::default();
        let lease = state.acquire(&a, now).unwrap();
        lease
            .failed(
                ApiError::failure(
                    status,
                    if matches!(status, 401 | 403) {
                        br#"{"error":{"message":"billing required"}}"#
                    } else {
                        b""
                    },
                    Some(7200),
                ),
                now,
            )
            .unwrap();
        assert_eq!(
            state.unavailable(&b.health, now).is_some(),
            blocks_sibling,
            "status {status}"
        );
        assert_eq!(
            state.unavailable(&other_model.health, now).is_some(),
            blocks_other_model,
            "status {status}"
        );
        assert!(state.unavailable(&other_pool.health, now).is_none());
        assert_eq!(
            state
                .unavailable(&a.health, now)
                .unwrap()
                .retry_after_seconds,
            7200
        );
    }
    let mut body = crate::gateway::body::Status::default();
    // Local handoff deadlines keep their client error but cannot poison health.
    let state = Executors::default();
    let lease = state.acquire(&a, now).unwrap();
    assert!(lease.failed(body.timeout_error(), now).is_none());
    body.waiting_upstream
        .store(true, std::sync::atomic::Ordering::Release);
    assert!(lease.failed(body.timeout_error(), now).is_some());
    *body.failure_head.get_mut().unwrap() = Some(ApiError::failure(429, b"", Some(9)));
    assert!(matches!(
        body.timeout_error().recovery.class,
        Some(RecoveryClass::RateLimit)
    ));
}

#[test]
fn recovery_backoff_is_bounded_and_success_resets_it() {
    let (a, _) = candidates();
    let state = Executors::default();
    let mut now = Instant::now();
    for seconds in [1, 2, 4, 8, 16, 32, 60, 60] {
        let lease = state.acquire(&a, now).unwrap();
        let observation = lease
            .failed(ApiError::transport(RecoveryClass::Upstream), now)
            .unwrap();
        assert_eq!(observation.retry_after_seconds, seconds);
        drop(lease);
        assert!(state.acquire(&a, now).is_err());
        now += Duration::from_secs(seconds);
    }
    let recovered = state.acquire(&a, now).unwrap();
    assert!(recovered.completed(None, now).0);
    drop(recovered);
    let next = state.acquire(&a, now).unwrap();
    assert_eq!(
        next.failed(ApiError::transport(RecoveryClass::Server), now)
            .unwrap()
            .retry_after_seconds,
        1
    );
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
        .failed(ApiError::transport(RecoveryClass::Upstream), now)
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
    assert_eq!(ordered[0].cache_scope.as_str(), "synthetic-b");
    state.order(None, &mut ordered, now + Duration::from_secs(1)); // stable input order when no preference
    assert_eq!(ordered[0].cache_scope.as_str(), "synthetic-b");
}
