//! Opt-in operator metadata. Never observes bodies, arbitrary headers or identities.
//! Request paths only try-send to a bounded writer; failures cannot alter responses.
use serde::Serialize;
use std::time::Instant;
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
// Only fixed classes escape; unsupported names and error payloads never do.
fn decode_failure(error: &crate::execution::AttemptError) -> &'static str {
    use crate::{
        execution::AttemptError,
        protocol::{CodecError, stream::SseError},
        semantic::task::generation::EventError,
    };
    let codec = match error {
        AttemptError::Codec(error) | AttemptError::Sse(SseError::Codec(error)) => error,
        AttemptError::Sse(_) => return "framing",
        _ => return "other",
    };
    match codec {
        CodecError::StreamFailure(_) => "upstream_stream_error",
        CodecError::Invalid(label) => match *label {
            "response id changed" => "metadata_id_changed",
            "response model changed" => "metadata_model_changed",
            "created changed" => "metadata_created_changed",
            "JSON" => "invalid_json",
            "object" | "response object" => "invalid_object",
            "chunk object" => "invalid_chunk_object",
            "id" | "model" | "created time" | "timestamp" => "invalid_metadata_shape",
            "choices" => "invalid_choices",
            "call identity" => "invalid_call_identity",
            "delta" | "role" => "invalid_delta",
            "usage tail" => "invalid_usage_tail",
            "early usage" => "invalid_early_usage",
            "chunk after finish" | "content after finish" | "repeated finish" => {
                "invalid_after_finish"
            }
            "sequence" => "invalid_sequence",
            "metadata changed"
            | "service tier changed"
            | "fingerprint changed"
            | "reported service changed"
            | "Interactions service_tier changed" => "invalid_metadata",
            "reported settings changed" => "reported_settings_conflict",
            "configuration snapshot changed" => "configuration_conflict",
            "item snapshot" => "invalid_item_snapshot",
            "terminal snapshot" => "invalid_terminal_snapshot",
            "terminal snapshot count" | "terminal snapshot content" => "snapshot_shape",
            "terminal snapshot identity" => "snapshot_identity",
            "terminal snapshot lifecycle" => "snapshot_lifecycle",
            "terminal snapshot phase" => "snapshot_phase",
            "terminal snapshot replay" => "snapshot_replay",
            "terminal snapshot replay added" => "snapshot_replay_added",
            "terminal snapshot replay removed" => "snapshot_replay_removed",
            "terminal snapshot replay changed" => "snapshot_replay_changed",
            "terminal snapshot summary" => "snapshot_summary",
            "terminal snapshot text" => "snapshot_text",
            "terminal snapshot annotations" => "snapshot_annotations",
            "terminal snapshot probability presence" => "snapshot_probability_presence",
            "terminal snapshot probabilities" => "snapshot_probabilities",
            "part snapshot" | "value snapshot" => "invalid_value_snapshot",
            "initial reasoning" | "reasoning status" | "terminal reasoning closure" => {
                "invalid_reasoning"
            }
            "logprobs" | "logprob owner" | "initial text metadata" => "invalid_probabilities",
            "item status" | "initial message" => "invalid_item",
            "item id" | "output index" | "part index" | "part index domain" => "invalid_identity",
            _ => "invalid_other",
        },
        CodecError::Unsupported(_) => "unsupported",
        CodecError::Event(EventError::Identity) => "event_identity",
        CodecError::Event(EventError::Lifecycle) => "event_lifecycle",
        CodecError::Event(EventError::Limit) | CodecError::Limit => "limit",
        CodecError::Event(EventError::EofBeforeTerminal) => "missing_terminal",
        CodecError::Event(EventError::Semantic(_)) | CodecError::Semantic(_) => "semantic",
        _ => "other",
    }
}
#[derive(Clone, PartialEq, Serialize)]
struct LossObservation {
    #[serde(flatten)]
    evidence: crate::lowering::generation::LossEvidence,
    #[serde(skip_serializing_if = "Option::is_none")]
    input: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    output: Option<serde_json::Value>,
}
fn reasoning_controls(
    controls: &crate::lowering::generation::ReasoningControls,
) -> serde_json::Value {
    serde_json::json!({
        "activation":format!("{:?}",controls.activation), "effort":format!("{:?}",controls.effort),
        "budget_tokens":controls.budget_tokens, "mode":format!("{:?}",controls.mode),
        "max_output_tokens":controls.max_output_tokens,
        "summary":format!("{:?}",controls.summary), "display":format!("{:?}",controls.display),
        "configuration_efforts":format!("{:?}",controls.configuration_efforts),
    })
}
#[derive(Serialize)]
struct CandidateRecord {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    embedding_receipts: Vec<crate::lowering::embeddings::Receipt>,
    #[serde(flatten)]
    routing: RoutingObservation,
    #[serde(skip_serializing_if = "Option::is_none")]
    reported_reasoning: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    losses: Vec<LossObservation>,
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
    /// Degradable rule names applied to this candidate's request, never field values.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    request_rules: Vec<&'static str>,
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
    request_id: Option<String>,
    #[serde(flatten)]
    routing: RoutingObservation,
    #[serde(skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    selection_skips: std::collections::BTreeMap<&'static str, u64>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    input_losses: Vec<LossObservation>,
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
    decode_failure: Option<&'static str>,
    projection_failure: Option<&'static str>,
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
    embedding_receipts: Vec<crate::lowering::embeddings::Receipt>,
    reported_reasoning: Option<serde_json::Value>,
    losses: Vec<LossObservation>,
    pub(super) capture: Option<super::capture::Exchange>,
    pub(super) capture_attempt: usize,
    sink: Option<Sink>,
    record: Record,
    start: Instant,
    candidate_start: Option<(usize, Instant, u64, u64)>,
    request_rules: Vec<&'static str>,
    provider: Option<String>,
}
impl Trace {
    pub(super) fn new(sink: Option<&Sink>) -> Self {
        Self {
            embedding_receipts: vec![],
            reported_reasoning: None,
            losses: vec![],
            capture: None,
            capture_attempt: 0,
            sink: sink.cloned(),
            record: Record {
                request_id: sink.and_then(Sink::request_id),
                ..Default::default()
            },
            start: Instant::now(),
            candidate_start: None,
            request_rules: vec![],
            provider: None,
        }
    }
    pub(super) fn begin_candidate(&mut self, ordinal: usize) {
        self.embedding_receipts.clear();
        self.record.routing = RoutingObservation::default();
        self.capture_attempt = ordinal;
        self.losses.clear();
        self.reported_reasoning = None;
        self.record.affinity = AffinityObservation::default();
        self.record.reported_cached_tokens = None;
        self.record.upstream_status = None;
        self.record.retry_after_seconds = None;
        self.record.upstream_head_ms = None;
        self.request_rules.clear();
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
        if let Some((ordinal, start, received, handed)) = self.candidate_start.take()
            && self.record.candidates.len() < crate::topology::route::MAX_ROUTE_CANDIDATES
        {
            self.record.candidates.push(CandidateRecord {
                embedding_receipts: std::mem::take(&mut self.embedding_receipts),
                routing: self.record.routing,
                reported_reasoning: self.reported_reasoning.take(),
                losses: std::mem::take(&mut self.losses),
                affinity: self.record.affinity,
                ordinal,
                stage: self.record.stage,
                upstream_status: self.record.upstream_status,
                retry_after_seconds: self.record.retry_after_seconds,
                received_bytes: self.record.received_bytes.saturating_sub(received),
                handed_off_bytes: self.record.handed_off_bytes.saturating_sub(handed),
                elapsed_ms: start.elapsed().as_millis().min(u64::MAX as u128) as u64,
                error_code,
                advanced,
                request_rules: std::mem::take(&mut self.request_rules),
                provider: self.provider.take(),
            });
        }
    }
    pub(super) fn executor_order(&mut self, label: &'static str) {
        self.record.executor_order = Some(label);
    }
    pub(super) fn embedding_receipts(&mut self, receipts: &[crate::lowering::embeddings::Receipt]) {
        if self.sink.is_some() {
            self.embedding_receipts.extend_from_slice(receipts);
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
    pub(super) fn request_rules(&mut self, rules: Vec<&'static str>) {
        if self.sink.is_some() {
            for rule in &rules {
                if self.losses.iter().any(|loss| {
                    loss.evidence.rule == *rule && loss.evidence.direction == "request.encode"
                }) {
                    continue;
                }
                self.loss(
                    crate::lowering::generation::LossEvidence {
                        rule,
                        field: "protocol.representation",
                        direction: "request.encode",
                        action: "adapt",
                    },
                    None,
                    None,
                );
            }
            self.request_rules = rules;
        }
    }
    fn loss(
        &mut self,
        evidence: crate::lowering::generation::LossEvidence,
        input: Option<serde_json::Value>,
        output: Option<serde_json::Value>,
    ) {
        if self.sink.is_some() && self.losses.len() < 64 {
            let observation = LossObservation {
                evidence,
                input,
                output,
            };
            if !self.losses.contains(&observation) {
                self.losses.push(observation);
            }
        }
    }
    pub(super) fn request_controls(
        &mut self,
        losses: &[crate::lowering::generation::RequestControlLoss],
    ) {
        use crate::lowering::generation::RequestControlLoss as Loss;
        use serde_json::json;
        if self.sink.is_none() {
            return;
        }
        for loss in losses {
            let (input, output) = match loss {
                Loss::Projection(evidence) => {
                    self.loss(evidence.clone(), None, None);
                    continue;
                }
                Loss::Reasoning {
                    input,
                    output,
                    preset_tokens,
                    ..
                } => {
                    let mut output = reasoning_controls(output);
                    output["preset_tokens"] = json!(preset_tokens);
                    (reasoning_controls(input), output)
                }
                Loss::MessagesVerbosity { input, output }
                | Loss::Verbosity { input, output, .. } => {
                    (json!(format!("{input:?}")), json!(format!("{output:?}")))
                }
                Loss::Summary { input, output, .. } => {
                    (json!(format!("{input:?}")), json!(format!("{output:?}")))
                }
                Loss::Role { input, output, .. } => {
                    (json!(format!("{input:?}")), json!(format!("{output:?}")))
                }
                Loss::Sampling {
                    temperature, top_p, ..
                } => (
                    json!({"temperature_present":temperature,"top_p_present":top_p}),
                    json!({"temperature_present":false,"top_p_present":false}),
                ),
            };
            self.loss(
                crate::lowering::generation::LossEvidence {
                    rule: loss.rule(),
                    field: loss.field(),
                    direction: loss.direction(),
                    action: loss.action(),
                },
                Some(input),
                Some(output),
            );
        }
    }
    pub(super) fn request_intake(&mut self, fidelity: &crate::protocol::fidelity::FidelityRecords) {
        if self.sink.is_none() {
            return;
        }
        for normalization in fidelity.normalizations() {
            use crate::protocol::fidelity::Normalization;
            let (rule, field, action) = match normalization {
                Normalization::HistoryAnnotationsDefault => (
                    "responses.default-history-annotations.v1",
                    "generation.content.annotations",
                    "insert",
                ),
                Normalization::Omitted { rule, field } => (*rule, *field, "omit"),
                _ => continue,
            };
            let loss = LossObservation {
                evidence: crate::lowering::generation::LossEvidence {
                    rule,
                    field,
                    direction: "request.decode",
                    action,
                },
                input: None,
                output: None,
            };
            if !self.record.input_losses.contains(&loss) {
                self.record.input_losses.push(loss);
            }
        }
    }
    pub(super) fn reported_settings(
        &mut self,
        settings: Option<&crate::semantic::task::generation::GenerationSettings>,
        fidelity: Option<&crate::protocol::fidelity::FidelityRecords>,
    ) {
        if self.sink.is_none() {
            return;
        }
        if let Some(settings) = settings {
            self.reported_reasoning = Some(reasoning_controls(&settings.into()));
        }
        if let Some(fidelity) = fidelity {
            for normalization in fidelity.normalizations() {
                use crate::protocol::fidelity::Normalization;
                match normalization {
                    Normalization::ReasoningEffortMapping {
                        rule,
                        input,
                        output,
                    } => self.loss(
                        crate::lowering::generation::LossEvidence {
                            rule,
                            field: "response.reported.effort",
                            direction: "response.decode",
                            action: "normalize",
                        },
                        Some(serde_json::json!(input)),
                        Some(serde_json::json!(output)),
                    ),
                    Normalization::Omitted { rule, field } => self.loss(
                        crate::lowering::generation::LossEvidence {
                            rule,
                            field,
                            direction: "response.decode",
                            action: "omit",
                        },
                        None,
                        None,
                    ),
                    _ => {}
                }
            }
        }
    }
    pub(super) fn delivery_losses(&mut self, losses: &[crate::lowering::generation::LossEvidence]) {
        for loss in losses {
            self.loss(loss.clone(), None, None);
        }
    }
    pub(super) fn stage(&mut self, stage: Stage) {
        self.record.stage = stage;
    }
    pub(super) fn outcome(&mut self, outcome: Outcome) {
        self.record.outcome = outcome;
    }
    pub(super) fn head(&mut self, status: u16, headers: &reqwest::header::HeaderMap) {
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
        if self.sink.is_some() {
            self.record.decode_failure = Some(decode_failure(error));
        }
    }
    pub(super) fn projection_error(&mut self, error: &crate::execution::AttemptError) {
        use crate::{
            adapter::AdapterError,
            execution::AttemptError,
            lowering::generation::RepresentationError as R,
            protocol::{CodecError, stream::SseError},
        };
        if self.sink.is_none() {
            return;
        }
        let label = match error {
            AttemptError::Adapter(AdapterError::Representation(error))
            | AttemptError::Representation(error) => match error {
                R::UsageDetails => "usage_details",
                R::UsageProjection => "usage",
                R::ReportedFacts => "reported_facts",
                R::MessageGrouping => "message_grouping",
                R::UnmigratedSemantic => "unmigrated_semantic",
                R::ReplayOrigin | R::ReplayFormat | R::ReplayPhase => "replay",
                R::Metadata => "metadata",
                R::SystemFingerprint => "system_fingerprint",
                R::Terminal => "terminal",
                R::Safety => "safety",
                _ => "other",
            },
            AttemptError::Codec(error)
            | AttemptError::Adapter(AdapterError::Codec(error))
            | AttemptError::Sse(SseError::Codec(error)) => match error {
                // Exact internal labels only. Never serialize an Unsupported string
                // or classify by an untrusted prefix.
                CodecError::Unsupported(reason) => match reason.as_str() {
                    "Messages terminal"
                    | "Messages completion cause"
                    | "Messages continuation"
                    | "Messages partial tool terminal" => "terminal",
                    "Messages usage"
                    | "Messages usage report"
                    | "Messages usage scopes"
                    | "Messages late cache breakdown"
                    | "Messages erased usage observation" => "usage",
                    _ => "codec",
                },
                CodecError::Invalid(
                    "Messages usage" | "Messages output usage" | "Messages projected usage",
                ) => "usage",
                _ => "codec",
            },
            _ => "codec",
        };
        self.record.projection_failure = Some(label);
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
    pub(super) fn events(&mut self, events: &[crate::semantic::task::generation::StreamEvent]) {
        use crate::semantic::task::generation::{ItemKind, StreamEvent};
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
                self.reported_usage(report.counts);
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
        loss: crate::lowering::images::AccountingLoss,
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
        self.record.handed_off_bytes = self.record.handed_off_bytes.saturating_add(n as u64);
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
        self.end_candidate(Some("interrupted"), false);
        if let Some(sink) = self.sink.take() {
            self.record.capture_exchange = self.capture.as_ref().map(super::capture::Exchange::id);
            self.record.elapsed_ms = self.start.elapsed().as_millis().min(u64::MAX as u128) as u64;
            sink.record(Box::new(std::mem::take(&mut self.record)));
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
            assert!(matches!(rx.try_recv(), Ok(writer::Message::Record(_))));
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
            lowering::images::{AccountingPolicy, project_response},
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
        let (tx, _rx) = mpsc::sync_channel(1);
        let sink = Sink::from_sender(tx);
        let mut trace = Trace::new(Some(&sink));
        let mut fidelity = crate::protocol::fidelity::FidelityRecords::default();
        fidelity.record_history_annotations_default();
        trace.request_intake(&fidelity);
        trace.request_intake(&fidelity);
        assert_eq!(
            serde_json::to_value(&trace.record.input_losses).unwrap(),
            serde_json::json!([{
                "rule":"responses.default-history-annotations.v1",
                "field":"generation.content.annotations",
                "direction":"request.decode","action":"insert"
            }])
        );
        trace.executor_order("affinity");
        trace.begin_candidate(1);
        trace.probe(true);
        trace.health(crate::gateway::routing::executors::Observation {
            reason: "failure",
            scope: "pool",
            source: "retry_after",
            retry_after_seconds: 7,
        });
        trace.selection_skip("failure_scope");
        trace.executor("zhipu");
        trace.head(429, &HeaderMap::new());
        trace.received(7);
        use crate::protocol::openai::{Adapter, Dialect, Profile};
        let binding = crate::topology::catalog::API_KEY_BINDINGS
            .iter()
            .find(|binding| binding.endpoint_prefix == "zhipu")
            .unwrap();
        let endpoint = binding
            .endpoint(crate::topology::ProtocolProfile::OpenAiChat)
            .unwrap();
        let request = Adapter::new(Profile::Chat, Dialect::MorphieCore, None).decode_request(
            br#"{"model":"m","messages":[{"role":"user","content":"synthetic-private-prompt"}],"reasoning_effort":"medium"}"#
        ).unwrap();
        let (_, _, losses) = crate::execution::attempt::prepare_with_evidence(
            &endpoint,
            &(binding.provider)(),
            &crate::provider::SecretMaterial::new("synthetic-key").unwrap(),
            &request,
        )
        .unwrap();
        assert_eq!(losses.len(), 1);
        trace.request_controls(&losses);
        trace.request_controls(&losses);
        trace.reported_settings(None, None);
        let output_loss = crate::lowering::generation::LossEvidence {
            rule: "responses.omit-system-fingerprint.v1",
            field: "response.context.system_fingerprint",
            direction: "event.encode",
            action: "omit",
        };
        trace.delivery_losses(&[output_loss.clone(), output_loss]);
        trace.end_candidate(Some("upstream_rate_limit"), true);
        assert_eq!(trace.record.candidates[0].losses.len(), 2);
        let safe = serde_json::to_value(&trace.record.candidates[0]).unwrap();
        assert!(
            safe["losses"][0]["input"]["effort"]
                .as_str()
                .unwrap()
                .contains("Medium")
        );
        assert!(
            safe["losses"][0]["output"]["effort"]
                .as_str()
                .unwrap()
                .contains("High")
        );
        assert!(!safe.to_string().contains("synthetic-private-prompt"));
        assert!(safe.get("reported_reasoning").is_none());
        let grouped = Adapter::new(Profile::Chat,Dialect::Standard,None).decode_request(
            br#"{"model":"m","messages":[{"role":"assistant","content":null,"tool_calls":[{"id":"c","type":"function","function":{"name":"lookup","arguments":"{}"}}]},{"role":"tool","tool_call_id":"c","content":"done"},{"role":"user","content":"go"}]}"#).unwrap();
        let mut responses = endpoint.clone();
        responses.protocol = crate::topology::ProtocolProfile::OpenAiResponses;
        responses.representation = crate::protocol::openai::TargetCaps::full().into();
        let mut provider = (binding.provider)();
        provider.endpoints.insert(
            crate::provider::ProviderOperation::Responses,
            crate::provider::ProviderEndpoint {
                path: responses.target.path.clone(),
                auth: None,
            },
        );
        let (_, rules, losses) = crate::execution::attempt::prepare_with_evidence(
            &responses,
            &provider,
            &crate::provider::SecretMaterial::new("synthetic-key").unwrap(),
            &grouped,
        )
        .unwrap();
        assert!(rules.contains(&"responses.omit-message-envelopes.v1"));
        trace.begin_candidate(2);
        trace.request_controls(&losses);
        trace.request_rules(rules);
        trace.request_controls(&losses);
        trace.end_candidate(None, false);
        let exact: Vec<_> = trace.record.candidates[1]
            .losses
            .iter()
            .filter(|loss| loss.evidence.rule == "responses.omit-message-envelopes.v1")
            .collect();
        assert_eq!(exact.len(), 1);
        assert_eq!(exact[0].evidence.field, "generation.message_envelopes");
        assert_eq!(exact[0].evidence.direction, "request.encode");
        trace.record.candidates.pop();
        trace.begin_candidate(2);
        trace.stage(Stage::Prepare);
        trace.selection_stop("no_successor");
        trace.end_candidate(Some("credential_unavailable"), false);
        assert_eq!(trace.record.candidates[0].upstream_status, Some(429));
        assert_eq!(trace.record.candidates[0].received_bytes, 7);
        assert_eq!(trace.record.candidates[1].upstream_status, None);
        assert_eq!(trace.record.candidates[1].received_bytes, 0);
        for n in 3..=100 {
            trace.begin_candidate(n);
            trace.end_candidate(None, false);
        }
        assert_eq!(trace.record.candidates.len(), 64);
        let wire = serde_json::to_value(&trace.record.candidates[0]).unwrap();
        assert_eq!(wire["ordinal"], 1);
        assert_eq!(wire["provider"], "zhipu");
        assert!(
            serde_json::to_value(&trace.record.candidates[1])
                .unwrap()
                .get("provider")
                .is_none()
        );
        assert_eq!(
            serde_json::to_value(&trace.record).unwrap()["executor_order"],
            "affinity"
        );
        assert_eq!(wire["advanced"], true);
        assert_eq!(wire["health"]["scope"], "pool");
        assert_eq!(wire["recovery_probe"], true);
        let local = serde_json::to_value(&trace.record.candidates[1]).unwrap();
        assert_eq!(local["stage"], "prepare");
        assert_eq!(local["stop_reason"], "no_successor");
        assert!(local.get("health").is_none());
        assert!(local.get("recovery_probe").is_none());
        assert_eq!(
            serde_json::to_value(&trace.record).unwrap()["selection_skips"]["failure_scope"],
            1
        );
        assert!(wire.get("endpoint").is_none());
        assert!(wire.get("body").is_none());
    }
    #[test]
    fn messages_projection_classifies_real_codec_failures_without_relaxing_them() {
        use crate::{
            adapter::AdapterError,
            execution::AttemptError,
            protocol::{
                CodecError, anthropic::messages::client::sse::ClientEventEncoder, stream::SseError,
            },
            semantic::task::generation::*,
        };
        let (tx, _rx) = mpsc::sync_channel(1);
        let sink = Sink::from_sender(tx);
        let mut trace = Trace::new(Some(&sink));
        for (cause, expected) in [
            (None, "Messages terminal"),
            (
                Some(CompletionCause::NaturalOrStopSequence),
                "Messages completion cause",
            ),
        ] {
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
            assert!(matches!(&error, CodecError::Unsupported(reason) if reason == expected));
            trace.projection_error(&AttemptError::Codec(error));
            assert_eq!(trace.record.projection_failure, Some("terminal"));
            assert!(encoder.finish().is_err());
        }
        let mut encoder = ClientEventEncoder::new("synthetic", "synthetic");
        // Includes-cache input without a cache count cannot become Messages input.
        let error = encoder
            .encode(&StreamEvent::Usage(Usage::operation(10, 3, 13)))
            .unwrap_err();
        assert!(matches!(&error, CodecError::Unsupported(reason) if reason == "Messages usage"));
        trace.projection_error(&AttemptError::Adapter(AdapterError::Codec(error)));
        assert_eq!(trace.record.projection_failure, Some("usage"));
        let mut encoder = ClientEventEncoder::new("synthetic", "synthetic");
        encoder
            .encode(&StreamEvent::Progress(InteractionProgress::TurnFinished))
            .unwrap();
        let error = encoder
            .encode(&StreamEvent::Terminal {
                terminal: StreamTerminal::Completed,
                details: TerminalDetails {
                    completion: Some(CompletionCause::Natural),
                    ..Default::default()
                },
            })
            .unwrap_err();
        assert!(matches!(error, CodecError::Invalid("Messages usage")));
        trace.projection_error(&AttemptError::Sse(SseError::Codec(error)));
        assert_eq!(trace.record.projection_failure, Some("usage"));
        assert!(encoder.finish().is_err());
        trace.projection_error(&AttemptError::Codec(CodecError::Unsupported(
            "Messages usage: synthetic-private-value".into(),
        )));
        assert_eq!(trace.record.projection_failure, Some("codec"));
        assert!(
            !serde_json::to_string(&trace.record)
                .unwrap()
                .contains("synthetic")
        );
    }
    #[test]
    fn decode_failure_never_serializes_untrusted_names_or_values() {
        use crate::{
            execution::AttemptError,
            protocol::CodecError,
            semantic::task::generation::{ItemId, PartId, StreamEvent},
        };
        let (tx, _rx) = mpsc::sync_channel(1);
        let sink = Sink::from_sender(tx);
        let mut trace = Trace::new(Some(&sink));
        trace.events(&[StreamEvent::Delta {
            item: ItemId::new(1),
            part: PartId::new(1),
            fragment: "synthetic-private-body".into(),
            logprobs: vec![],
        }]);
        assert!(trace.record.first_upstream_delta_ms.is_some());
        trace.projection_error(&AttemptError::Adapter(
            crate::adapter::AdapterError::Representation(
                crate::lowering::generation::RepresentationError::UsageDetails,
            ),
        ));
        assert_eq!(trace.record.projection_failure, Some("usage_details"));
        trace.projection_error(&AttemptError::Codec(CodecError::Unsupported(
            "synthetic-private-projection".into(),
        )));
        assert_eq!(trace.record.projection_failure, Some("codec"));
        let usage = crate::semantic::task::generation::Usage {
            input_text_tokens: Some(0),
            rejected_prediction_tokens: Some(0),
            ..crate::semantic::task::generation::Usage::operation(3, 2, 5)
        };
        trace.reported_usage(usage);
        assert_eq!(trace.record.reported_usage_detail_mask, Some(18));
        let mut disabled = Trace::new(None);
        disabled.reported_usage(usage);
        assert!(disabled.record.reported_usage_detail_mask.is_none());
        trace.decode_error(&AttemptError::Codec(CodecError::Unsupported(
            "synthetic-private-field".into(),
        )));
        let wire = serde_json::to_string(&trace.record).unwrap();
        assert!(!wire.contains("synthetic"));
        assert!(wire.contains("unsupported"));
        assert_eq!(trace.record.events.event_deltas, 1);
        trace.decode_error(&AttemptError::Codec(CodecError::Invalid(
            "terminal snapshot",
        )));
        assert_eq!(
            trace.record.decode_failure,
            Some("invalid_terminal_snapshot")
        );
        for (label, expected) in [
            ("chunk object", "invalid_chunk_object"),
            ("created changed", "metadata_created_changed"),
            ("choices", "invalid_choices"),
            ("usage tail", "invalid_usage_tail"),
            ("synthetic-private-field", "invalid_other"),
            ("terminal snapshot replay added", "snapshot_replay_added"),
            (
                "terminal snapshot replay removed",
                "snapshot_replay_removed",
            ),
            (
                "terminal snapshot replay changed",
                "snapshot_replay_changed",
            ),
        ] {
            trace.decode_error(&AttemptError::Codec(CodecError::Invalid(label)));
            assert_eq!(trace.record.decode_failure, Some(expected));
        }
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
