//! Bounded account/target health and success-owned conversation preferences.
//! Foreground probes are atomic; neither health nor affinity performs network I/O.
use crate::{
    gateway::{ApiError, BoundCandidate},
    provider::{RecoveryClass, RecoveryPolicy},
};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashMap},
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::time::Instant;
mod bindings;
pub(in crate::gateway) use bindings::Commit as AffinityCommit;
type Key = [u8; 32];

/// Account identity excludes origin; target identity includes operation and model.
#[derive(Clone)]
pub(in crate::gateway) struct Scopes([Key; 2]);
impl Scopes {
    pub(in crate::gateway) fn bind(
        namespace: &[u8; 32],
        endpoint: &crate::topology::Endpoint,
        provenance: &str,
    ) -> Self {
        let digest = |kind: &str, target: bool| {
            let mut hash = Sha256::new();
            hash.update(namespace);
            for value in [kind, endpoint.provider.as_str(), provenance] {
                hash.update((value.len() as u64).to_le_bytes());
                hash.update(value.as_bytes());
            }
            if target {
                hash.update([endpoint.protocol.identity_tag()]);
                for value in [
                    endpoint.target.origin.as_str(),
                    endpoint.target.path.as_str(),
                    endpoint.upstream_model.as_str(),
                ] {
                    hash.update((value.len() as u64).to_le_bytes());
                    hash.update(value.as_bytes());
                }
            }
            hash.finalize().into()
        };
        Self([
            digest("health.account.v2", false),
            digest("health.target.v2", true),
        ])
    }
}
#[derive(Clone, Copy, Debug, serde::Serialize)]
pub(in crate::gateway) struct Observation {
    pub reason: &'static str,
    pub scope: &'static str,
    pub source: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_after_seconds: Option<u64>,
    #[serde(skip)]
    pub rate_or_quota_only: bool,
}
#[derive(Clone, Copy)]
pub(in crate::gateway) struct FailureObservation {
    pub error: ApiError,
    pub at: Instant,
    pub material_revision: Option<u64>,
}
#[derive(Clone, Copy, Eq, PartialEq, Ord, PartialOrd)]
enum Class {
    Auth,
    Quota,
    Transient,
    RateLimit,
    Permission,
}
impl Class {
    fn from(error: ApiError) -> Option<Self> {
        if !error.recovery.affects_health {
            return None;
        }
        Some(match error.recovery.class? {
            RecoveryClass::Auth => Self::Auth,
            RecoveryClass::Quota => Self::Quota,
            RecoveryClass::Transient => Self::Transient,
            RecoveryClass::RateLimit => Self::RateLimit,
            RecoveryClass::Permission => Self::Permission,
            _ => return None,
        })
    }
    fn scope(self) -> usize {
        if matches!(self, Self::Auth | Self::Quota) {
            0
        } else {
            1
        }
    }
    fn rate_or_quota(self) -> bool {
        matches!(self, Self::RateLimit | Self::Quota)
    }
}
#[derive(Clone, Copy)]
struct Cooling {
    until: Instant,
    source: &'static str,
    material_revision: Option<u64>,
}
impl Cooling {
    fn superseded(self, class: Class, material: Option<u64>) -> bool {
        class == Class::Auth
            && self
                .material_revision
                .zip(material)
                .is_some_and(|(old, current)| current > old)
    }
}
#[derive(Default)]
struct Health {
    generation: u64,
    failures: u32,
    last_failure: Option<Instant>,
    constraints: BTreeMap<Class, Cooling>,
    probe: Option<u64>,
    target_probes: usize,
    latest_material: Option<u64>,
}
struct State {
    health: HashMap<Key, Health>,
    next_ticket: u64,
    affinity: bindings::Bindings,
}
pub(in crate::gateway) struct Executors {
    state: Arc<Mutex<State>>,
    policies: BTreeMap<String, RecoveryPolicy>,
    defaults: RecoveryPolicy,
}
pub(in crate::gateway) struct BindingObservation {
    state: Arc<Mutex<State>>,
    observed: bindings::Observed,
}
impl Drop for BindingObservation {
    fn drop(&mut self) {
        self.state
            .lock()
            .expect("executor state")
            .affinity
            .release(&self.observed);
    }
}
impl Default for Executors {
    fn default() -> Self {
        let limits = crate::gateway::Limits::default();
        Self::new(
            limits.affinity_ttl,
            limits.affinity_capacity,
            limits.provider_recovery,
        )
    }
}
fn remaining(until: Instant, now: Instant) -> u64 {
    let delay = until.saturating_duration_since(now);
    delay.as_secs() + u64::from(delay.subsec_nanos() != 0)
}
fn blocking(
    state: &State,
    scopes: &Scopes,
    now: Instant,
) -> Option<(Option<Instant>, Observation)> {
    let mut latest: Option<(Instant, Observation)> = None;
    let mut rate_only = true;
    for (index, key) in scopes.0.iter().enumerate() {
        let Some(health) = state.health.get(key) else {
            continue;
        };
        let scope = if index == 0 { "account" } else { "target" };
        if health.probe.is_some()
            || (index == 0 && !health.constraints.is_empty() && health.target_probes > 0)
        {
            return Some((
                None,
                Observation {
                    reason: "probing",
                    scope,
                    source: "probe",
                    retry_after_seconds: None,
                    rate_or_quota_only: false,
                },
            ));
        }
        for (class, cooling) in &health.constraints {
            if cooling.until <= now || cooling.superseded(*class, health.latest_material) {
                continue;
            }
            rate_only &= class.rate_or_quota();
            if latest
                .as_ref()
                .is_none_or(|(until, _)| *until < cooling.until)
            {
                latest = Some((
                    cooling.until,
                    Observation {
                        reason: "cooling",
                        scope,
                        source: cooling.source,
                        retry_after_seconds: Some(remaining(cooling.until, now)),
                        rate_or_quota_only: false,
                    },
                ));
            }
        }
    }
    latest.map(|(until, mut observation)| {
        observation.rate_or_quota_only = rate_only;
        (Some(until), observation)
    })
}
fn blocked(state: &State, scopes: &Scopes, now: Instant) -> Option<Observation> {
    blocking(state, scopes, now).map(|(_, observation)| observation)
}
fn delay(
    policy: &RecoveryPolicy,
    class: Class,
    level: u32,
    hint: Option<u64>,
) -> (Duration, &'static str) {
    let (fallback, cap) = match class {
        Class::Auth => (policy.auth_cooldown_ms, policy.auth_cooldown_ms),
        Class::Permission => (policy.permission_cooldown_ms, policy.permission_cooldown_ms),
        Class::Quota => (policy.quota_cooldown_ms, policy.quota_reset_max_ms),
        Class::Transient => (
            policy
                .transient_cooldown_ms
                .saturating_mul(
                    u64::from(policy.transient_multiplier).saturating_pow(level.saturating_sub(1)),
                )
                .min(policy.transient_max_cooldown_ms),
            policy.transient_max_cooldown_ms,
        ),
        Class::RateLimit => (
            policy
                .rate_limit_cooldown_ms
                .saturating_mul(
                    u64::from(policy.rate_limit_multiplier).saturating_pow(level.saturating_sub(1)),
                )
                .min(policy.rate_limit_max_cooldown_ms),
            policy.rate_limit_reset_max_ms,
        ),
    };
    if !matches!(class, Class::Auth | Class::Permission)
        && let Some(hint) = hint.filter(|hint| *hint > 0)
    {
        return (
            Duration::from_secs(hint)
                .max(Duration::from_millis(policy.reset_hint_floor_ms))
                .min(Duration::from_millis(cap)),
            "retry_after",
        );
    }
    (
        Duration::from_millis(fallback),
        if matches!(class, Class::Transient | Class::RateLimit) {
            "backoff"
        } else {
            "policy"
        },
    )
}
impl Executors {
    pub(in crate::gateway) fn new(
        ttl: Duration,
        capacity: usize,
        policies: BTreeMap<String, RecoveryPolicy>,
    ) -> Self {
        Self {
            state: Arc::new(Mutex::new(State {
                health: HashMap::new(),
                next_ticket: 0,
                affinity: bindings::Bindings::new(ttl, capacity),
            })),
            policies,
            defaults: RecoveryPolicy::default(),
        }
    }
    pub(in crate::gateway) fn policy(&self, candidate: &BoundCandidate) -> &RecoveryPolicy {
        self.policies
            .get(candidate.provider.id.as_str())
            .unwrap_or(&self.defaults)
    }
    pub(in crate::gateway) fn verified_material(&self, scopes: &Scopes, revision: u64) {
        let mut state = self.state.lock().expect("executor state");
        let health = state.health.entry(scopes.0[0]).or_default();
        health.latest_material = Some(
            health
                .latest_material
                .map_or(revision, |old| old.max(revision)),
        );
    }
    pub(in crate::gateway) fn observe(
        &self,
        agent: Option<&str>,
        model: &str,
        conversation: Option<&str>,
        now: Instant,
    ) -> Option<BindingObservation> {
        let (agent, conversation) = (agent?, conversation?);
        let mut hash = Sha256::new();
        hash.update(b"affinity.binding.v1");
        for value in [agent, model, conversation] {
            hash.update((value.len() as u64).to_le_bytes());
            hash.update(value.as_bytes());
        }
        let observed = self
            .state
            .lock()
            .expect("executor state")
            .affinity
            .observe(hash.finalize().into(), now)?;
        Some(BindingObservation {
            state: self.state.clone(),
            observed,
        })
    }
    /// Only an existing binding can earn one pre-dispatch cooldown wait.
    pub(in crate::gateway) fn preferred_wait<T: AsRef<BoundCandidate>>(
        &self,
        observation: Option<&BindingObservation>,
        candidates: &[T],
        now: Instant,
        max_wait: Duration,
    ) -> Option<Instant> {
        let state = self.state.lock().expect("executor state");
        let observed = observation.filter(|o| Arc::ptr_eq(&o.state, &self.state))?;
        let preferred = state.affinity.preferred(&observed.observed, now)?;
        let candidate = candidates
            .iter()
            .find(|c| c.as_ref().cache_scope().as_str() == preferred)?;
        let until = blocking(&state, &candidate.as_ref().health, now)?.0?;
        (until.saturating_duration_since(now) <= max_wait).then_some(until)
    }
    pub(in crate::gateway) fn order<T: AsRef<BoundCandidate>>(
        &self,
        observation: Option<&BindingObservation>,
        candidates: &mut [T],
        now: Instant,
    ) -> &'static str {
        let state = self.state.lock().expect("executor state");
        let preferred = observation
            .filter(|o| Arc::ptr_eq(&o.state, &self.state))
            .and_then(|o| state.affinity.preferred(&o.observed, now));
        let cooling = |c: &T| blocked(&state, &c.as_ref().health, now).is_some();
        let affine = candidates
            .iter()
            .any(|c| !cooling(c) && Some(c.as_ref().cache_scope().as_str()) == preferred);
        let cooled = candidates.iter().any(&cooling);
        candidates.sort_by_key(|c| {
            (
                cooling(c),
                Some(c.as_ref().cache_scope().as_str()) != preferred,
            )
        });
        match (affine, cooled) {
            (true, true) => "affinity_cooling",
            (true, false) => "affinity",
            (false, true) => "cooling",
            (false, false) => "compiled",
        }
    }
    pub(in crate::gateway) fn unavailable(
        &self,
        scopes: &Scopes,
        now: Instant,
    ) -> Option<Observation> {
        blocked(&self.state.lock().expect("executor state"), scopes, now)
    }
    /// Expired or replaced-material constraints require one atomic foreground probe.
    pub(in crate::gateway) fn acquire<'a>(
        &'a self,
        candidate: &'a BoundCandidate,
        now: Instant,
    ) -> Result<Lease<'a>, Observation> {
        let mut state = self.state.lock().expect("executor state");
        if let Some(observation) = blocked(&state, &candidate.health, now) {
            return Err(observation);
        }
        state.next_ticket += 1;
        let ticket = state.next_ticket;
        let mut generations = [0; 2];
        let mut probing = false;
        let mut target_probe = false;
        for (index, key) in candidate.health.0.iter().enumerate() {
            let health = state.health.entry(*key).or_default();
            generations[index] = health.generation;
            if !health.constraints.is_empty() {
                health.probe = Some(ticket);
                probing = true;
                target_probe |= index == 1;
            }
        }
        if target_probe {
            state
                .health
                .get_mut(&candidate.health.0[0])
                .expect("account scope")
                .target_probes += 1;
        }
        Ok(Lease {
            owner: self,
            candidate,
            ticket,
            generations,
            probing,
            target_probe,
            sent_material: Mutex::new(None),
        })
    }
}
/// Drop releases ownership only. Independently observed failures are resolved by the request owner.
pub(in crate::gateway) struct Lease<'a> {
    owner: &'a Executors,
    candidate: &'a BoundCandidate,
    ticket: u64,
    generations: [u64; 2],
    pub probing: bool,
    target_probe: bool,
    sent_material: Mutex<Option<u64>>,
}
impl Lease<'_> {
    pub(in crate::gateway) fn sent_material(&self, revision: Option<u64>) {
        *self.sent_material.lock().expect("sent material") = revision;
        if let Some(revision) = revision {
            self.owner
                .verified_material(&self.candidate.health, revision);
        }
    }
    pub(in crate::gateway) fn completed(
        &self,
        binding: Option<&BindingObservation>,
        now: Instant,
    ) -> (bool, AffinityCommit) {
        let material = *self.sent_material.lock().expect("sent material");
        let mut state = self.owner.state.lock().expect("executor state");
        let fresh = self.candidate.health.0.iter().enumerate().all(|(i, key)| {
            state
                .health
                .get(key)
                .is_some_and(|h| h.generation == self.generations[i])
        });
        if !fresh {
            return (
                false,
                if binding.is_some() {
                    AffinityCommit::Stale
                } else {
                    AffinityCommit::NotApplicable
                },
            );
        }
        for key in &self.candidate.health.0 {
            let health = state.health.get_mut(key).expect("acquired scope");
            let before = health.constraints.len();
            health.constraints.retain(|class, cooling| {
                *class == Class::Auth
                    && cooling.material_revision.is_some()
                    && health
                        .latest_material
                        .is_some_and(|latest| material.is_none_or(|sent| sent < latest))
            });
            if health.constraints.len() != before {
                health.generation += 1;
            }
            health.failures = 0;
            health.last_failure = None;
            if health.probe == Some(self.ticket) {
                health.probe = None;
            }
        }
        let affinity = match binding {
            None => AffinityCommit::NotApplicable,
            Some(binding) if Arc::ptr_eq(&binding.state, &self.owner.state) => {
                state.affinity.commit(
                    &binding.observed,
                    self.candidate.cache_scope().as_str(),
                    now,
                )
            }
            Some(_) => AffinityCommit::Stale,
        };
        (true, affinity)
    }
    #[cfg(test)]
    pub(in crate::gateway) fn failed(
        &self,
        error: ApiError,
        at: Instant,
        material_revision: Option<u64>,
    ) -> Option<Observation> {
        self.resolve_failures(
            &[FailureObservation {
                error,
                at,
                material_revision,
            }],
            at,
        )
    }
    /// One backoff advance per affected scope, even when two attempts differ.
    pub(in crate::gateway) fn resolve_failures(
        &self,
        failures: &[FailureObservation],
        now: Instant,
    ) -> Option<Observation> {
        let policy = self.owner.policy(self.candidate);
        let mut state = self.owner.state.lock().expect("executor state");
        let mut result: Option<(Instant, Observation)> = None;
        for (index, key) in self.candidate.health.0.iter().enumerate() {
            let health = state.health.get_mut(key).expect("acquired scope");
            let selected: Vec<_> = failures
                .iter()
                .filter_map(|failure| {
                    let class = Class::from(failure.error)?;
                    if class.scope() != index
                        || (class == Class::Auth
                            && failure
                                .material_revision
                                .zip(health.latest_material)
                                .is_some_and(|(old, current)| old < current))
                    {
                        return None;
                    }
                    Some((failure, class))
                })
                .collect();
            if selected.is_empty() {
                continue;
            }
            let first = selected.iter().map(|(f, _)| f.at).min().expect("failure");
            let last = selected.iter().map(|(f, _)| f.at).max().expect("failure");
            if health.last_failure.is_none_or(|last| {
                first.saturating_duration_since(last)
                    >= Duration::from_millis(policy.failure_free_reset_ms)
            }) {
                health.failures = 1;
            } else if health
                .constraints
                .values()
                .all(|cooling| cooling.until <= first)
            {
                health.failures = health.failures.saturating_add(1).max(1);
            }
            health.last_failure = Some(health.last_failure.map_or(last, |old| old.max(last)));
            for (failure, class) in selected {
                let (delay, source) =
                    delay(policy, class, health.failures, failure.error.retry_after);
                let mut cooling = Cooling {
                    until: failure.at + delay,
                    source,
                    material_revision: failure.material_revision,
                };
                if let Some(previous) = health.constraints.get(&class)
                    && previous.until > cooling.until
                {
                    cooling.until = previous.until;
                    cooling.source = previous.source;
                }
                health.constraints.insert(class, cooling);
            }
            health.generation += 1;
            if let Some((_, cooling)) = health.constraints.iter().max_by_key(|(_, c)| c.until)
                && result
                    .as_ref()
                    .is_none_or(|(until, _)| *until < cooling.until)
            {
                result = Some((
                    cooling.until,
                    Observation {
                        reason: "failure",
                        scope: if index == 0 { "account" } else { "target" },
                        source: cooling.source,
                        retry_after_seconds: Some(remaining(cooling.until, now)),
                        rate_or_quota_only: health
                            .constraints
                            .keys()
                            .all(|class| class.rate_or_quota()),
                    },
                ));
            }
        }
        result.map(|(_, observation)| observation)
    }
}
impl Drop for Lease<'_> {
    fn drop(&mut self) {
        let mut state = self.owner.state.lock().expect("executor state");
        if self.target_probe {
            state
                .health
                .get_mut(&self.candidate.health.0[0])
                .expect("account scope")
                .target_probes -= 1;
        }
        for key in &self.candidate.health.0 {
            if let Some(h) = state.health.get_mut(key)
                && h.probe == Some(self.ticket)
            {
                h.probe = None;
            }
        }
    }
}
#[cfg(test)]
mod tests;
