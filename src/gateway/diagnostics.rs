//! Opt-in operator metadata. Only trusted Agent identity and body-free observations.
//! Request paths only try-send to a bounded writer; failures cannot alter responses.
use serde::Serialize;
use std::time::Instant;
mod export;
mod telemetry;
mod writer;
pub(super) use writer::Sink;

#[derive(Clone, Copy, Default, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Stage {
    #[default]
    Admission,
    Prepare,
    Connect,
    ResponseHead,
    Intake,
    Terminal,
    Projection,
    Delivery,
    Complete,
}
#[derive(Clone, Copy, Default, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Outcome {
    #[default]
    Error,
    Interrupted,
    Timeout,
    Shutdown,
    Complete,
}
impl Outcome {
    fn label(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Interrupted => "interrupted",
            Self::Timeout => "timeout",
            Self::Shutdown => "shutdown",
            Self::Complete => "complete",
        }
    }
}
fn is_zero(n: &u64) -> bool {
    *n == 0
}
#[derive(Default, Serialize)]
struct EventCounts {
    event_items: u64,
    event_reasoning_items: u64,
    event_parts: u64,
    event_deltas: u64,
    event_item_closures: u64,
}

#[derive(Serialize)]
struct CandidateRecord {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    embedding_adaptations: Vec<crate::protocol::embedding::projection::AdaptationRecord>,
    #[serde(flatten)]
    routing: RoutingObservation,
    #[serde(skip_serializing_if = "Option::is_none")]
    reported_reasoning: Option<serde_json::Value>,
    #[serde(flatten)]
    affinity: AffinityObservation,
    ordinal: usize,
    stage: Stage,
    upstream_status: Option<u16>,
    retry_after_seconds: Option<u64>,
    received_bytes: u64,
    handed_off_bytes: u64,
    elapsed_ms: u64,
    error_code: Option<&'static str>,
    advanced: bool,
    /// The executor's catalog Provider ID; never a credential or key fingerprint.
    #[serde(skip_serializing_if = "Option::is_none")]
    provider: Option<String>,
}
#[derive(Default, Serialize)]
struct ImageAccountingObservation {
    #[serde(skip_serializing_if = "Option::is_none")]
    reported_input_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reported_output_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reported_image_cost_usd: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    image_usage_omitted: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    image_billing_omitted: Option<bool>,
}
#[derive(Default, Serialize)]
struct Record {
    #[serde(skip_serializing_if = "Option::is_none")]
    agent_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    generation: Option<crate::execution::report::RequestReport>,
    request_id: Option<String>,
    #[serde(flatten)]
    routing: RoutingObservation,
    #[serde(skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    selection_skips: std::collections::BTreeMap<&'static str, u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    capture_exchange: Option<u64>,
    #[serde(flatten)]
    affinity: AffinityObservation,
    #[serde(skip_serializing_if = "Option::is_none")]
    reported_cached_tokens: Option<u64>,
    stage: Stage,
    outcome: Outcome,
    upstream_status: Option<u16>,
    retry_after_seconds: Option<u64>,
    received_bytes: u64,
    handed_off_bytes: u64,
    elapsed_ms: u64,
    upstream_head_ms: Option<u64>,
    first_upstream_bytes_ms: Option<u64>,
    first_upstream_delta_ms: Option<u64>,
    candidates: Vec<CandidateRecord>,
    /// Maximum accounted delivery buffer bytes observed at an attempt's exit.
    #[serde(skip_serializing_if = "is_zero")]
    coalescing_peak_bytes: u64,
    /// Why candidates left compiled order: `affinity`, `cooling`, both, or `compiled`.
    #[serde(skip_serializing_if = "Option::is_none")]
    executor_order: Option<&'static str>,
    /// Presence only: input image/text, output text, accepted/rejected prediction,
    /// input/output audio, in that bit order. Never count values or response text.
    #[serde(skip_serializing_if = "Option::is_none")]
    reported_usage_detail_mask: Option<u8>,
    /// Untrusted field names are not safe diagnostic labels.
    #[serde(skip_serializing_if = "is_zero")]
    ignored_request_field_count: u64,
    /// Count of skipped unregistered upstream event types; names never escape.
    #[serde(skip_serializing_if = "is_zero")]
    ignored_upstream_events: u64,
    #[serde(flatten)]
    events: EventCounts,
    #[serde(flatten)]
    image: ImageAccountingObservation,
}
#[derive(Clone, Copy, Default, Serialize)]
struct RoutingObservation {
    #[serde(skip_serializing_if = "Option::is_none")]
    stop_reason: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    health: Option<crate::gateway::routing::executors::Observation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    recovery_probe: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    completion_state: Option<&'static str>,
}
#[derive(Clone, Copy, Default, Serialize)]
struct AffinityObservation {
    #[serde(skip_serializing_if = "Option::is_none")]
    affinity_source: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    affinity_key_sent: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    affinity_session_sent: Option<bool>,
}
pub(super) struct Trace {
    pub(super) report: crate::execution::report::Report,
    embedding_adaptations: Vec<crate::protocol::embedding::projection::AdaptationRecord>,
    generation: bool,
    pub(super) capture: Option<super::capture::Exchange>,
    pub(super) capture_attempt: usize,
    sink: Option<Sink>,
    record: Record,
    start: Instant,
    candidate_start: Option<(usize, Instant, u64, u64)>,
    provider: Option<String>,
    telemetry: Option<telemetry::Request>,
}
impl Trace {
    pub(super) fn for_request(sink: Option<&Sink>, request: &axum::extract::Request) -> Self {
        let mut trace = Self::build(
            sink,
            request
                .extensions()
                .get::<super::ingress::correlation::RequestId>()
                .map(|id| id.0.to_string()),
            request
                .extensions()
                .get::<super::ingress::auth::AuthenticatedAgent>()
                .map(|agent| agent.agent_id.to_string()),
        );
        trace.capture = request
            .extensions()
            .get::<super::capture::Exchange>()
            .cloned();
        trace
    }

    pub(super) fn generation(&mut self) {
        self.generation = true;
    }

    #[cfg(test)]
    pub(super) fn generation_trace(sink: Option<&Sink>) -> Self {
        let mut trace = Self::new(sink);
        trace.generation();
        trace
    }

    #[cfg(test)]
    pub(super) fn new(sink: Option<&Sink>) -> Self {
        Self::build(
            sink,
            super::ingress::correlation::RequestIds::new()
                .unwrap()
                .next()
                .map(|id| id.0.to_string()),
            None,
        )
    }

    fn build(sink: Option<&Sink>, request_id: Option<String>, agent_id: Option<String>) -> Self {
        let telemetry = sink.map(|sink| {
            sink.request(
                request_id.as_deref().unwrap_or("unavailable"),
                agent_id.as_deref(),
            )
        });
        Self {
            telemetry,
            report: Default::default(),
            embedding_adaptations: vec![],
            generation: false,
            capture: None,
            capture_attempt: 0,
            sink: sink.cloned(),
            record: Record {
                request_id,
                agent_id,
                ..Default::default()
            },
            start: Instant::now(),
            candidate_start: None,
            provider: None,
        }
    }
    pub(super) fn begin_candidate(&mut self, ordinal: usize) {
        if let Some(telemetry) = &mut self.telemetry {
            telemetry.begin(ordinal);
        }
        self.report.begin(ordinal);
        self.embedding_adaptations.clear();
        self.record.routing = RoutingObservation::default();
        self.capture_attempt = ordinal;

        self.record.affinity = AffinityObservation::default();
        self.record.reported_cached_tokens = None;
        self.record.reported_usage_detail_mask = None;
        self.record.upstream_status = None;
        self.record.retry_after_seconds = None;
        self.record.upstream_head_ms = None;
        self.provider = None;
        if self.sink.is_some() && ordinal <= crate::topology::route::MAX_ROUTE_CANDIDATES {
            self.candidate_start = Some((
                ordinal,
                Instant::now(),
                self.record.received_bytes,
                self.record.handed_off_bytes,
            ));
        }
    }
    pub(super) fn end_candidate(&mut self, error_code: Option<&'static str>, advanced: bool) {
        let observed = self.report.attempt();
        let error_code = error_code.map(|fallback| {
            observed
                .source_failure
                .as_ref()
                .filter(|source| {
                    source.kind != crate::protocol::failure::SourceFailureKind::Other
                        || matches!(
                            observed.task_outcome,
                            Some(crate::execution::report::TaskOutcome::Failed)
                        )
                })
                .map(|source| crate::execution::failure::source_code(&source.kind))
                .or_else(|| {
                    observed
                        .failure
                        .as_ref()
                        .map(|failure| failure.cause.code())
                })
                .unwrap_or(fallback)
        });
        if let Some(telemetry) = &mut self.telemetry {
            telemetry.end(error_code, advanced);
        }
        self.report.end(error_code.is_none());
        if let Some((ordinal, start, received, handed)) = self.candidate_start.take()
            && self.record.candidates.len() < crate::topology::route::MAX_ROUTE_CANDIDATES
        {
            self.record.candidates.push(CandidateRecord {
                embedding_adaptations: std::mem::take(&mut self.embedding_adaptations),
                routing: self.record.routing,
                reported_reasoning: self
                    .report
                    .attempt()
                    .reported_settings
                    .as_ref()
                    .and_then(|s| serde_json::to_value(s).ok()),
                affinity: self.record.affinity,
                ordinal,
                stage: self.record.stage,
                upstream_status: self.record.upstream_status,
                retry_after_seconds: self.record.retry_after_seconds,
                received_bytes: self.record.received_bytes.saturating_sub(received),
                handed_off_bytes: if self.generation {
                    self.report.attempt().handed_off_bytes
                } else {
                    self.record.handed_off_bytes.saturating_sub(handed)
                },
                elapsed_ms: start.elapsed().as_millis().min(u64::MAX as u128) as u64,
                error_code,
                advanced,
                provider: self.provider.take(),
            });
        }
    }
    pub(super) fn executor_order(&mut self, label: &'static str) {
        self.record.executor_order = Some(label);
    }
    pub(super) fn embedding_adaptations(
        &mut self,
        adaptations: &[crate::protocol::embedding::projection::AdaptationRecord],
    ) {
        if self.sink.is_some() {
            self.embedding_adaptations.extend_from_slice(adaptations);
        }
    }
    pub(super) fn selection_skip(&mut self, reason: &'static str) {
        if self.sink.is_some() {
            *self.record.selection_skips.entry(reason).or_default() += 1;
        }
    }
    pub(super) fn health(&mut self, observation: crate::gateway::routing::executors::Observation) {
        self.record.routing.health = Some(observation);
    }
    pub(super) fn probe(&mut self, probing: bool) {
        self.record.routing.recovery_probe = probing.then_some(true);
    }
    pub(super) fn affinity_commit(&mut self, fresh: bool) {
        self.record.routing.completion_state = Some(if fresh { "committed" } else { "stale" });
    }
    pub(super) fn selection_stop(&mut self, reason: &'static str) {
        self.record.routing.stop_reason = Some(reason);
    }
    pub(super) fn executor(&mut self, provider: &str) {
        if self.sink.is_some() {
            self.provider = Some(provider.into());
        }
    }
    pub(super) fn ignored_fields(&mut self, names: Vec<String>) {
        if self.sink.is_some() {
            self.record.ignored_request_field_count = names.len() as u64;
        }
    }
    pub(super) fn ignored_events(&mut self, count: usize) {
        self.record.ignored_upstream_events = count as u64;
    }
    pub(super) fn reported_settings(
        &mut self,
        settings: Option<&crate::semantic::task::generation::GenerationSettings>,
    ) {
        self.report
            .update(|a| a.reported_settings = settings.map(Into::into));
    }
    pub(super) fn stage(&mut self, stage: Stage) {
        self.record.stage = stage;
    }
    pub(super) fn outcome(&mut self, outcome: Outcome) {
        self.record.outcome = outcome;
        self.report.result(match outcome {
            Outcome::Complete => crate::execution::report::ResultState::Succeeded,
            Outcome::Interrupted | Outcome::Shutdown => {
                crate::execution::report::ResultState::Interrupted
            }
            _ => crate::execution::report::ResultState::Failed,
        });
    }
    pub(super) fn head(&mut self, status: u16, headers: &reqwest::header::HeaderMap) {
        self.report.update(|a| a.observed_status = Some(status));
        self.record.upstream_status = Some(status);
        self.record.upstream_head_ms =
            Some(self.start.elapsed().as_millis().min(u64::MAX as u128) as u64);
        self.record.retry_after_seconds = crate::gateway::error::retry_after(headers);
    }
    pub(super) fn received(&mut self, n: usize) {
        if n > 0 && self.record.first_upstream_bytes_ms.is_none() {
            self.record.first_upstream_bytes_ms =
                Some(self.start.elapsed().as_millis().min(u64::MAX as u128) as u64);
        }
        self.record.received_bytes = self.record.received_bytes.saturating_add(n as u64);
    }
    pub(super) fn decode_error(&mut self, error: &crate::execution::AttemptError) {
        let boundary = if matches!(error, crate::execution::AttemptError::Sse(_)) {
            crate::protocol::conversion::generation::ConversionBoundary::EventDecode
        } else {
            crate::protocol::conversion::generation::ConversionBoundary::ResponseDecode
        };
        self.report.failure(Some(boundary), error);
    }
    pub(super) fn projection_error(
        &mut self,
        boundary: crate::protocol::conversion::generation::ConversionBoundary,
        error: &crate::execution::AttemptError,
    ) {
        self.report.failure(Some(boundary), error);
    }
    pub(super) fn reported_usage(&mut self, usage: crate::semantic::task::generation::Usage) {
        if self.sink.is_none() {
            return;
        }
        self.record.reported_cached_tokens = usage.cached_input_tokens;
        self.record.reported_usage_detail_mask = Some(
            [
                usage.input_image_tokens,
                usage.input_text_tokens,
                usage.output_text_tokens,
                usage.accepted_prediction_tokens,
                usage.rejected_prediction_tokens,
                usage.input_audio_tokens,
                usage.output_audio_tokens,
            ]
            .iter()
            .enumerate()
            .fold(0, |mask, (bit, value)| {
                mask | if value.is_some() { 1 << bit } else { 0 }
            }),
        );
    }
    pub(super) fn events(
        &mut self,
        events: &[crate::semantic::task::generation::StreamEvent],
        evidence: &crate::protocol::conversion::generation::EvidenceDelta,
    ) {
        use crate::semantic::task::generation::{ItemKind, StreamEvent};
        for event in events {
            match event {
                StreamEvent::Usage(usage) => self.report.source_usage(
                    &crate::semantic::task::generation::UsageReport::operation(*usage),
                    evidence,
                ),
                StreamEvent::UsageReport(report) => self.report.source_usage(report, evidence),
                _ => {}
            }
        }
        if self.sink.is_none() {
            return;
        }
        for event in events {
            if let StreamEvent::Usage(usage) = event {
                self.reported_usage(*usage);
            }
            if let StreamEvent::UsageReport(report) = event
                && report.scope == crate::semantic::task::generation::UsageScope::Operation
            {
                // Only source-reported detail flags; derived inclusive totals are not reports.
                self.reported_usage(report.reported_usage());
            }
            let counts = &mut self.record.events;
            match event {
                StreamEvent::Delta { fragment, .. } if !fragment.is_empty() => {
                    self.record.first_upstream_delta_ms.get_or_insert_with(|| {
                        self.start.elapsed().as_millis().min(u64::MAX as u128) as u64
                    });
                    counts.event_deltas = counts.event_deltas.saturating_add(1);
                }
                StreamEvent::ItemStarted { kind, .. } => {
                    counts.event_items = counts.event_items.saturating_add(1);
                    if matches!(kind, ItemKind::Reasoning) {
                        counts.event_reasoning_items =
                            counts.event_reasoning_items.saturating_add(1);
                    }
                }
                StreamEvent::PartStarted { .. } => {
                    counts.event_parts = counts.event_parts.saturating_add(1)
                }
                StreamEvent::Delta { .. } => {
                    counts.event_deltas = counts.event_deltas.saturating_add(1)
                }
                StreamEvent::ItemFinished { .. } => {
                    counts.event_item_closures = counts.event_item_closures.saturating_add(1)
                }
                _ => {}
            }
        }
    }
    pub(super) fn image_accounting(
        &mut self,
        usage: Option<&crate::semantic::task::image_generation::ImageUsage>,
        loss: crate::protocol::image::projection::AccountingLoss,
    ) {
        if self.sink.is_none() {
            return;
        }
        self.record.image = ImageAccountingObservation {
            reported_input_tokens: usage.and_then(|u| u.input.value()).copied(),
            reported_output_tokens: usage.and_then(|u| u.output.value()).copied(),
            reported_image_cost_usd: usage
                .and_then(|u| u.billing.cost.value())
                .map(|v| v.amount().to_string()),
            image_usage_omitted: Some(loss.usage_omitted),
            image_billing_omitted: Some(loss.billing_omitted),
        };
    }
    pub(super) fn handed_off(&mut self, n: usize) {
        // Generation is acknowledged atomically by FrameAck, before the
        // producer can resume or be cancelled. Standalone tasks use this path.
        if !self.generation {
            self.record.handed_off_bytes = self.record.handed_off_bytes.saturating_add(n as u64);
        }
    }
    pub(super) fn coalescing_peak(&mut self, bytes: usize) {
        self.record.coalescing_peak_bytes = self.record.coalescing_peak_bytes.max(bytes as u64);
    }
    pub(super) fn affinity(&mut self, source: &'static str, key: bool, session: bool) {
        if self.sink.is_some() {
            self.record.affinity = AffinityObservation {
                affinity_source: Some(source),
                affinity_key_sent: Some(key),
                affinity_session_sent: Some(session),
            };
        }
    }
    pub(super) fn affinity_carriers(&mut self, key: bool, session: bool) {
        if self.sink.is_some() {
            self.record.affinity.affinity_key_sent = Some(key);
            self.record.affinity.affinity_session_sent = Some(session);
        }
    }
}
impl Drop for Trace {
    fn drop(&mut self) {
        self.report.interrupted();
        self.end_candidate(Some("interrupted"), false);
        if let Some(_sink) = self.sink.take() {
            if self.generation {
                let report = self.report.snapshot();
                self.record.handed_off_bytes = report
                    .attempts
                    .iter()
                    .fold(0u64, |sum, a| sum.saturating_add(a.handed_off_bytes));
                self.record.generation = Some(report);
            }
            self.record.capture_exchange = self.capture.as_ref().map(super::capture::Exchange::id);
            self.record.elapsed_ms = self.start.elapsed().as_millis().min(u64::MAX as u128) as u64;
            if let Some(telemetry) = self.telemetry.take() {
                telemetry.finish(&self.record);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderMap;
    use std::sync::mpsc;

    #[tokio::test]
    async fn full_or_disconnected_logging_cannot_change_admission_errors() {
        use axum::{
            body::{Body, to_bytes},
            http::Request,
        };
        use tower::ServiceExt;
        for disconnected in [false, true] {
            let (tx, rx) = mpsc::sync_channel(1);
            let mut gate =
                crate::gateway::tests::support::gateway(crate::gateway::Limits::default());
            std::sync::Arc::get_mut(&mut gate.state)
                .unwrap()
                .diagnostics = Some(Sink::from_sender(tx.clone()));
            let request = || {
                Request::post("/v1/chat/completions")
                    .header(
                        "authorization",
                        format!("Bearer {}", crate::gateway::tests::support::KEY),
                    )
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"model":"missing","messages":[{"role":"user","content":"synthetic"}]}"#,
                    ))
                    .unwrap()
            };
            let first = gate.router().oneshot(request()).await.unwrap();
            assert_eq!(first.status(), 404);
            // First establish that this HTTP path actually records metadata.
            assert!(matches!(rx.try_recv(), Ok(writer::Message::Log(_))));
            let mut rx = Some(rx);
            if disconnected {
                drop(rx.take());
            } else {
                let (ack, _) = tokio::sync::oneshot::channel();
                assert!(tx.try_send(writer::Message::Flush(ack)).is_ok());
            }
            let response = gate.router().oneshot(request()).await.unwrap();
            assert_eq!(response.status(), 404);
            let bytes = to_bytes(response.into_body(), 64 << 10).await.unwrap();
            assert_eq!(
                serde_json::from_slice::<serde_json::Value>(&bytes).unwrap()["error"]["code"],
                "model_not_found"
            );
            if let Some(rx) = rx {
                assert!(matches!(rx.try_recv(), Ok(writer::Message::Flush(_))));
                assert!(matches!(rx.try_recv(), Err(mpsc::TryRecvError::Empty)));
            }
        }
    }

    #[test]
    fn image_accounting_observation_is_exact_opt_in_and_body_free() {
        use crate::{
            protocol::image::projection::{AccountingPolicy, project_response},
            protocol::openrouter_images,
        };
        let response=openrouter_images::decode_response(br#"{"created":1,"data":[{"b64_json":"AQID"}],"usage":{"prompt_tokens":3,"completion_tokens":5,"total_tokens":8,"cost":0.01234567890123456789}}"#).unwrap();
        let projected =
            project_response(&response, AccountingPolicy::OmitUnrepresentableAccounting).unwrap();
        let (tx, _rx) = mpsc::sync_channel(1);
        let sink = Sink::from_sender(tx);
        for enabled in [false, true] {
            let mut trace = Trace::new(enabled.then_some(&sink));
            trace.image_accounting(response.usage.value(), projected.loss);
            let value = serde_json::to_value(&trace.record).unwrap();
            assert!(!value.to_string().contains("AQID"));
            if enabled {
                assert_eq!(value["reported_image_cost_usd"], "0.01234567890123456789");
                assert_eq!(value["reported_input_tokens"], 3);
                assert_eq!(value["image_usage_omitted"], true);
            } else {
                assert!(value.get("reported_image_cost_usd").is_none());
            }
        }
    }
    #[test]
    fn candidate_observations_do_not_mix_status_bytes_or_unbounded_identity() {
        use crate::{
            execution::{
                AttemptError,
                report::{ResultState, View},
            },
            protocol::CodecError,
            protocol::conversion::generation::{
                Change, ConversionBoundary as B, Effect, EvidenceDelta, Field,
            },
        };
        let (tx, _rx) = mpsc::sync_channel(1);
        let sink = Sink::from_sender(tx);
        let mut trace = Trace::new(Some(&sink));
        let mut intake = EvidenceDelta::default();
        intake.push(Effect::new(
            "synthetic.intake.v1",
            Field::HistoryAnnotations,
            B::RequestDecode,
            Change::Approximate,
        ));
        trace.report.intake(intake);
        trace.executor_order("affinity");
        trace.begin_candidate(1);
        trace.probe(true);
        trace.executor("zhipu");
        trace.head(429, &HeaderMap::new());
        trace.received(7);
        let usage = crate::semantic::task::generation::Usage {
            input_text_tokens: Some(0),
            rejected_prediction_tokens: Some(0),
            ..crate::semantic::task::generation::Usage::operation(10, 3, 13)
        };
        trace.reported_usage(usage);
        assert_eq!(trace.record.reported_usage_detail_mask, Some(18));
        let mut disabled = Trace::new(None);
        disabled.reported_usage(usage);
        assert_eq!(disabled.record.reported_usage_detail_mask, None);
        let binding = crate::topology::catalog::API_KEY_BINDINGS
            .iter()
            .find(|b| b.endpoint_prefix == "zhipu")
            .unwrap();
        let endpoint = binding
            .endpoint(crate::topology::GenerationProtocol::OpenAiChat)
            .unwrap();
        let request=crate::protocol::openai::GenerationCodec::new(crate::protocol::openai::GenerationApi::Chat,crate::protocol::openai::Dialect::MorphieCore,None).decode_request(br#"{"model":"m","messages":[{"role":"user","content":"synthetic-private-prompt"}],"reasoning_effort":"medium"}"#).unwrap();
        let converted = crate::execution::attempt::prepare_with_evidence(
            &endpoint,
            &(binding.provider)(),
            &crate::provider::SecretMaterial::new("synthetic-key").unwrap(),
            &request,
        )
        .unwrap();
        assert!(!converted.evidence.is_empty());
        trace
            .report
            .accept(View::UpstreamRequest, converted.evidence);
        let output = Effect::new(
            "responses.omit-system-fingerprint.v1",
            Field::SystemFingerprint,
            B::EventProject,
            Change::Omit,
        );
        trace.report.batch(
            View::DownstreamSse,
            vec![],
            std::iter::once(output).collect(),
        );
        trace.decode_error(&AttemptError::Codec(CodecError::Invalid(
            crate::protocol::failure::ShapeReason::Json,
        )));
        trace.end_candidate(Some("upstream_rate_limit"), true);
        trace.begin_candidate(2);
        assert_eq!(trace.record.reported_usage_detail_mask, None);
        trace.end_candidate(None, false);
        trace.outcome(Outcome::Complete);
        let report = trace.report.snapshot();
        assert_eq!(report.intake.entries.len(), 1);
        assert_eq!(report.attempts[0].observed_status, Some(429));
        assert!(report.attempts[0].failure.is_some());
        assert!(report.attempts[1].failure.is_none());
        assert_eq!(report.result, ResultState::Succeeded);
        assert_eq!(report.producing_attempt, Some(2));
        assert!(report.failure.is_none());
        assert_eq!(trace.record.candidates[0].received_bytes, 7);
        assert_eq!(trace.record.candidates[1].received_bytes, 0);
        assert_eq!(trace.record.candidates[1].upstream_status, None);
        let safe = serde_json::to_value(&report).unwrap();
        assert!(!safe.to_string().contains("synthetic-private"));
        assert!(!safe.to_string().contains("synthetic-key"));
        assert!(
            safe["attempts"][0]["evidence"]["entries"]
                .as_array()
                .unwrap()
                .iter()
                .any(|e| e["input"]["effort"]
                    .as_str()
                    .is_some_and(|s| s.contains("Medium")))
        );
        for n in 3..=100 {
            trace.begin_candidate(n);
            trace.end_candidate(None, false);
        }
        assert_eq!(
            trace.record.candidates.len(),
            crate::topology::route::MAX_ROUTE_CANDIDATES
        );
        assert_eq!(
            trace.report.snapshot().attempts.len(),
            crate::topology::route::MAX_ROUTE_CANDIDATES
        );
        let mut disabled = Trace::new(None);
        disabled.begin_candidate(1);
        disabled.decode_error(&AttemptError::Codec(CodecError::Invalid(
            crate::protocol::failure::ShapeReason::Json,
        )));
        assert!(disabled.report.snapshot().attempts[0].failure.is_some());
    }
    #[test]
    fn messages_projection_classifies_real_codec_failures_without_relaxing_them() {
        use crate::{
            execution::AttemptError,
            protocol::{
                CodecError,
                anthropic::messages::client::sse::ClientEventEncoder,
                failure::{CarrierReason, ShapeReason},
            },
            semantic::task::generation::*,
        };
        for cause in [None, Some(CompletionCause::NaturalOrStopSequence)] {
            let mut encoder = ClientEventEncoder::new("synthetic", "synthetic");
            let mut usage = Usage::operation(10, 3, 13);
            usage.cached_input_tokens = Some(0);
            encoder.encode(&StreamEvent::Usage(usage)).unwrap();
            encoder
                .encode(&StreamEvent::Progress(InteractionProgress::TurnFinished))
                .unwrap();
            let error = encoder
                .encode(&StreamEvent::Terminal {
                    terminal: StreamTerminal::Completed,
                    details: TerminalDetails {
                        completion: cause,
                        ..Default::default()
                    },
                })
                .unwrap_err();
            assert!(matches!(
                error,
                CodecError::Unsupported(CarrierReason::Terminal)
            ));
            let mut trace = Trace::new(None);
            trace.begin_candidate(1);
            trace.projection_error(
                crate::protocol::conversion::generation::ConversionBoundary::EventEncode,
                &AttemptError::Codec(error),
            );
            assert!(trace.report.snapshot().attempts[0].failure.is_some());
            assert!(encoder.finish().is_err());
        }
        let mut encoder = ClientEventEncoder::new("synthetic", "synthetic");
        let error = encoder
            .encode(&StreamEvent::Usage(Usage::operation(10, 3, 13)))
            .unwrap_err();
        assert!(matches!(
            error,
            CodecError::Unsupported(CarrierReason::Usage)
        ));
        let mut trace = Trace::new(None);
        trace.begin_candidate(1);
        trace.projection_error(
            crate::protocol::conversion::generation::ConversionBoundary::EventEncode,
            &AttemptError::Codec(error),
        );
        assert!(matches!(
            trace.report.snapshot().attempts[0]
                .failure
                .as_ref()
                .unwrap()
                .cause,
            AttemptError::Codec(CodecError::Unsupported(CarrierReason::Usage))
        ));
        let mut encoder = ClientEventEncoder::new("synthetic", "synthetic");
        encoder
            .encode(&StreamEvent::Progress(InteractionProgress::TurnFinished))
            .unwrap();
        assert!(matches!(
            encoder.encode(&StreamEvent::Terminal {
                terminal: StreamTerminal::Completed,
                details: TerminalDetails {
                    completion: Some(CompletionCause::Natural),
                    ..Default::default()
                }
            }),
            Err(CodecError::Invalid(ShapeReason::Usage))
        ));
        assert!(encoder.finish().is_err());
    }
    #[test]
    fn decode_failure_never_serializes_untrusted_names_or_values() {
        use crate::{
            execution::AttemptError,
            protocol::{
                CodecError,
                failure::{CarrierReason, ShapeReason},
                openai::{Dialect, GenerationApi, GenerationCodec},
            },
            semantic::task::generation::{ItemId, PartId, StreamEvent},
        };
        let (tx, _rx) = mpsc::sync_channel(1);
        let sink = Sink::from_sender(tx);
        let mut trace = Trace::new(Some(&sink));
        trace.events(
            &[StreamEvent::Delta {
                item: ItemId::new(1),
                part: PartId::new(1),
                fragment: "synthetic-private-body".into(),
                logprobs: vec![],
            }],
            &Default::default(),
        );
        let error=GenerationCodec::new(GenerationApi::Chat,Dialect::Standard,None).decode_request(br#"{"model":"m","messages":[{"role":"user","content":"synthetic-private-body"}],"synthetic-private-field":"synthetic-private-value"}"#).unwrap_err();
        assert!(matches!(
            error,
            CodecError::Unsupported(CarrierReason::UnknownField)
        ));
        trace.begin_candidate(1);
        trace.decode_error(&AttemptError::Codec(error));
        for reason in [
            ShapeReason::SnapshotReplayAdded,
            ShapeReason::SnapshotReplayRemoved,
            ShapeReason::SnapshotReplayChanged,
            ShapeReason::MetadataCreatedChanged,
        ] {
            let mut single = Trace::new(None);
            single.begin_candidate(1);
            single.decode_error(&AttemptError::Codec(CodecError::Invalid(reason)));
            assert!(
                matches!(single.report.snapshot().attempts[0].failure.as_ref().unwrap().cause,AttemptError::Codec(CodecError::Invalid(r)) if r==reason)
            );
        }
        let wire = serde_json::to_string(&trace.report.snapshot()).unwrap();
        assert!(!wire.contains("synthetic"));
        assert!(wire.contains("codec.unsupported.unknown_field"));
        assert_eq!(trace.record.events.event_deltas, 1);
        assert!(trace.record.first_upstream_delta_ms.is_some());
    }
    #[test]
    fn recognized_source_classification_wins_over_preliminary_status_and_handling() {
        use crate::{
            execution::AttemptError,
            protocol::failure::{Observation, SourceFailureEvidence},
        };
        let (tx, _rx) = mpsc::sync_channel(1);
        let sink = Sink::from_sender(tx);
        let mut trace = Trace::new(Some(&sink));
        trace.begin_candidate(1);
        trace.report.failure(
            None,
            &AttemptError::Status {
                status: 429,
                class: crate::provider::RecoveryClass::RateLimit,
            },
        );
        trace.report.source_failure(SourceFailureEvidence::http(
            429,
            br#"{"error":{"code":"insufficient_quota"}}"#,
            Observation::Complete,
        ));
        trace.end_candidate(Some("gateway.quota"), true);
        assert_eq!(trace.record.candidates[0].error_code, Some("source.quota"));
        trace.begin_candidate(2);
        trace
            .report
            .source_failure(SourceFailureEvidence::http(429, b"", Observation::Complete));
        trace.report.failure(
            None,
            &AttemptError::Delivery(crate::execution::attempt::DeliveryReason::BodyConsumerClosed),
        );
        trace.end_candidate(Some("gateway.upstream"), false);
        assert_eq!(
            trace.record.candidates[1].error_code,
            Some("source.rate_limit")
        );
        assert!(trace.report.attempt().handling_failure.is_some());
        trace.begin_candidate(3);
        trace.end_candidate(
            Some(crate::gateway::error::ErrorKind::CredentialUnavailable.diagnostic_code()),
            true,
        );
        assert_eq!(
            trace.record.candidates[2].error_code,
            Some("gateway.credential_unavailable")
        );
    }
    #[test]
    fn full_or_disconnected_sink_does_not_block_a_request() {
        let (tx, rx) = mpsc::sync_channel(1);
        let sink = Sink::from_sender(tx);
        drop(Trace::new(Some(&sink)));
        let trace = Trace::new(Some(&sink));
        let (done, completed) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            drop(trace);
            let _ = done.send(());
        });
        completed
            .recv_timeout(std::time::Duration::from_secs(2))
            .unwrap();
        worker.join().unwrap();
        drop(rx);
        drop(Trace::new(Some(&sink)));
    }
    #[tokio::test]
    async fn metadata_is_closed_bounded_and_never_replaces_existing_files() {
        let directory = crate::test_files::private_directory();
        let path = directory.path().join("events.jsonl");
        let sink = Sink::open(&path).unwrap();
        assert!(Sink::open(&path).is_err());
        let mut trace = Trace::new(Some(&sink));
        let mut upstream = reqwest::header::HeaderMap::new();
        upstream.insert("retry-after", "7".parse().unwrap());
        upstream.insert("x-private", "synthetic-private".parse().unwrap());
        trace.stage(Stage::ResponseHead);
        trace.head(429, &upstream);
        drop(trace);
        drop(Trace::new(None));
        sink.flush().await;
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(!text.contains("synthetic"));
        let records: Vec<serde_json::Value> = text
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .filter(|value: &serde_json::Value| value["event"] == "request")
            .collect();
        assert_eq!(records.len(), 1);
        let value = &records[0];
        assert_eq!(value["request_id"].as_str().unwrap().find(':'), Some(32));
        assert_eq!(value["upstream_status"], 429);
        assert_eq!(value["retry_after_seconds"], 7);
        assert!(text.len() <= 1 << 20);
        drop(sink);
    }
}
