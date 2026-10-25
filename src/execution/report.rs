//! Bounded execution-owned evidence. Observations never authorize routing or delivery.
use super::AttemptError;
use crate::lowering::generation::{
    ConversionBoundary, Coverage, Effect, EvidenceDelta, MAX_EFFECTS, MAX_EVIDENCE_BYTES,
};
use crate::protocol::diagnostics::{AdaptationNotice, FailureCategory, NoticeFailure};
use crate::protocol::failure::SourceFailureEvidence;
use bytes::Bytes;
use serde::{Serialize, Serializer, ser::SerializeStruct};
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
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ResultState {
    #[default]
    Pending,
    Succeeded,
    Failed,
    Interrupted,
}
#[derive(Clone, Debug)]
pub struct Failure {
    pub boundary: Option<ConversionBoundary>,
    pub cause: AttemptError,
}
impl Serialize for Failure {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut s = s.serialize_struct("Failure", 3)?;
        s.serialize_field("boundary", &self.boundary)?;
        s.serialize_field("reason", &safe_reason(&self.cause))?;
        s.serialize_field("locus", &self.cause.locus())?;
        s.end()
    }
}
fn safe_reason(error: &AttemptError) -> String {
    use crate::{
        adapter::ConversionFailure,
        protocol::{CodecError, stream::SseError},
    };
    fn codec(error: &CodecError) -> String {
        match error {
            CodecError::Representation { cause, .. } => format!("representation.{cause:?}"),
            CodecError::Invalid(reason) => format!("invalid.{reason:?}"),
            CodecError::Unsupported(reason) => format!("unsupported.{reason:?}"),
            CodecError::Semantic(e) => format!("semantic.{e:?}"),
            CodecError::Event(e) => format!("event.{e:?}"),
            CodecError::Context(_) => "context".into(),
            CodecError::StreamFailure(e) => format!("source.{:?}", e.kind),
            CodecError::ProfileMismatch => "profile_mismatch".into(),
            CodecError::Limit => "value_limit".into(),
            CodecError::Resource(r) => format!("resource.{r:?}"),
        }
    }
    match error {
        AttemptError::Codec(e)
        | AttemptError::Adapter(ConversionFailure {
            cause: crate::adapter::ConversionCause::Codec(e),
            ..
        })
        | AttemptError::Sse(SseError::Codec(e)) => codec(e),
        AttemptError::Representation(e)
        | AttemptError::Adapter(ConversionFailure {
            cause: crate::adapter::ConversionCause::Representation(e),
            ..
        }) => format!("representation.{e:?}"),
        AttemptError::Sse(_) => "framing".into(),
        AttemptError::Protocol(r) => format!("protocol.{r:?}"),
        AttemptError::Delivery(r) => format!("delivery.{r:?}"),
        AttemptError::Transport(r) => format!("transport.{r:?}"),
        AttemptError::Status { class, .. } => format!("status.{class:?}"),
        AttemptError::Lifecycle(r) => format!("lifecycle.{r:?}"),
        AttemptError::Limit => "execution_limit".into(),
        AttemptError::Resource(r) => format!("resource.{r:?}"),
        AttemptError::Deadline(r) => format!("deadline.{r:?}"),
    }
}
#[derive(Clone, Debug, Serialize)]
pub struct Entry {
    #[serde(flatten)]
    pub effect: Effect,
    pub view: View,
    pub checked: u64,
    pub no_carrier: u64,
    pub handed_off: u64,
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
    fn accept(&mut self, view: View, delta: EvidenceDelta, no_carrier: bool) -> Vec<usize> {
        self.omitted_entries = self.omitted_entries.saturating_add(delta.omitted_entries);
        if delta.coverage != Coverage::Assessed {
            self.coverage = delta.coverage;
        }
        let mut indices = Vec::new();
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
                    no_carrier: 0,
                    handed_off: 0,
                });
                self.entries.len() - 1
            };
            let e = &mut self.entries[i];
            e.checked = e.checked.saturating_add(1);
            if no_carrier {
                e.no_carrier = e.no_carrier.saturating_add(1);
            }
            indices.push(i);
        }
        indices
    }
}
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskOutcome {
    Completed,
    Incomplete,
    Failed,
    Cancelled,
}
#[derive(Clone, Debug, Default, Serialize)]
pub struct AttemptReport {
    pub ordinal: usize,
    pub reported_settings: Option<crate::lowering::generation::ReasoningControls>,
    pub prepared: bool,
    pub send_started: bool,
    pub observed_status: Option<u16>,
    pub source_failure: Option<SourceFailureEvidence>,
    pub failure: Option<Failure>,
    pub handling_failure: Option<Failure>,
    pub result: ResultState,
    pub published: bool,
    pub encoding_finalized: bool,
    pub task_outcome: Option<TaskOutcome>,
    pub public_code: Option<&'static str>,
    pub public_status: Option<u16>,
    pub policy_class: Option<crate::provider::ErrorClass>,
    pub health_failure: bool,
    pub advanced: bool,
    pub source_eof: bool,
    pub delivery_complete: bool,
    pub checked_units: u64,
    pub handed_off_frames: u64,
    pub partial_units: u64,
    pub abandoned_units: u64,
    pub active_units: Vec<BatchProgress>,
    pub last_abandoned_unit: Option<BatchProgress>,
    pub handed_off_bytes: u64,
    pub association_omissions: u64,
    pub evidence: Evidence,
}
#[derive(Clone, Debug, Default, Serialize)]
pub struct RequestReport {
    pub public_code: Option<&'static str>,
    pub public_status: Option<u16>,
    #[serde(serialize_with = "serialize_planning_failure")]
    pub planning_failure: Option<super::plan::PlanError>,
    pub intake: Evidence,
    pub failure: Option<Failure>,
    pub attempts: Vec<AttemptReport>,
    pub producing_attempt: Option<usize>,
    pub result: ResultState,
}
#[derive(Clone, Default, Debug)]
pub struct Report(Arc<Mutex<RequestReport>>);
fn serialize_planning_failure<S: Serializer>(
    failure: &Option<super::plan::PlanError>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    use super::plan::PlanError;
    let Some(error) = failure else {
        return serializer.serialize_none();
    };
    // Retain the typed cause internally, but never expose private endpoint IDs.
    let (kind, reasons) = match error {
        PlanError::UnknownModel => ("unknown_model", vec![]),
        PlanError::TaskMismatch => ("task_mismatch", vec![]),
        PlanError::InvalidRequest => ("invalid_request", vec![]),
        PlanError::CandidateLimit => ("candidate_limit", vec![]),
        PlanError::CandidateRejected(reason) => ("candidate_rejected", vec![format!("{reason:?}")]),
        PlanError::NoCandidate { rejections } => (
            "no_candidate",
            rejections
                .iter()
                .take(super::plan::MAX_CANDIDATES)
                .map(|r| format!("{:?}", r.reason))
                .collect(),
        ),
    };
    let mut state = serializer.serialize_struct("PlanningFailure", 2)?;
    state.serialize_field("kind", kind)?;
    state.serialize_field("reasons", &reasons)?;
    state.end()
}
impl Report {
    fn with<T>(&self, f: impl FnOnce(&mut RequestReport) -> T) -> T {
        f(&mut self.0.lock().unwrap_or_else(|p| p.into_inner()))
    }
    pub fn public_failure(&self, code: &'static str, status: u16) {
        self.with(|r| {
            r.public_code = Some(code);
            r.public_status = Some(status);
            if r.result == ResultState::Pending {
                r.result = ResultState::Failed;
            }
        });
    }
    pub fn snapshot(&self) -> RequestReport {
        self.with(|r| r.clone())
    }
    pub(crate) fn planning_failure(&self, failure: super::plan::PlanError) {
        self.with(|r| r.planning_failure = Some(failure));
    }
    pub fn intake(&self, evidence: EvidenceDelta) {
        self.with(|r| {
            r.intake.accept(View::RequestIntake, evidence, false);
        });
    }
    pub fn begin(&self, ordinal: usize) {
        self.with(|r| {
            if r.attempts.len() < crate::topology::route::MAX_ROUTE_CANDIDATES {
                r.attempts.push(AttemptReport {
                    ordinal,
                    ..Default::default()
                });
            }
        });
    }
    pub fn attempt(&self) -> AttemptReport {
        self.with(|r| r.attempts.last().cloned().unwrap_or_default())
    }
    pub fn update(&self, f: impl FnOnce(&mut AttemptReport)) {
        self.with(|r| {
            if let Some(a) = r.attempts.last_mut() {
                f(a)
            }
        });
    }
    pub fn failure(&self, boundary: Option<ConversionBoundary>, error: &AttemptError) {
        self.with(|r| {
            let failure = Failure {
                boundary: error.boundary().or(boundary),
                cause: error.clone(),
            };
            if let Some(a) = r.attempts.last_mut() {
                if let Some(source) = error.source_failure() {
                    a.source_failure.get_or_insert(source);
                }
                if a.failure.is_none() {
                    a.failure = Some(failure)
                } else if a.handling_failure.is_none() {
                    a.handling_failure = Some(failure)
                }
            } else if r.failure.is_none() {
                r.failure = Some(failure)
            }
        });
    }
    pub fn source_failure(&self, e: SourceFailureEvidence) {
        self.update(|a| {
            a.source_failure = Some(e);
        });
    }
    pub fn end(&self, success: bool) {
        self.with(|r| {
            if let Some(a) = r.attempts.last_mut()
                && a.result == ResultState::Pending
            {
                a.result = if success {
                    ResultState::Succeeded
                } else {
                    ResultState::Failed
                };
                if let Some(e) = &mut a.source_failure
                    && e.observation == crate::protocol::failure::Observation::Pending
                {
                    e.observation = crate::protocol::failure::Observation::Interrupted;
                }
                if success {
                    r.producing_attempt = Some(a.ordinal);
                }
            }
        });
    }
    pub fn result(&self, result: ResultState) {
        self.with(|r| r.result = result);
    }
    pub fn interrupted(&self) {
        self.with(|r| {
            if r.result == ResultState::Pending {
                r.result = ResultState::Interrupted;
            }
            if let Some(a) = r.attempts.last_mut()
                && a.result == ResultState::Pending
            {
                a.result = ResultState::Interrupted;
                if let Some(e) = &mut a.source_failure
                    && e.observation == crate::protocol::failure::Observation::Pending
                {
                    e.observation = crate::protocol::failure::Observation::Interrupted;
                }
            }
        });
    }
    pub fn accept(&self, view: View, delta: EvidenceDelta) {
        self.update(|a| {
            a.evidence.accept(view, delta, false);
        });
    }
    pub fn batch(&self, view: View, frames: Vec<Bytes>, delta: EvidenceDelta) -> EncodedBatch {
        let (observer, association) = self.with(|r| {
            let Some(attempt) = r.attempts.len().checked_sub(1) else {
                return (None, None);
            };
            let a = &mut r.attempts[attempt];
            a.checked_units = a.checked_units.saturating_add(1);
            let indices = a.evidence.accept(view, delta, frames.is_empty());
            let observer = (!frames.is_empty()).then(|| (self.clone(), attempt));
            if frames.is_empty() {
                return (observer, None);
            }
            if a.active_units.len() >= 32 {
                a.association_omissions = a.association_omissions.saturating_add(1);
                return (observer, None);
            }
            let unit = a.checked_units;
            a.active_units.push(BatchProgress {
                unit,
                frames: frames.len(),
                acknowledged: 0,
                indices,
            });
            (
                observer,
                Some(Arc::new(Association {
                    report: self.clone(),
                    attempt,
                    unit,
                })),
            )
        });
        EncodedBatch {
            frames,
            observer,
            association,
        }
    }
    pub(crate) fn preview_notice(
        &self,
        view: View,
        delta: &EvidenceDelta,
    ) -> Option<AdaptationNotice> {
        let preview = Self(Arc::new(Mutex::new(self.snapshot())));
        preview.accept(view, delta.clone());
        preview.notice(view)
    }
    /// Selected producing-attempt facts only. Rejected candidates never enter a response.
    pub(crate) fn notice(&self, view: View) -> Option<AdaptationNotice> {
        self.with(|r| {
            let a = r.attempts.last()?;
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
#[derive(Clone, Debug, Serialize)]
pub struct BatchProgress {
    pub unit: u64,
    pub frames: usize,
    pub acknowledged: usize,
    #[serde(skip)]
    indices: Vec<usize>,
}
struct Association {
    report: Report,
    attempt: usize,
    unit: u64,
}
impl Drop for Association {
    fn drop(&mut self) {
        self.report.with(|r| {
            if let Some(a) = r.attempts.get_mut(self.attempt)
                && let Some(i) = a.active_units.iter().position(|p| p.unit == self.unit)
            {
                let progress = a.active_units.remove(i);
                a.abandoned_units = a.abandoned_units.saturating_add(1);
                if progress.acknowledged > 0 {
                    a.partial_units = a.partial_units.saturating_add(1);
                }
                a.last_abandoned_unit = Some(progress);
            }
        });
    }
}
/// Report updates and acknowledgment publication share one short critical section.
/// Thus producer cancellation cannot race a successful acknowledgment out of its log.
pub(crate) struct FrameAck {
    report: Report,
    attempt: usize,
    bytes: usize,
    association: Option<Arc<Association>>,
}
impl FrameAck {
    pub(crate) fn acknowledge(self, send: impl FnOnce() -> bool) -> bool {
        self.report.with(|r| {
            if !send() {
                return false;
            }
            if let Some(a) = r.attempts.get_mut(self.attempt) {
                a.handed_off_frames = a.handed_off_frames.saturating_add(1);
                a.handed_off_bytes = a.handed_off_bytes.saturating_add(self.bytes as u64);
                if let Some(association) = &self.association
                    && let Some(i) = a
                        .active_units
                        .iter()
                        .position(|p| p.unit == association.unit)
                {
                    a.active_units[i].acknowledged += 1;
                    if a.active_units[i].acknowledged == a.active_units[i].frames {
                        let complete = a.active_units.remove(i);
                        for index in complete.indices {
                            if let Some(e) = a.evidence.entries.get_mut(index) {
                                e.handed_off = e.handed_off.saturating_add(1);
                            }
                        }
                    }
                }
            }
            true
        })
    }
}
/// An immutable checked output unit. Dropping it does not acknowledge delivery.
pub struct EncodedBatch {
    frames: Vec<Bytes>,
    observer: Option<(Report, usize)>,
    association: Option<Arc<Association>>,
}
impl EncodedBatch {
    pub fn empty() -> Self {
        Self {
            frames: vec![],
            observer: None,
            association: None,
        }
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
    pub(crate) fn into_acknowledged_frames(
        self,
    ) -> impl Iterator<Item = (Bytes, Option<FrameAck>)> {
        let Self {
            frames,
            observer,
            association,
        } = self;
        frames.into_iter().map(move |b| {
            let ack = observer.as_ref().map(|(report, attempt)| FrameAck {
                report: report.clone(),
                attempt: *attempt,
                bytes: b.len(),
                association: association.clone(),
            });
            (b, ack)
        })
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
