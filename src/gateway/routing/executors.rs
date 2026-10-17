//! Bounded private health domains and exact-executor conversation preferences.
//! Recovery uses foreground request ownership, never background calls or queues.
use crate::gateway::{ApiError, BoundCandidate};
use crate::provider::ErrorClass;
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::time::Instant;

const MAX_CONVERSATIONS: usize = 4096;
const AFFINITY_TTL: Duration = Duration::from_secs(3600);
const FORGET_FAILURES: Duration = Duration::from_secs(1800);
type Key = [u8; 32];

/// Credential/origin, exact credential/deployment/model, deployment/model pool.
/// These keys never appear in diagnostics or downstream data.
#[derive(Clone)]
pub(in crate::gateway) struct Scopes([Key; 3]);
impl Scopes {
    pub(in crate::gateway) fn bind(
        auth: &[u8; 32],
        endpoint: &crate::topology::Endpoint,
        provenance: &str,
        pool: &[String],
    ) -> Self {
        let digest = |kind: &str, fields: &[&str]| {
            let mut hash = Sha256::new();
            hash.update(auth);
            for value in [
                kind,
                endpoint.provider.as_str(),
                endpoint.target.origin.as_str(),
            ]
            .into_iter()
            .chain(fields.iter().copied())
            {
                hash.update((value.len() as u64).to_le_bytes());
                hash.update(value.as_bytes());
            }
            hash.finalize().into()
        };
        let path = endpoint.target.path.as_str();
        let model = endpoint.upstream_model.as_str();
        let mut members: Vec<_> = pool.iter().map(String::as_str).collect();
        members.sort_unstable();
        let mut fields = vec![path, model];
        fields.extend(members);
        Self([
            digest("health.credential.v1", &[provenance]),
            digest("health.target.v1", &[path, model, provenance]),
            digest("health.pool.v1", &fields),
        ])
    }
}

#[derive(Clone, Copy, Debug, serde::Serialize)]
pub(in crate::gateway) struct Observation {
    pub reason: &'static str,
    pub scope: &'static str,
    pub source: &'static str,
    pub retry_after_seconds: u64,
}
#[derive(Clone, Copy)]
struct Cooling {
    until: Instant,
    observation: Observation,
}
#[derive(Default)]
struct Health {
    generation: u64,
    failures: u32,
    cooling: Option<Cooling>,
    probe: Option<u64>,
}
#[derive(Default)]
struct State {
    // Keys come only from bound candidates. Keep generations after recovery so
    // old in-flight attempts cannot regain authority through delete/reinsert.
    health: HashMap<Key, Health>,
    next_ticket: u64,
    affinity: HashMap<(String, String), (String, Instant)>,
}
#[derive(Default)]
pub(in crate::gateway) struct Executors {
    state: Mutex<State>,
}
fn remaining(until: Instant, now: Instant) -> u64 {
    let duration = until.saturating_duration_since(now);
    duration.as_secs() + u64::from(duration.subsec_nanos() != 0)
}
fn blocked(state: &State, scopes: &Scopes, now: Instant) -> Option<Observation> {
    // A candidate must satisfy every scope: use its latest blocking deadline.
    scopes
        .0
        .iter()
        .filter_map(|key| {
            let health = state.health.get(key)?;
            let cooling = health.cooling?;
            if cooling.until > now || health.probe.is_some() {
                Some(Observation {
                    reason: if health.probe.is_some() {
                        "probing"
                    } else {
                        "cooling"
                    },
                    retry_after_seconds: remaining(cooling.until, now).max(1),
                    ..cooling.observation
                })
            } else {
                None
            }
        })
        .max_by_key(|observation| observation.retry_after_seconds)
}
impl Executors {
    pub(in crate::gateway) fn order(
        &self,
        model: &str,
        conversation: Option<&str>,
        candidates: &mut [Arc<BoundCandidate>],
    ) -> &'static str {
        let now = Instant::now();
        let state = self.state.lock().expect("executor state");
        let preferred = conversation
            .and_then(|c| state.affinity.get(&(model.to_owned(), c.to_owned())))
            .filter(|(_, at)| now.duration_since(*at) < AFFINITY_TTL)
            .map(|(id, _)| id.as_str());
        let cooling = |c: &BoundCandidate| blocked(&state, &c.health, now).is_some();
        let affine = candidates
            .iter()
            .any(|c| !cooling(c) && Some(c.cache_scope.as_str()) == preferred);
        let cooled = candidates.iter().any(|c| cooling(c));
        candidates.sort_by_key(|c| (cooling(c), Some(c.cache_scope.as_str()) != preferred));
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
    /// Check and acquire all affected recovery scopes atomically, immediately
    /// before dispatch. Merely considering a successor never reserves a probe.
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
        let mut generations = [0; 3];
        let mut probing = false;
        for (index, key) in candidate.health.0.iter().enumerate() {
            let health = state.health.entry(*key).or_default();
            generations[index] = health.generation;
            if health.cooling.is_some() {
                health.probe = Some(ticket);
                probing = true;
            }
        }
        Ok(Lease {
            owner: self,
            candidate,
            ticket,
            generations,
            probing,
        })
    }
}

/// Drop releases recovery ownership on cancellation, panic or a local failure.
/// It never records a successful or failed upstream interaction by itself.
pub(in crate::gateway) struct Lease<'a> {
    owner: &'a Executors,
    candidate: &'a BoundCandidate,
    ticket: u64,
    generations: [u64; 3],
    pub probing: bool,
}
impl Lease<'_> {
    pub(in crate::gateway) fn completed(
        &self,
        model: &str,
        conversation: Option<&str>,
        now: Instant,
    ) -> bool {
        let mut state = self.owner.state.lock().expect("executor state");
        let fresh = self
            .candidate
            .health
            .0
            .iter()
            .enumerate()
            .all(|(index, key)| {
                state
                    .health
                    .get(key)
                    .is_some_and(|h| h.generation == self.generations[index])
            });
        if !fresh {
            return false;
        }
        for key in &self.candidate.health.0 {
            let health = state.health.get_mut(key).expect("acquired scope");
            if health.cooling.take().is_some() {
                health.generation += 1;
            }
            health.failures = 0;
            if health.probe == Some(self.ticket) {
                health.probe = None;
            }
        }
        let Some(conversation) = conversation else {
            return true;
        };
        if state.affinity.len() >= MAX_CONVERSATIONS {
            state
                .affinity
                .retain(|_, (_, at)| now.duration_since(*at) < AFFINITY_TTL);
        }
        let key = (model.to_owned(), conversation.to_owned());
        if state.affinity.len() >= MAX_CONVERSATIONS
            && !state.affinity.contains_key(&key)
            && let Some(oldest) = state
                .affinity
                .iter()
                .min_by_key(|(_, (_, at))| *at)
                .map(|(key, _)| key.clone())
        {
            state.affinity.remove(&oldest);
        }
        state
            .affinity
            .insert(key, (self.candidate.cache_scope.as_str().to_owned(), now));
        true
    }
    pub(in crate::gateway) fn failed(&self, error: ApiError, now: Instant) -> Option<Observation> {
        if !error.health {
            return None;
        }
        let (index, scope, transient) = match error.fallback? {
            ErrorClass::CredentialUnavailable | ErrorClass::Authentication => {
                (0, "credential", false)
            }
            ErrorClass::PermissionDenied | ErrorClass::Quota => (1, "target", false),
            ErrorClass::RateLimit => (2, "pool", false),
            ErrorClass::Server | ErrorClass::Timeout | ErrorClass::Upstream => (1, "target", true),
            _ => return None,
        };
        let mut state = self.owner.state.lock().expect("executor state");
        let health = state
            .health
            .get_mut(&self.candidate.health.0[index])
            .expect("acquired scope");
        let previous = health.cooling;
        if previous.is_none_or(|c| now.saturating_duration_since(c.until) >= FORGET_FAILURES) {
            health.failures = 1;
        } else if previous.is_some_and(|c| c.until <= now) {
            health.failures = health.failures.saturating_add(1);
        }
        let base = if transient { 1u64 } else { 60 };
        let cap = if transient { 60 } else { 1800 };
        let delay = error
            .retry_after
            .unwrap_or_else(|| (base << health.failures.saturating_sub(1).min(10)).min(cap));
        let mut cooling = Cooling {
            until: now + Duration::from_secs(delay.min(86400)),
            observation: Observation {
                reason: "failure",
                scope,
                source: if error.retry_after.is_some() {
                    "retry_after"
                } else {
                    "backoff"
                },
                retry_after_seconds: delay.min(86400),
            },
        };
        if let Some(previous) = previous
            && previous.until > cooling.until
        {
            cooling = previous;
        }
        health.generation += 1;
        health.cooling = Some(cooling);
        // Keep any in-flight probe reserved until its own lease is released.
        Some(Observation {
            reason: "failure",
            retry_after_seconds: remaining(cooling.until, now),
            ..cooling.observation
        })
    }
}
impl Drop for Lease<'_> {
    fn drop(&mut self) {
        let mut state = self.owner.state.lock().expect("executor state");
        for key in &self.candidate.health.0 {
            if let Some(health) = state.health.get_mut(key)
                && health.probe == Some(self.ticket)
            {
                health.probe = None;
            }
        }
    }
}

#[cfg(test)]
mod tests;
