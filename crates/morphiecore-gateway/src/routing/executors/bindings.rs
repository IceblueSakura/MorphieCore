//! Best-effort conversation preferences; only successful delivery stores or renews one.
use super::Key;
use std::{collections::HashMap, time::Duration};
use tokio::time::Instant;

struct Binding {
    target: String,
    successful_at: Instant,
}
pub(super) struct Bindings {
    entries: HashMap<Key, Binding>,
    ttl: Duration,
    capacity: usize,
}
impl Bindings {
    pub(super) fn new(ttl: Duration, capacity: usize) -> Self {
        Self {
            entries: HashMap::new(),
            ttl,
            capacity,
        }
    }
    pub(super) fn preferred(&self, key: &Key, now: Instant) -> Option<&str> {
        let binding = self.entries.get(key)?;
        (now.saturating_duration_since(binding.successful_at) < self.ttl)
            .then_some(binding.target.as_str())
    }
    pub(super) fn commit(&mut self, key: Key, target: &str, now: Instant) {
        if self.capacity == 0 || self.ttl.is_zero() {
            return;
        }
        self.entries
            .retain(|_, binding| now.saturating_duration_since(binding.successful_at) < self.ttl);
        if !self.entries.contains_key(&key)
            && self.entries.len() >= self.capacity
            && let Some(oldest) = self
                .entries
                .iter()
                .min_by_key(|(_, binding)| binding.successful_at)
                .map(|(key, _)| *key)
        {
            self.entries.remove(&oldest);
        }
        self.entries.insert(
            key,
            Binding {
                target: target.into(),
                successful_at: now,
            },
        );
    }
    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.entries.len()
    }
}
