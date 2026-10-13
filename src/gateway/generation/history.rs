//! Process-local response snapshots. Pending delivery is never readable.
use crate::{
    gateway::{ApiError, Limits},
    protocol::{DecodedRequest, DecodedResponse, openai::responses::managed::Options},
};
use axum::http::StatusCode;
use bytes::Bytes;
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};
use tokio::time::Instant;

pub(in crate::gateway) struct Snapshot {
    pub id: String,
    pub input: DecodedRequest,
    pub response: DecodedResponse,
    pub body: Bytes,
    pub weight: usize,
}
impl std::fmt::Debug for Snapshot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Snapshot([redacted])")
    }
}
struct Entry {
    snapshot: Arc<Snapshot>,
    expires: Option<Instant>,
}
pub(in crate::gateway) struct Store {
    entries: Mutex<BTreeMap<String, Entry>>,
    ttl: std::time::Duration,
    max_entries: usize,
    max_bytes: usize,
}
fn missing() -> ApiError {
    ApiError::new(StatusCode::NOT_FOUND, "response_not_found")
}
impl Store {
    pub fn new(limits: &Limits) -> Self {
        Self {
            entries: Mutex::new(BTreeMap::new()),
            ttl: limits.history_ttl,
            max_entries: limits.history_entries,
            max_bytes: limits.history_bytes,
        }
    }
    fn prune(entries: &mut BTreeMap<String, Entry>, now: Instant) {
        entries.retain(|_, entry| entry.expires.is_none_or(|expiry| now < expiry));
    }
    pub fn get(&self, id: &str, now: Instant) -> Result<Arc<Snapshot>, ApiError> {
        let mut entries = self.entries.lock().expect("history lock");
        Self::prune(&mut entries, now);
        entries
            .get(id)
            .filter(|entry| entry.expires.is_some())
            .map(|entry| entry.snapshot.clone())
            .ok_or_else(missing)
    }
    pub fn remove(&self, id: &str, now: Instant) -> Result<(), ApiError> {
        let mut entries = self.entries.lock().expect("history lock");
        Self::prune(&mut entries, now);
        if entries.get(id).is_none_or(|entry| entry.expires.is_none()) {
            return Err(missing());
        }
        entries.remove(id);
        Ok(())
    }
    pub fn clear(&self) {
        self.entries.lock().expect("history lock").clear();
    }
    pub fn stage(
        self: &Arc<Self>,
        snapshot: Snapshot,
        now: Instant,
    ) -> Result<Reservation, ApiError> {
        let mut entries = self.entries.lock().expect("history lock");
        Self::prune(&mut entries, now);
        let used: usize = entries.values().map(|entry| entry.snapshot.weight).sum();
        if entries.len() >= self.max_entries
            || snapshot.weight > self.max_bytes.saturating_sub(used)
        {
            return Err(ApiError::new(
                StatusCode::TOO_MANY_REQUESTS,
                "history_capacity_exceeded",
            ));
        }
        if entries.contains_key(&snapshot.id) {
            return Err(ApiError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "response_id_collision",
            ));
        }
        let id = snapshot.id.clone();
        entries.insert(
            id.clone(),
            Entry {
                snapshot: Arc::new(snapshot),
                expires: None,
            },
        );
        Ok(Reservation {
            store: self.clone(),
            id,
            pending: true,
        })
    }
}
/// Queued final bytes own this reservation. Dropping them cannot leave a readable response.
pub(in crate::gateway) struct Reservation {
    store: Arc<Store>,
    id: String,
    pending: bool,
}
impl Reservation {
    pub fn publish(mut self, now: Instant) {
        if let Some(entry) = self
            .store
            .entries
            .lock()
            .expect("history lock")
            .get_mut(&self.id)
        {
            entry.expires = Some(now + self.store.ttl);
        }
        self.pending = false;
    }
}
impl Drop for Reservation {
    fn drop(&mut self) {
        if self.pending {
            self.store
                .entries
                .lock()
                .expect("history lock")
                .remove(&self.id);
        }
    }
}
pub(in crate::gateway) struct Pending {
    pub id: String,
    pub options: Options,
    pub input: Option<DecodedRequest>,
    pub store: Arc<Store>,
}
impl Pending {
    pub fn stage(&self, response: DecodedResponse, body: Bytes) -> Result<Reservation, ApiError> {
        let input = self.input.as_ref().ok_or_else(ApiError::upstream)?;
        let weight = crate::protocol::openai::responses::managed::retained_size(
            input,
            &response,
            body.len(),
        )
        .map_err(|_| ApiError::upstream())?;
        self.store.stage(
            Snapshot {
                id: self.id.clone(),
                input: input.clone(),
                response,
                body,
                weight,
            },
            Instant::now(),
        )
    }
    pub fn new(
        options: Options,
        input: Option<DecodedRequest>,
        store: Arc<Store>,
    ) -> Result<Self, ApiError> {
        let mut seed = [0; 16];
        getrandom::fill(&mut seed)
            .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "entropy_unavailable"))?;
        Ok(Self {
            id: format!("resp_mc_{:032x}", u128::from_be_bytes(seed)),
            options,
            input,
            store,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::openai::{Adapter, Dialect, Profile};
    use std::time::Duration;

    fn snapshot(id: &str) -> Snapshot {
        let adapter = Adapter::new(Profile::Responses, Dialect::Standard, None);
        let input = adapter
            .decode_client_request(br#"{"model":"synthetic","input":"first"}"#)
            .unwrap()
            .task;
        let response = adapter.decode_response(br#"{"id":"synthetic","object":"response","model":"synthetic","created_at":1,"status":"completed","output":[]}"#).unwrap();
        Snapshot {
            id: id.into(),
            input,
            response,
            body: bytes::Bytes::from_static(b"{}"),
            weight: 10,
        }
    }
    #[test]
    fn snapshots_require_delivery_and_expire_without_refreshing_on_read() {
        let store = Arc::new(Store::new(&Limits {
            history_ttl: Duration::from_secs(2),
            history_entries: 2,
            history_bytes: 20,
            ..Default::default()
        }));
        let now = Instant::now();
        let pending = store.stage(snapshot("parent"), now).unwrap();
        assert_eq!(store.get("parent", now).unwrap_err().status, 404);
        pending.publish(now);
        assert_eq!(store.get("parent", now).unwrap().id, "parent");
        store.stage(snapshot("child"), now).unwrap().publish(now);
        assert_eq!(
            store.stage(snapshot("full"), now).err().unwrap().status,
            429
        );
        store.remove("parent", now).unwrap();
        assert!(store.get("child", now).is_ok());
        let mut oversized = snapshot("oversized");
        oversized.weight = 11;
        assert_eq!(store.stage(oversized, now).err().unwrap().status, 429);
        drop(store.stage(snapshot("cancelled"), now).unwrap());
        assert!(store.get("cancelled", now).is_err());
        assert!(store.get("child", now + Duration::from_secs(1)).is_ok());
        assert!(store.get("child", now + Duration::from_secs(2)).is_err());
        assert!(
            store
                .stage(snapshot("released"), now + Duration::from_secs(2))
                .is_ok()
        );
    }
}
