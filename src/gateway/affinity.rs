//! Disposable prefix metadata. Never stores bodies, selects credentials or restores history.
use std::{
    collections::VecDeque,
    sync::Mutex,
    time::{Duration, Instant},
};

const TTL: Duration = Duration::from_secs(300);
const MAX_RECORDS: usize = 4096;
const MAX_BYTES: usize = 1 << 20;
type Digest = [u8; 32];
type Group = [u8; 16];
struct Record {
    scope: Digest,
    prefix: Digest,
    group: Option<Group>,
    expires: Instant,
}
pub(super) struct Index {
    records: Option<Mutex<VecDeque<Record>>>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Lookup {
    Miss,
    Hit,
    Ambiguous,
    Unavailable,
}
impl Index {
    pub(super) fn new(enabled: bool) -> Self {
        let mut records = VecDeque::new();
        let capacity = MAX_RECORDS.min(MAX_BYTES / std::mem::size_of::<Record>());
        let records =
            (enabled && records.try_reserve_exact(capacity).is_ok()).then(|| Mutex::new(records));
        Self { records }
    }
    pub(super) fn lookup(
        &self,
        scope: Digest,
        prefixes: &[Digest],
        now: Instant,
    ) -> (Lookup, Option<Group>) {
        let Some(mut records) = self.records.as_ref().and_then(|r| r.try_lock().ok()) else {
            return (Lookup::Unavailable, None);
        };
        records.retain(|r| now < r.expires);
        for prefix in prefixes
            .iter()
            .rev()
            .take(crate::protocol::cache_affinity::MAX_CHECKPOINTS)
        {
            if let Some(record) = records
                .iter()
                .find(|r| r.scope == scope && r.prefix == *prefix)
            {
                return (
                    if record.group.is_some() {
                        Lookup::Hit
                    } else {
                        Lookup::Ambiguous
                    },
                    record.group,
                );
            }
        }
        (Lookup::Miss, None)
    }
    pub(super) fn publish(&self, scope: Digest, prefixes: &[Digest], group: Group, now: Instant) {
        let Some(mut records) = self.records.as_ref().and_then(|r| r.try_lock().ok()) else {
            return;
        };
        records.retain(|r| now < r.expires);
        let capacity = MAX_RECORDS.min(MAX_BYTES / std::mem::size_of::<Record>());
        for prefix in prefixes.iter().take(2) {
            if let Some(old) = records
                .iter_mut()
                .find(|r| r.scope == scope && r.prefix == *prefix)
            {
                if old.group != Some(group) {
                    old.group = None;
                }
                if old.group.is_some() {
                    old.expires = now + TTL;
                }
                continue;
            }
            if records.len() == capacity {
                records.pop_front();
            }
            records.push_back(Record {
                scope,
                prefix: *prefix,
                group: Some(group),
                expires: now + TTL,
            });
        }
    }
}
pub(super) struct Selection {
    scope: Digest,
    prefix: Digest,
    pub(super) hint: crate::protocol::cache::InferredAffinity,
}
fn target_prefixes(
    candidate: &super::BoundCandidate,
    request: &crate::adapter::Request,
) -> Option<Vec<Digest>> {
    let adapter = candidate.endpoint.adapter();
    let profile = adapter.openai().ok()?.protocol;
    let wire = adapter
        .encode_request(
            request,
            &candidate.endpoint.upstream_model,
            &candidate.endpoint.representation,
        )
        .ok()?;
    crate::protocol::cache_affinity::prefixes(&wire, profile)
}
pub(super) fn choose(
    index: &Index,
    candidate: &super::BoundCandidate,
    request: &crate::adapter::Request,
    trace: &mut super::diagnostics::Trace,
) -> Option<Selection> {
    use sha2::{Digest as _, Sha256};
    let context = request.conversation.as_ref()?;
    if context.is_conversation_scoped() || request.cache_session.is_some() {
        trace.affinity("explicit", false, false);
        return None;
    }
    if index.records.is_none() {
        trace.affinity("disabled", false, false);
        return None;
    }
    let endpoint = &candidate.endpoint;
    let adapter = endpoint.adapter();
    let adapter = adapter.openai().ok()?;
    let mut cache = endpoint.representation.cache;
    cache.intersect(adapter.adaptation.cache);
    if !cache.key && !adapter.adaptation.rules.opencode_go_headers
        || cache.key
            && !adapter.adaptation.rules.opencode_go_headers
            && !request.context.cache.prompt_cache_key.is_absent()
    {
        trace.affinity("explicit_or_unmapped", false, false);
        return None;
    }
    let scope = endpoint.representation.adaptation.scope.as_ref()?;
    let mut hash = Sha256::new();
    hash.update(b"MorphieCore.cache-scope.v1\0");
    for value in [
        scope.as_str(),
        endpoint.id.as_str(),
        adapter.adaptation.profile_id,
    ] {
        hash.update((value.len() as u64).to_be_bytes());
        hash.update(value.as_bytes());
    }
    let scope: Digest = hash.finalize().into();
    let Some(prefixes) = target_prefixes(candidate, request) else {
        trace.affinity("unsupported", false, false);
        return None;
    };
    let (lookup, existing) = index.lookup(scope, &prefixes, Instant::now());
    let mut fresh = Sha256::new();
    fresh.update(b"MorphieCore.cache-group.v1\0");
    fresh.update(scope);
    fresh.update(context.id().as_bytes());
    let group: Group =
        existing.unwrap_or_else(|| fresh.finalize()[..16].try_into().expect("digest"));
    trace.affinity(
        match lookup {
            Lookup::Miss => "miss",
            Lookup::Hit => "hit",
            Lookup::Ambiguous => "ambiguous",
            Lookup::Unavailable => "unavailable",
        },
        false,
        false,
    );
    Some(Selection {
        scope,
        prefix: *prefixes.last()?,
        hint: crate::protocol::cache::InferredAffinity(group),
    })
}
impl Selection {
    pub(super) fn project(
        &self,
        candidate: &super::BoundCandidate,
        source: &crate::adapter::Request,
    ) -> Option<crate::adapter::Request> {
        let mut local = source.clone();
        local.inferred_affinity = Some(self.hint);
        crate::execution::plan::representable(&candidate.endpoint, &local).ok()?;
        Some(local)
    }
    pub(super) fn publish(
        &self,
        index: &Index,
        candidate: &super::BoundCandidate,
        successor: Option<&crate::adapter::Request>,
    ) {
        let mut prefixes = vec![self.prefix];
        if let Some(prefix) = successor
            .and_then(|r| target_prefixes(candidate, r))
            .and_then(|p| p.last().copied())
        {
            prefixes.push(prefix);
        }
        index.publish(self.scope, &prefixes, self.hint.0, Instant::now());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{protocol::openai::Profile, semantic::context::ConversationContext};

    fn request(entry: &super::super::BoundEntry, seed: &str) -> crate::adapter::Request {
        let mut request = entry.client.decode_request(
            br#"{"model":"fixture-model","messages":[{"role":"system","content":"rules"},{"role":"user","content":"hello"}]}"#
        ).unwrap();
        request.conversation = Some(ConversationContext::independent_request(seed).unwrap());
        request
    }
    fn trace() -> super::super::diagnostics::Trace {
        super::super::diagnostics::Trace::new(None, &axum::http::HeaderMap::new())
    }

    #[test]
    fn candidate_hints_never_mutate_identity_or_cross_the_fixed_scope() {
        let gate = super::super::tests::gateway(super::super::Limits::default());
        let entry =
            &gate.state.entries[&(super::super::family(Profile::Chat), "fixture-model".into())];
        let candidate = &entry.candidates[0];
        let first = request(entry, "one");
        let saved = first.clone();
        let selection = choose(&gate.state.affinity, candidate, &first, &mut trace()).unwrap();
        assert!(selection.project(candidate, &first).is_some());
        selection.publish(&gate.state.affinity, candidate, None);
        let second = request(entry, "two");
        assert_eq!(
            choose(&gate.state.affinity, candidate, &second, &mut trace())
                .unwrap()
                .hint,
            selection.hint
        );
        assert_eq!(first, saved);
        for edit in ["instructions", "tools", "history"] {
            let mut edited = second.clone();
            let mut wire = serde_json::json!({"model":"fixture-model","messages":[
                {"role":"system","content":"rules"},{"role":"user","content":"hello"}]});
            if edit == "tools" {
                wire["tools"] = serde_json::json!([{"type":"function","function":{"name":"lookup","parameters":{"type":"object"},"strict":false}}]);
            } else {
                wire["messages"][if edit == "instructions" { 0 } else { 1 }]["content"] =
                    serde_json::json!("changed");
            }
            edited.task = entry
                .client
                .decode_request(&serde_json::to_vec(&wire).unwrap())
                .unwrap()
                .task;
            assert_ne!(
                choose(&gate.state.affinity, candidate, &edited, &mut trace())
                    .unwrap()
                    .hint,
                selection.hint
            );
        }
        let mut explicit = second.clone();
        explicit.conversation = Some(ConversationContext::conversation("explicit").unwrap());
        assert!(choose(&gate.state.affinity, candidate, &explicit, &mut trace()).is_none());
        let mut other = super::super::BoundCandidate {
            endpoint: candidate.endpoint.clone(),
            provider: candidate.provider.clone(),
            secret: candidate.secret.clone(),
            credential_fallback: candidate.credential_fallback,
        };
        let original = other
            .endpoint
            .adapter()
            .encode_request(
                &first,
                &other.endpoint.upstream_model,
                &other.endpoint.representation,
            )
            .unwrap();
        other.endpoint.execution.request_body_limit =
            crate::semantic::value::json_size(&original, 256 << 10).unwrap();
        assert!(crate::execution::plan::representable(&other.endpoint, &first).is_ok());
        assert!(selection.project(&other, &first).is_none());
        other.endpoint = candidate.endpoint.clone();
        other.endpoint.representation.adaptation.scope = Some(
            crate::semantic::value::ReplayOrigin::new("other-principal-or-key-epoch").unwrap(),
        );
        assert_ne!(
            choose(&gate.state.affinity, &other, &second, &mut trace())
                .unwrap()
                .hint,
            selection.hint
        );
        other.endpoint = candidate.endpoint.clone();
        other.endpoint.upstream_model = "other-model".into();
        assert_ne!(
            choose(&gate.state.affinity, &other, &second, &mut trace())
                .unwrap()
                .hint,
            selection.hint
        );
        other.endpoint = candidate.endpoint.clone();
        other.endpoint.representation.adaptation.profile_id = "projection-v2";
        assert_ne!(
            choose(&gate.state.affinity, &other, &second, &mut trace())
                .unwrap()
                .hint,
            selection.hint
        );
    }

    #[test]
    fn scope_expiry_disabled_and_conflicting_branches_are_independent() {
        let index = Index::new(true);
        let now = Instant::now();
        index.publish([1; 32], &[[2; 32]], [3; 16], now);
        assert_eq!(
            index.lookup([1; 32], &[[2; 32], [4; 32]], now),
            (Lookup::Hit, Some([3; 16]))
        );
        assert_eq!(index.lookup([9; 32], &[[2; 32]], now).1, None);
        assert_eq!(index.lookup([1; 32], &[[2; 32]], now + TTL).1, None);
        index.publish([1; 32], &[[2; 32]], [3; 16], now + TTL);
        index.publish([1; 32], &[[2; 32]], [4; 16], now + TTL);
        assert_eq!(
            index.lookup([1; 32], &[[2; 32]], now + TTL),
            (Lookup::Ambiguous, None)
        );
        let disabled = Index::new(false);
        disabled.publish([1; 32], &[[2; 32]], [3; 16], now);
        assert_eq!(
            disabled.lookup([1; 32], &[[2; 32]], now),
            (Lookup::Unavailable, None)
        );
    }

    #[test]
    fn capacity_contention_and_poison_do_not_wait_or_revive_records() {
        let index = Index::new(true);
        let now = Instant::now();
        for n in 0..=MAX_RECORDS {
            let mut digest = [0; 32];
            digest[..8].copy_from_slice(&(n as u64).to_be_bytes());
            index.publish([1; 32], &[digest], [2; 16], now);
        }
        assert!(index.lookup([1; 32], &[[0; 32]], now).1.is_none());
        let records = index.records.as_ref().unwrap();
        let lock = records.lock().unwrap();
        assert!(lock.len() <= MAX_RECORDS);
        assert!(lock.capacity() * std::mem::size_of::<Record>() <= MAX_BYTES);
        assert_eq!(
            index.lookup([1; 32], &[[0; 32]], now).0,
            Lookup::Unavailable
        );
        index.publish([1; 32], &[[3; 32]], [2; 16], now);
        drop(lock);
        let _ = std::panic::catch_unwind(|| {
            let _lock = records.lock().unwrap();
            panic!("synthetic poisoned index");
        });
        assert_eq!(
            index.lookup([1; 32], &[[0; 32]], now).0,
            Lookup::Unavailable
        );
    }
}
