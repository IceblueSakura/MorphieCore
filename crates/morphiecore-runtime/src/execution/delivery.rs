//! Incremental projection and explicit I/O-owned commit. Encoding is not delivery.
use crate::protocol::failure::CarrierReason;
mod coalescing;
mod completion;
mod json;
mod stream;
use crate::{
    adapter::ClientAdapter,
    execution::{
        Attempt, AttemptError, Lifecycle,
        report::{Report, View},
    },
    protocol::{
        ClientProtocol, CodecError, ResponseMetadata,
        conversion::generation::{Effect, EvidenceDelta},
        openai::obfuscation::Obfuscation,
        stream::SseLimits,
    },
    semantic::{
        context::StreamOptions,
        task::generation::{Item, ItemKind, StreamEvent},
    },
};
pub use coalescing::DeltaCoalescing;
use sha2::{Digest, Sha256};
use std::sync::Arc;
pub use stream::PreparedEvents;
use stream::StreamDelivery;

/// Outgoing carrier identity, not an upstream item fact or replay credential.
/// Local decoder counters restart for every response; a history can contain many.
fn delivery_item_id(
    model: &str,
    response: &str,
    owner: crate::semantic::task::generation::ItemId,
    prefix: &str,
) -> String {
    let mut hash = Sha256::new();
    for value in [model, response] {
        hash.update((value.len() as u64).to_le_bytes());
        hash.update(value.as_bytes());
    }
    let digest: String = hash
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    format!(
        "{}_{digest}",
        crate::protocol::fidelity::generated_item_id(owner, prefix)
    )
}

fn response_id() -> Result<String, AttemptError> {
    let mut bytes = [0; 16];
    getrandom::fill(&mut bytes)
        .map_err(|_| AttemptError::Protocol(crate::execution::attempt::ProtocolReason::Entropy))?;
    Ok(format!(
        "resp_{}",
        bytes.iter().map(|b| format!("{b:02x}")).collect::<String>()
    ))
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum ProjectionPhase {
    Open,
    Finalized,
    Rejected,
}

/// Coordinates incremental projection, event queuing, and explicit I/O-owned commit.
pub fn content_event(event: &StreamEvent) -> bool {
    use crate::semantic::task::generation::ItemKind;
    match event {
        StreamEvent::Delta { fragment, .. } => !fragment.is_empty(),
        StreamEvent::ItemStarted {
            kind: ItemKind::ToolCall { .. },
            ..
        }
        | StreamEvent::ToolResultOutput { .. }
        | StreamEvent::AudioDelta { .. }
        | StreamEvent::PartReplay { .. }
        | StreamEvent::ItemFinished {
            replay: Some(_), ..
        } => true,
        _ => false,
    }
}

/// Request-local envelope data, never stored as upstream metadata or history.
struct Envelope {
    public_label: String,
    native_created: Option<serde_json::Number>,
    generated_id: Option<String>,
    presentation: Option<Arc<crate::protocol::GenerationDelivery>>,
    input_estimate: u64,
    request_settings_present: [bool; 3],
}

struct Projection {
    content_ready: bool,
    messages_terminal: Option<crate::protocol::anthropic::messages::client::GoogleTerminalPolicy>,
    messages_without_stops: bool,
    messages_ambiguous_stops: bool,
    source: Option<std::sync::Arc<()>>,
    rendered: bool,
    phase: ProjectionPhase,
    adapter: Option<crate::adapter::ClientProjection>,
}

enum OutputMode {
    Json,
    Sse(Box<StreamDelivery>),
}

impl OutputMode {
    fn stream(&self) -> Option<&StreamDelivery> {
        match self {
            Self::Sse(stream) => Some(stream),
            Self::Json => None,
        }
    }

    fn stream_mut(&mut self) -> Result<&mut StreamDelivery, AttemptError> {
        match self {
            Self::Sse(stream) => Ok(stream),
            Self::Json => Err(AttemptError::Delivery(
                crate::execution::attempt::DeliveryReason::MissingStreamEncoder,
            )),
        }
    }
}

/// Coordinates incremental projection, event queuing, and explicit I/O-owned commit.
///
/// # Delivery Invariants
/// - **Pre-Publication Holding**: Initial lifecycle frames are held in memory until the first
///   content delta is ready, enabling clean failover advancement on unpublished failures.
/// - **Publication Freeze**: Flushing content frames commits the delivery, permanently
///   freezing candidate advancement and translating subsequent upstream failures into
///   terminal error stream events.
/// - **Honest Completion**: Successful termination requires validated upstream EOF before
///   releasing downstream connections.
pub struct ResponseDelivery {
    envelope: Envelope,
    projection: Projection,
    output: OutputMode,
    report: Report,
    client: ClientAdapter,
    limits: SseLimits,
    lifecycle: Lifecycle,
}
impl ResponseDelivery {
    pub fn new(
        mut client: ClientAdapter,
        public_label: &str,
        limits: SseLimits,
        options: StreamOptions,
        padding: Obfuscation,
        streaming: bool,
    ) -> Self {
        client.bind_replay_origin(&crate::protocol::replay_envelope::CLIENT_ORIGIN);
        let report = Report::default();
        report.begin();
        Self {
            envelope: Envelope {
                public_label: public_label.into(),
                native_created: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .ok()
                    .map(|time| time.as_secs().into()),
                generated_id: None,
                presentation: None,
                input_estimate: 0,
                request_settings_present: [false; 3],
            },
            projection: Projection {
                content_ready: false,
                messages_terminal: None,
                messages_without_stops: false,
                messages_ambiguous_stops: false,
                adapter: None,
                source: None,
                rendered: false,
                phase: ProjectionPhase::Open,
            },
            output: if streaming {
                OutputMode::Sse(Box::new(StreamDelivery::new(options, padding)))
            } else {
                OutputMode::Json
            },
            report,
            client,
            limits,
            lifecycle: Lifecycle::new(),
        }
    }
    pub fn is_messages(&self) -> bool {
        self.client.protocol() == ClientProtocol::Messages
    }
    pub fn is_streaming(&self) -> bool {
        matches!(self.output, OutputMode::Sse(_))
    }
    pub fn content_ready(&self) -> bool {
        self.projection.content_ready
    }
    /// Only Responses can deliver an upstream-reported failed response as such.
    pub fn carries_failed_responses(&self) -> bool {
        self.client.protocol() == ClientProtocol::Responses
    }
    pub fn with_request(
        mut self,
        endpoint: &crate::topology::Endpoint,
        request: &crate::protocol::GenerationRequestEnvelope,
        original: &crate::protocol::GenerationRequestEnvelope,
    ) -> Self {
        self.envelope.presentation = Some(Arc::new(crate::protocol::GenerationDelivery {
            settings: if self.client.protocol() == ClientProtocol::Responses {
                completion::settings(&request.task.semantic)
            } else {
                Default::default()
            },
            usage: None,
            request_metadata: if self.client.protocol() == ClientProtocol::Responses {
                original.context.metadata.value().cloned()
            } else {
                Default::default()
            },
        }));
        let settings = request.task.semantic.settings();
        self.envelope.request_settings_present = [
            settings.tools.is_some(),
            settings.tool_choice.is_some(),
            settings.parallel_tool_calls.is_some(),
        ];
        self.envelope.input_estimate = completion::input_estimate(&request.task.semantic);
        if self.is_messages() {
            self.projection.messages_without_stops = original
                .task
                .semantic
                .settings()
                .controls
                .stop_sequences
                .is_empty()
                && settings.controls.stop_sequences.is_empty();
            self.projection.messages_terminal = if self.projection.messages_without_stops {
                endpoint.representation.messages_terminal_policy(request)
            } else {
                None
            };
            self.projection.messages_ambiguous_stops = !self.projection.messages_without_stops
                && endpoint.protocol != crate::topology::GenerationProtocol::AnthropicMessages;
        }
        self
    }
    #[cfg(test)]
    pub fn effects(&self) -> Vec<Effect> {
        self.report
            .attempt()
            .evidence
            .entries
            .into_iter()
            .map(|e| e.effect)
            .collect()
    }
    pub fn with_report(mut self, report: Report) -> Self {
        self.report = report;
        self
    }
    pub fn lifecycle(&self) -> &Lifecycle {
        &self.lifecycle
    }
    // Wire IDs can repeat across attempts; only the execution-owned token binds
    // this delivery. A different attempt needs a fresh uncommitted delivery.
    fn bind(&mut self, attempt: &Attempt) -> Result<(), AttemptError> {
        if self.projection.messages_terminal.is_some()
            && self.projection.messages_terminal != attempt.messages_terminal_source()
        {
            self.projection.phase = ProjectionPhase::Rejected;
            return Err(AttemptError::Delivery(
                crate::execution::attempt::DeliveryReason::TerminalProjectionSourceChanged,
            ));
        }
        if self
            .projection
            .source
            .as_ref()
            .is_some_and(|source| !std::sync::Arc::ptr_eq(source, attempt.identity()))
        {
            self.projection.phase = ProjectionPhase::Rejected;
            return Err(AttemptError::Delivery(
                crate::execution::attempt::DeliveryReason::AttemptChanged,
            ));
        }
        self.projection.source = Some(attempt.identity().clone());
        if self.envelope.generated_id.is_none()
            && attempt
                .metadata()
                .is_some_and(|metadata| metadata.id.is_none())
        {
            self.envelope.generated_id = Some(response_id()?);
        }
        Ok(())
    }
    fn metadata(&self, attempt: &Attempt) -> Result<ResponseMetadata, AttemptError> {
        let mut metadata = attempt.metadata().cloned().ok_or(AttemptError::Protocol(
            crate::execution::attempt::ProtocolReason::MissingResponseMetadata,
        ))?;
        metadata.model = self.envelope.public_label.clone();
        metadata.delivery = self.envelope.presentation.clone();
        if self.client.protocol() == ClientProtocol::Responses {
            metadata.storage = Some(crate::protocol::ResponseStorage {
                store: crate::semantic::value::Presence::Value(false),
                previous_response_id: crate::semantic::value::Presence::Null,
            });
        }
        if let Some(id) = &self.envelope.generated_id {
            // A later source identity must not replace an already chosen client envelope.
            metadata.id = Some(id.clone());
        }
        if metadata.id.is_none() {
            return Err(AttemptError::Delivery(
                crate::execution::attempt::DeliveryReason::MissingDeliveryIdentity,
            ));
        }
        if attempt.is_native() {
            // This is the Gateway envelope's creation time, not an upstream report.
            metadata.created = Some(self.envelope.native_created.clone().ok_or(
                AttemptError::Protocol(crate::execution::attempt::ProtocolReason::GatewayClock),
            )?);
        }
        Ok(metadata)
    }
    fn ready(&self) -> Result<(), AttemptError> {
        if self.projection.phase != ProjectionPhase::Open {
            Err(AttemptError::Protocol(
                crate::execution::attempt::ProtocolReason::DeliveryClosed,
            ))
        } else {
            Ok(())
        }
    }
    pub fn client_response(
        &self,
        attempt: &Attempt,
    ) -> Result<crate::protocol::DecodedResponse, AttemptError> {
        let mut decoded = attempt.response()?.clone();
        decoded.evidence = EvidenceDelta::default();
        decoded.metadata = self.metadata(attempt)?;
        super::replay::seal_for_client(
            &mut decoded,
            &crate::protocol::replay_envelope::CLIENT_ORIGIN,
        )?;
        if self.client.protocol() == ClientProtocol::Responses || attempt.is_native() {
            for (owner, item) in decoded.semantic.items() {
                if decoded.fidelity.response_item_id(*owner).is_none() {
                    decoded.fidelity.record_response_item_id(
                        *owner,
                        &delivery_item_id(
                            &self.envelope.public_label,
                            decoded.metadata.required_id()?,
                            *owner,
                            if matches!(item, Item::Message(_) | Item::Instruction(_)) {
                                "msg"
                            } else {
                                "item"
                            },
                        ),
                    )?;
                }
            }
        }
        Ok(decoded)
    }
    /// Bounded cache-only projection of exactly the delivered values; not stored history.
    pub fn cache_successor(
        &mut self,
        attempt: &Attempt,
        request: &crate::protocol::GenerationRequestEnvelope,
    ) -> Option<crate::protocol::GenerationRequestEnvelope> {
        if self.projection.phase == ProjectionPhase::Rejected
            || self.is_messages()
            || self.lifecycle.state() != crate::execution::DeliveryState::Terminal
            || attempt.response().ok()?.semantic.outcome()
                != crate::semantic::task::generation::Outcome::Completed
            || request.task.semantic.items().iter().any(|(_, item)| {
                matches!(item, crate::semantic::task::generation::Item::Reasoning(r) if r.replay.is_some())
            })
        {
            return None;
        }
        let decoded = self.client_response(attempt).ok()?;
        let mut successor = self.client.cache_successor(request, &decoded)?;
        super::replay::open_from_client(&mut successor).ok()?;
        Some(successor)
    }
    /// Creation and retrieval share presentation; typed replay can retain hidden owners.
    fn complete_presentation(
        &mut self,
        attempt: &Attempt,
        view: View,
    ) -> Result<EvidenceDelta, AttemptError> {
        let Some(presentation) = self.envelope.presentation.as_mut() else {
            return Ok(EvidenceDelta::default());
        };
        let mut evidence = EvidenceDelta::default();
        let response = attempt.response()?;
        let usage_evidence = completion::complete(
            Arc::make_mut(presentation),
            &response.semantic,
            self.client.protocol(),
            self.envelope.input_estimate,
        )?;
        if view != View::DownstreamSse
            || self.client.protocol() != ClientProtocol::Chat
            || self
                .output
                .stream()
                .is_some_and(|stream| stream.options.usage())
        {
            evidence.extend(usage_evidence);
        }
        if self.client.protocol() == ClientProtocol::Responses {
            let source = response.metadata.context.settings();
            for (index, (missing, field, rule)) in [
                (
                    source.is_none_or(|s| s.tools.is_none()),
                    crate::protocol::conversion::generation::Field::Tools,
                    "responses.complete-required-tools.v1",
                ),
                (
                    source.is_none_or(|s| s.tool_choice.is_none()),
                    crate::protocol::conversion::generation::Field::ToolChoice,
                    "responses.complete-required-tool-choice.v1",
                ),
                (
                    source.is_none_or(|s| s.parallel_tool_calls.is_none()),
                    crate::protocol::conversion::generation::Field::ParallelTools,
                    "responses.complete-required-parallel-tools.v1",
                ),
            ]
            .into_iter()
            .enumerate()
            {
                if missing {
                    let change = if self.envelope.request_settings_present[index] {
                        crate::protocol::conversion::generation::Change::InferFromRequest
                    } else {
                        crate::protocol::conversion::generation::Change::Default(
                            crate::protocol::conversion::generation::DefaultBasis::SelectedPolicy,
                        )
                    };
                    evidence.push(Effect::new(
                        rule,
                        field,
                        crate::protocol::conversion::generation::ConversionBoundary::ResponseProject,
                        change,
                    ));
                }
            }
        }
        // A final presentation is new even if the source metadata did not change.
        if let OutputMode::Sse(stream) = &mut self.output {
            stream.prepared_source = None;
        }
        Ok(evidence)
    }
    fn project_events(
        &mut self,
        attempt: &Attempt,
        events: &[StreamEvent],
        streaming: bool,
    ) -> Result<Vec<StreamEvent>, AttemptError> {
        let projected = self
            .projection
            .adapter
            .get_or_insert_with(|| {
                crate::adapter::ClientProjection::new(
                    self.client.clone(),
                    attempt.is_native(),
                    streaming,
                )
                .with_limits(self.limits)
                .with_grouping(attempt.is_native() || attempt.is_responses())
                .with_messages_terminal(
                    self.projection.messages_without_stops,
                    self.projection.messages_ambiguous_stops,
                    self.projection.messages_terminal,
                )
            })
            .project(events)?;
        self.projection.content_ready |= projected.iter().any(content_event);
        Ok(projected)
    }
    /// Call at the external visibility boundary, not when bytes are merely encoded.
    pub fn commit(&mut self) -> Result<(), AttemptError> {
        if self.projection.phase == ProjectionPhase::Rejected || !self.projection.rendered {
            return Err(AttemptError::Delivery(
                crate::execution::attempt::DeliveryReason::NoDeliverableOutput,
            ));
        }
        Ok(self.lifecycle.commit()?)
    }
    /// Call after successful final downstream delivery; EOF alone is insufficient.
    pub fn complete(&mut self, attempt: &Attempt) -> Result<(), AttemptError> {
        self.bind(attempt)?;
        attempt.response()?;
        if self.projection.phase != ProjectionPhase::Finalized
            || self.lifecycle.may_retry_or_fallback()
        {
            return Err(AttemptError::Delivery(
                crate::execution::attempt::DeliveryReason::DeliveryNotComplete,
            ));
        }
        self.lifecycle.terminal()?;
        Ok(())
    }
    pub fn cancel(&mut self) {
        self.projection.phase = ProjectionPhase::Rejected;
        if let OutputMode::Sse(stream) = &mut self.output {
            stream.cancel();
        }
        self.projection.source = None;
        let _ = self.lifecycle.terminal();
    }
}

#[cfg(test)]
mod google_messages_tests;
#[cfg(test)]
mod native_response_tests;
#[cfg(test)]
mod replay_projection_tests;
#[cfg(test)]
mod tests;
