//! Optional body-free wire projection, independent of execution and Provider dialects.
use super::failure::SourceFailureKind;
use crate::protocol::conversion::generation::{
    Change, ConversionBoundary, Coverage, Effect, Field, Owner,
};
use serde::Serialize;
use serde_json::Value;

const MAX_NOTICE_ENTRIES: usize = 16;
const MAX_NOTICE_BYTES: usize = 2048;
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureCategory {
    Conversion,
    Source,
    Transport,
    Delivery,
}
#[derive(Clone, Debug, Serialize)]
pub struct NoticeFailure {
    pub boundary: Option<ConversionBoundary>,
    pub category: FailureCategory,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_kind: Option<SourceFailureKind>,
}
#[derive(Clone, Debug, Serialize)]
struct NoticeEntry {
    field: Field,
    owner: Owner,
    direction: ConversionBoundary,
    change: Change,
    occurrences: u64,
}
#[derive(Clone, Debug, Default, Serialize)]
pub struct AdaptationNotice {
    adaptations: Vec<NoticeEntry>,
    pub(crate) coverage: Coverage,
    pub(crate) omitted_entries: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) failure: Option<NoticeFailure>,
}
impl AdaptationNotice {
    pub(crate) fn push(&mut self, effect: &Effect, occurrences: u64) {
        if !effect.change().is_loss() {
            return;
        }
        if let Some(entry) = self.adaptations.iter_mut().find(|e| {
            e.field == effect.semantic_field()
                && e.owner == effect.owner()
                && e.direction == effect.boundary()
                && e.change == effect.change()
        }) {
            entry.occurrences = entry.occurrences.saturating_add(occurrences);
            return;
        }
        if self.adaptations.len() >= MAX_NOTICE_ENTRIES {
            self.omitted_entries = self.omitted_entries.saturating_add(1);
            return;
        }
        self.adaptations.push(NoticeEntry {
            field: effect.semantic_field(),
            owner: effect.owner(),
            direction: effect.boundary(),
            change: effect.change(),
            occurrences,
        });
        if serde_json::to_vec(self).map_or(true, |b| b.len() > MAX_NOTICE_BYTES) {
            self.adaptations.pop();
            self.omitted_entries = self.omitted_entries.saturating_add(1);
        }
    }
    pub(crate) fn add_effects<'a>(&mut self, effects: impl IntoIterator<Item = &'a Effect>) {
        for effect in effects {
            self.push(effect, 1);
        }
    }
    pub(crate) fn enforce_bounds(&mut self) {
        while !self.adaptations.is_empty()
            && serde_json::to_vec(self).map_or(true, |b| b.len() > MAX_NOTICE_BYTES)
        {
            self.adaptations.pop();
            self.omitted_entries = self.omitted_entries.saturating_add(1);
        }
    }
    pub(crate) fn is_empty(&self) -> bool {
        self.adaptations.is_empty() && self.omitted_entries == 0 && self.failure.is_none()
    }
    /// This checks the complete payload, not just the added field. No new packet is emitted.
    pub(crate) fn attach(
        &self,
        payload: &mut Value,
        max_payload_bytes: usize,
        nested_response: bool,
    ) -> bool {
        if self.is_empty() {
            return false;
        }
        let Some(target) = (if nested_response {
            payload.get_mut("response")
        } else {
            Some(&mut *payload)
        })
        .and_then(Value::as_object_mut) else {
            return false;
        };
        if target.contains_key("morphiecore") {
            return false;
        }
        let Ok(notice) = serde_json::to_value(self) else {
            return false;
        };
        target.insert("morphiecore".into(), notice);
        if crate::semantic::value::json_size(payload, max_payload_bytes).is_err() {
            let target = if nested_response {
                payload.get_mut("response").expect("response")
            } else {
                payload
            };
            target
                .as_object_mut()
                .expect("notice target")
                .remove("morphiecore");
            false
        } else {
            true
        }
    }
}
