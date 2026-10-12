//! In-memory executor health and conversation affinity. Both only reorder a
//! request's legal candidates for failover; neither creates nor removes one.
use super::BoundCandidate;
use crate::provider::ErrorClass;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

const MAX_CONVERSATIONS: usize = 4096;
/// Affinity outlives a typical pause between agent turns, not a day of idling.
const AFFINITY_TTL: Duration = Duration::from_secs(3600);
/// Cooldown without a reported reset, and the cap on a reported one.
const DEFAULT_COOLDOWN: Duration = Duration::from_secs(60);
const MAX_COOLDOWN: Duration = Duration::from_secs(3600);

/// The executor is the candidate's credential × Provider × upstream model, as its
/// cache scope digests them; Chat and Responses endpoints of one binding share it.
fn identity(candidate: &BoundCandidate) -> &str {
    candidate.cache_scope.as_str()
}

#[derive(Default)]
pub(super) struct Executors {
    state: Mutex<State>,
}
#[derive(Default)]
struct State {
    cooling: HashMap<String, Instant>,
    /// (public model, conversation) → the executor that last completed honestly.
    affinity: HashMap<(String, String), (String, Instant)>,
}
impl Executors {
    /// Compiled order, except that the conversation's executor goes first and
    /// cooling executors go last (still a last resort, never removed).
    pub(super) fn order(
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
        let cooling = |c: &BoundCandidate| state.cooling.get(identity(c)).is_some_and(|t| *t > now);
        let affine = candidates
            .iter()
            .any(|c| !cooling(c) && Some(identity(c)) == preferred);
        let cooled = candidates.iter().any(|c| cooling(c));
        candidates.sort_by_key(|c| (cooling(c), Some(identity(c)) != preferred));
        match (affine, cooled) {
            (true, true) => "affinity_cooling",
            (true, false) => "affinity",
            (false, true) => "cooling",
            (false, false) => "compiled",
        }
    }
    pub(super) fn completed(
        &self,
        model: &str,
        conversation: Option<&str>,
        candidate: &BoundCandidate,
    ) {
        let now = Instant::now();
        let mut state = self.state.lock().expect("executor state");
        state.cooling.remove(identity(candidate));
        let Some(conversation) = conversation else {
            return;
        };
        if state.affinity.len() >= MAX_CONVERSATIONS {
            state
                .affinity
                .retain(|_, (_, at)| now.duration_since(*at) < AFFINITY_TTL);
        }
        if state.affinity.len() >= MAX_CONVERSATIONS
            && let Some(oldest) = state
                .affinity
                .iter()
                .min_by_key(|(_, (_, at))| *at)
                .map(|(key, _)| key.clone())
        {
            state.affinity.remove(&oldest);
        }
        state.affinity.insert(
            (model.to_owned(), conversation.to_owned()),
            (identity(candidate).to_owned(), now),
        );
    }
    /// Rate limits, exhausted quota and rejected credentials are the executor's
    /// own state; server errors and timeouts are not.
    pub(super) fn failed(
        &self,
        candidate: &BoundCandidate,
        class: ErrorClass,
        retry_after: Option<u64>,
    ) {
        if !matches!(
            class,
            ErrorClass::RateLimit
                | ErrorClass::Quota
                | ErrorClass::Authentication
                | ErrorClass::PermissionDenied
        ) {
            return;
        }
        let pause = retry_after
            .map_or(DEFAULT_COOLDOWN, Duration::from_secs)
            .min(MAX_COOLDOWN);
        self.state
            .lock()
            .expect("executor state")
            .cooling
            .insert(identity(candidate).to_owned(), Instant::now() + pause);
    }
}
