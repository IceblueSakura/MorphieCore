//! Success-owned affinity epochs; absent observations live only with requests.
use super::Key;
use std::{collections::HashMap, sync::Arc, time::Duration};
use tokio::time::Instant;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::gateway) enum Commit {
    Committed,
    Stale,
    NotApplicable,
}
struct Binding {
    target: String,
    successful_at: Instant,
}
struct Entry {
    epoch: Arc<()>,
    binding: Option<Binding>,
}
pub(super) struct Observed {
    key: Key,
    epoch: Arc<()>,
}
pub(super) struct Bindings {
    entries: HashMap<Key, Entry>,
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
    fn expired(&self, entry: &Entry, now: Instant) -> bool {
        entry
            .binding
            .as_ref()
            .is_some_and(|binding| now.saturating_duration_since(binding.successful_at) >= self.ttl)
    }
    pub(super) fn observe(&mut self, key: Key, now: Instant) -> Option<Observed> {
        if self.capacity == 0 || self.ttl.is_zero() {
            return None;
        }
        if self
            .entries
            .get(&key)
            .is_some_and(|entry| self.expired(entry, now))
        {
            self.entries.remove(&key);
        }
        let entry = self.entries.entry(key).or_insert_with(|| Entry {
            epoch: Arc::new(()),
            binding: None,
        });
        Some(Observed {
            key,
            epoch: entry.epoch.clone(),
        })
    }
    pub(super) fn preferred(&self, observed: &Observed, now: Instant) -> Option<&str> {
        let entry = self.entries.get(&observed.key)?;
        (!self.expired(entry, now))
            .then_some(entry.binding.as_ref()?)
            .map(|binding| binding.target.as_str())
    }
    pub(super) fn commit(&mut self, observed: &Observed, target: &str, now: Instant) -> Commit {
        let Some(entry) = self.entries.get(&observed.key) else {
            return Commit::Stale;
        };
        if self.expired(entry, now) {
            self.entries.remove(&observed.key);
            return Commit::Stale;
        }
        if !Arc::ptr_eq(&entry.epoch, &observed.epoch) {
            return Commit::Stale;
        }
        if entry.binding.is_none()
            && self
                .entries
                .values()
                .filter(|entry| entry.binding.is_some())
                .count()
                >= self.capacity
        {
            let ttl = self.ttl;
            self.entries.retain(|_, entry| {
                entry.binding.as_ref().is_none_or(|binding| {
                    now.saturating_duration_since(binding.successful_at) < ttl
                })
            });
            if self
                .entries
                .values()
                .filter(|entry| entry.binding.is_some())
                .count()
                >= self.capacity
                && let Some(oldest) = self
                    .entries
                    .iter()
                    .filter_map(|(key, entry)| {
                        entry
                            .binding
                            .as_ref()
                            .map(|binding| (*key, binding.successful_at))
                    })
                    .min_by_key(|(_, successful_at)| *successful_at)
                    .map(|(key, _)| key)
            {
                self.entries.remove(&oldest);
            }
        }
        let entry = self
            .entries
            .get_mut(&observed.key)
            .expect("observed affinity slot");
        // Every commit fences other observers, including same-target renewals.
        entry.epoch = Arc::new(());
        entry.binding = Some(Binding {
            target: target.into(),
            successful_at: now,
        });
        Commit::Committed
    }
    pub(super) fn release(&mut self, observed: &Observed) {
        if self.entries.get(&observed.key).is_some_and(|entry| {
            entry.binding.is_none()
                && Arc::ptr_eq(&entry.epoch, &observed.epoch)
                && Arc::strong_count(&entry.epoch) == 2
        }) {
            self.entries.remove(&observed.key);
        }
    }
    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.entries.len()
    }
}
