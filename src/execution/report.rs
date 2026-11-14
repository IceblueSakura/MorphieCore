//! Bounded current-attempt evidence for public adaptation notices.
use super::AttemptError;
use crate::protocol::{
    conversion::generation::{
        ConversionBoundary, Coverage, Effect, EvidenceDelta, MAX_EFFECTS, MAX_EVIDENCE_BYTES,
    },
    diagnostics::{AdaptationNotice, FailureCategory, NoticeFailure},
    failure::SourceFailureEvidence,
};
use bytes::Bytes;
use serde::Serialize;
use std::sync::{Arc, Mutex};
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum View {
    RequestIntake,
    SourceIntake,
    UpstreamRequest,
    DownstreamJson,
    DownstreamSse,
    RetainedJson,
    ErrorDelivery,
}
#[derive(Clone, Debug)]
pub struct Failure {
    pub boundary: Option<ConversionBoundary>,
    pub cause: AttemptError,
    pub(crate) decode: Option<super::attempt::DecodeFailure>,
}
#[derive(Clone, Debug, Serialize)]
pub struct Entry {
    #[serde(flatten)]
    pub effect: Effect,
    pub view: View,
    pub checked: u64,
}
#[derive(Clone, Debug, Default, Serialize)]
pub struct Evidence {
    pub entries: Vec<Entry>,
    pub omitted_entries: u64,
    pub coverage: Coverage,
    #[serde(skip)]
    bytes: usize,
}
impl Evidence {
    fn accept(&mut self, view: View, delta: EvidenceDelta) {
        self.omitted_entries = self.omitted_entries.saturating_add(delta.omitted_entries);
        if delta.coverage != Coverage::Assessed {
            self.coverage = delta.coverage;
        }
        for effect in delta.into_effects() {
            let i = if let Some(i) = self
                .entries
                .iter()
                .position(|e| e.view == view && e.effect == effect)
            {
                i
            } else {
                let bytes = effect.retained_bytes() + std::mem::size_of::<Entry>();
                if self.entries.len() >= MAX_EFFECTS
                    || self.bytes.saturating_add(bytes) > MAX_EVIDENCE_BYTES
                {
                    self.omitted_entries = self.omitted_entries.saturating_add(1);
                    continue;
                }
                self.bytes += bytes;
                self.entries.push(Entry {
                    effect,
                    view,
                    checked: 0,
                });
                self.entries.len() - 1
            };
            let e = &mut self.entries[i];
            e.checked = e.checked.saturating_add(1);
        }
    }
}
#[derive(Clone, Debug, Default)]
pub struct AttemptFacts {
    pub source_failure: Option<SourceFailureEvidence>,
    pub failure: Option<Failure>,
    pub handling_failure: Option<Failure>,
    pub evidence: Evidence,
}
#[derive(Clone, Debug, Default)]
struct State {
    intake: Evidence,
    attempt: Option<AttemptFacts>,
    failure: Option<Failure>,
    request_error: Option<(&'static str, u16)>,
}
#[derive(Clone, Default, Debug)]
pub struct Report(Arc<Mutex<State>>);
impl Report {
    fn with<T>(&self, f: impl FnOnce(&mut State) -> T) -> T {
        f(&mut self.0.lock().expect("adaptation notice"))
    }
    pub fn request_failure(&self, code: &'static str, status: u16) {
        self.with(|r| r.request_error = Some((code, status)));
    }
    pub(crate) fn request_error(&self) -> Option<(&'static str, u16)> {
        self.with(|r| r.request_error)
    }
    pub(crate) fn cause(&self) -> Option<Failure> {
        self.with(|r| {
            r.attempt
                .as_ref()
                .and_then(|a| a.failure.clone())
                .or_else(|| r.failure.clone())
        })
    }
    pub fn intake(&self, evidence: EvidenceDelta) {
        self.with(|r| r.intake.accept(View::RequestIntake, evidence));
    }
    pub fn begin(&self) {
        self.with(|r| r.attempt = Some(AttemptFacts::default()));
    }
    pub fn attempt(&self) -> AttemptFacts {
        self.with(|r| r.attempt.clone().unwrap_or_default())
    }
    pub fn failure(&self, boundary: Option<ConversionBoundary>, error: &AttemptError) {
        self.failure_with_context(boundary, error, None);
    }
    pub(crate) fn failure_with_context(
        &self,
        boundary: Option<ConversionBoundary>,
        error: &AttemptError,
        decode: Option<super::attempt::DecodeFailure>,
    ) {
        self.with(|r| {
            let failure = Failure {
                boundary: error.boundary().or(boundary),
                cause: error.clone(),
                decode,
            };
            if let Some(a) = &mut r.attempt {
                let source_already_observed = a.source_failure.is_some();
                if let Some(source) = error.source_failure() {
                    a.source_failure.get_or_insert(source);
                }
                if a.failure.is_none() && !source_already_observed {
                    a.failure = Some(failure);
                } else if a.handling_failure.is_none() {
                    a.handling_failure = Some(failure);
                }
            } else if r.failure.is_none() {
                r.failure = Some(failure);
            }
        });
    }
    pub fn source_failure(&self, source: SourceFailureEvidence) {
        self.with(|r| {
            if let Some(a) = &mut r.attempt {
                a.source_failure = Some(source);
            }
        });
    }
    pub fn accept(&self, view: View, delta: EvidenceDelta) {
        self.with(|r| {
            if let Some(a) = &mut r.attempt {
                a.evidence.accept(view, delta);
            }
        });
    }
    pub fn batch(&self, view: View, frames: Vec<Bytes>, delta: EvidenceDelta) -> EncodedBatch {
        self.accept(view, delta);
        EncodedBatch { frames }
    }
    pub(crate) fn preview_notice(
        &self,
        view: View,
        delta: &EvidenceDelta,
    ) -> Option<AdaptationNotice> {
        let preview = Self(Arc::new(Mutex::new(self.with(|r| r.clone()))));
        preview.accept(view, delta.clone());
        preview.notice(view)
    }
    /// Selected producing-attempt facts only. Rejected candidates never enter a response.
    pub(crate) fn notice(&self, view: View) -> Option<AdaptationNotice> {
        self.with(|r| {
            let a = r.attempt.as_ref()?;
            let mut notice = AdaptationNotice::default();
            notice.coverage = a.evidence.coverage;
            notice.omitted_entries = a
                .evidence
                .omitted_entries
                .saturating_add(r.intake.omitted_entries);
            for selected in [view, View::SourceIntake, View::UpstreamRequest] {
                for e in a.evidence.entries.iter().filter(|e| e.view == selected) {
                    notice.push(&e.effect, e.checked);
                }
            }
            for e in &r.intake.entries {
                notice.push(&e.effect, e.checked);
            }
            if view == View::ErrorDelivery
                && let Some(failure) = &a.failure
            {
                let category = if a.source_failure.is_some() {
                    FailureCategory::Source
                } else {
                    match failure.cause {
                        AttemptError::Transport(_) | AttemptError::Status { .. } => {
                            FailureCategory::Transport
                        }
                        AttemptError::Delivery(_) | AttemptError::Lifecycle(_) => {
                            FailureCategory::Delivery
                        }
                        _ => FailureCategory::Conversion,
                    }
                };
                notice.failure = Some(NoticeFailure {
                    boundary: failure.boundary,
                    category,
                    source_kind: a.source_failure.map(|e| e.kind),
                });
            }
            notice.enforce_bounds();
            (!notice.is_empty()).then_some(notice)
        })
    }
}
/// An immutable checked output unit. Dropping it does not acknowledge delivery.
pub struct EncodedBatch {
    frames: Vec<Bytes>,
}
impl EncodedBatch {
    pub fn empty() -> Self {
        Self { frames: vec![] }
    }
    pub fn frames(&self) -> &[Bytes] {
        &self.frames
    }
    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }
    pub fn byte_len(&self) -> usize {
        self.frames.iter().map(Bytes::len).sum()
    }
    pub fn into_bytes(self) -> Vec<Bytes> {
        self.frames
    }
}
impl std::ops::Deref for EncodedBatch {
    type Target = [Bytes];
    fn deref(&self) -> &[Bytes] {
        &self.frames
    }
}
impl IntoIterator for EncodedBatch {
    type Item = Bytes;
    type IntoIter = std::vec::IntoIter<Bytes>;
    fn into_iter(self) -> Self::IntoIter {
        self.frames.into_iter()
    }
}

impl std::fmt::Debug for EncodedBatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EncodedBatch")
            .field("frames", &self.frames.len())
            .field("bytes", &self.byte_len())
            .finish()
    }
}

#[cfg(test)]
mod tests;
