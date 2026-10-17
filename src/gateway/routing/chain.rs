//! Stable successor eligibility and per-exchange accounting, without I/O.
use crate::{
    execution::{DeliveryState, fallback::Transition},
    gateway::BoundCandidate,
    provider::ErrorClass,
    topology::{EndpointId, RoutePolicy},
};
use std::{collections::HashMap, sync::Arc};

pub(in crate::gateway) struct Chain<'a> {
    candidates: &'a [Arc<BoundCandidate>],
    policy: &'a RoutePolicy,
    pub attempts: usize,
    endpoint_attempts: HashMap<EndpointId, usize>,
    failures: Vec<(usize, ErrorClass)>,
}
impl<'a> Chain<'a> {
    pub fn new(candidates: &'a [Arc<BoundCandidate>], policy: &'a RoutePolicy) -> Self {
        Self {
            candidates,
            policy,
            attempts: 0,
            endpoint_attempts: HashMap::new(),
            failures: vec![],
        }
    }
    pub fn skip(&self, index: usize) -> Option<&'static str> {
        if self.attempts >= self.policy.max_attempts {
            return Some("route_budget");
        }
        let next = &self.candidates[index];
        if self
            .endpoint_attempts
            .get(&next.endpoint.id)
            .copied()
            .unwrap_or(0)
            >= next.credential_attempts
        {
            return Some("pool_budget");
        }
        if self.failures.iter().any(|&(index, class)| {
            let previous = &self.candidates[index];
            !Transition::between(
                &previous.endpoint,
                &next.endpoint,
                previous.credential_fallback,
            )
            .may_advance(
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
    pub fn begin(&mut self, index: usize) {
        self.attempts += 1;
        *self
            .endpoint_attempts
            .entry(self.candidates[index].endpoint.id.clone())
            .or_default() += 1;
    }
    pub fn failed(&mut self, index: usize, class: ErrorClass) {
        self.failures.push((index, class));
    }
    /// Hold lifecycle frames only while a later candidate could legally answer
    /// an upstream failure. Pool budget and all prior exclusions still apply.
    pub fn could_follow(&self, current: usize, next: usize) -> bool {
        let current = &self.candidates[current];
        self.skip(next).is_none()
            && Transition::between(
                &current.endpoint,
                &self.candidates[next].endpoint,
                current.credential_fallback,
            )
            .may_advance(
                self.policy,
                DeliveryState::Uncommitted,
                ErrorClass::Upstream,
                self.attempts,
                true,
            )
    }
}

#[cfg(test)]
mod tests;
