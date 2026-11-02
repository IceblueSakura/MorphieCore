//! Stable successor eligibility and per-exchange accounting, without I/O.
use crate::{
    execution::{DeliveryState, fallback::Transition},
    gateway::BoundCandidate,
    provider::RecoveryClass,
    topology::RoutePolicy,
};
use std::sync::Arc;

pub(in crate::gateway) struct Chain<'a> {
    candidates: &'a [Arc<BoundCandidate>],
    policy: &'a RoutePolicy,
    pub attempts: usize,
    failures: Vec<(usize, RecoveryClass)>,
}
impl<'a> Chain<'a> {
    pub fn new(candidates: &'a [Arc<BoundCandidate>], policy: &'a RoutePolicy) -> Self {
        Self {
            candidates,
            policy,
            attempts: 0,
            failures: vec![],
        }
    }
    pub fn skip(&self, index: usize) -> Option<&'static str> {
        if self.attempts >= self.policy.max_attempts {
            return Some("route_budget");
        }
        let next = &self.candidates[index];
        if self.failures.iter().any(|&(index, class)| {
            let previous = &self.candidates[index];
            !Transition::between(&previous.endpoint, &next.endpoint, false).may_advance(
                self.policy,
                DeliveryState::Uncommitted,
                class,
                self.attempts,
                true,
            )
        }) {
            return Some("failure_scope");
        }
        None
    }
    pub fn begin(&mut self, _index: usize) {
        self.attempts += 1;
    }
    pub fn failed(&mut self, index: usize, class: RecoveryClass) {
        self.failures.push((index, class));
    }
    /// Hold lifecycle frames only while a later candidate could legally answer
    /// an upstream failure. All prior exclusions still apply.
    pub fn could_follow(&self, current: usize, next: usize) -> bool {
        let current = &self.candidates[current];
        self.skip(next).is_none()
            && Transition::between(&current.endpoint, &self.candidates[next].endpoint, false)
                .may_advance(
                    self.policy,
                    DeliveryState::Uncommitted,
                    RecoveryClass::Upstream,
                    self.attempts,
                    true,
                )
    }
}

#[cfg(test)]
mod tests;
