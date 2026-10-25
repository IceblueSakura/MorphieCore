//! Gateway-owned request references, independent of diagnostic sinks and client labels.
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

#[derive(Clone, Debug)]
pub(in crate::gateway) struct RequestId(pub Arc<str>);

pub(in crate::gateway) struct RequestIds {
    namespace: String,
    next: AtomicU64,
}
impl RequestIds {
    pub fn new() -> Result<Self, getrandom::Error> {
        let mut nonce = [0u8; 16];
        getrandom::fill(&mut nonce)?;
        Ok(Self {
            namespace: nonce.iter().map(|b| format!("{b:02x}")).collect(),
            next: AtomicU64::new(1),
        })
    }
    /// Exhaustion leaves correlation unknown rather than reusing a reference or
    /// rejecting otherwise valid business traffic.
    pub fn next(&self) -> Option<RequestId> {
        self.next
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
            .ok()
            .map(|n| RequestId(format!("{}:{n}", self.namespace).into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn references_are_unique_across_requests_and_runtimes_without_wraparound() {
        let ids = RequestIds::new().unwrap();
        let first = ids.next().unwrap();
        let second = ids.next().unwrap();
        assert_ne!(first.0, second.0);
        assert_eq!(first.0.split(':').next(), second.0.split(':').next());
        assert_ne!(
            first.0.split(':').next(),
            RequestIds::new()
                .unwrap()
                .next()
                .unwrap()
                .0
                .split(':')
                .next()
        );
        ids.next.store(u64::MAX, Ordering::Relaxed);
        assert!(ids.next().is_none());
        assert!(ids.next().is_none());
    }
}
