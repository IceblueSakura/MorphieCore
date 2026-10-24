//! Incremental projection and explicit I/O-owned commit. Encoding is not delivery.
mod coalescing;
mod completion;
use crate::{
    adapter::{ClientAdapter, ClientEncoder, ClientProtocol},
    execution::{Attempt, AttemptError, Lifecycle},
    protocol::{
        CodecError, ResponseMetadata, fidelity::FidelityRecords, openai::obfuscation::Obfuscation,
        stream::SseLimits,
    },
    semantic::{context::StreamOptions, task::generation::StreamEvent},
};
use bytes::Bytes;
pub use coalescing::DeltaCoalescing;
use sha2::{Digest, Sha256};
use std::sync::Arc;

/// Outgoing carrier identity, not an upstream item fact or replay credential.
/// Local decoder counters restart for every response; a history can contain many.
fn delivery_item_id(
    model: &str,
    response: &str,
    owner: crate::semantic::task::generation::ItemId,
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
        crate::protocol::fidelity::generated_item_id(owner)
    )
}

fn response_id() -> Result<String, AttemptError> {
    let mut bytes = [0; 16];
    getrandom::fill(&mut bytes).map_err(|_| AttemptError::Protocol("entropy"))?;
    Ok(format!(
        "resp_{}",
        bytes.iter().map(|b| format!("{b:02x}")).collect::<String>()
    ))
}

#[derive(Clone)]
struct EventContext {
    metadata: ResponseMetadata,
    fidelity: Arc<FidelityRecords>,
    accounted_bytes: std::sync::OnceLock<Option<usize>>,
    frame_bytes: std::sync::OnceLock<usize>,
}

/// Raw intake values let unchanged batches reuse the effective delivery context.
/// Generated aliases must not make every following delta look like a source change.
struct PreparedSource {
    metadata: ResponseMetadata,
    fidelity: Arc<FidelityRecords>,
    context: Arc<EventContext>,
}

/// Already projected and sealed. Consume in order on the delivery that prepared it.
pub(crate) struct PreparedEvents {
    pub(crate) events: Vec<StreamEvent>,
    context: Arc<EventContext>,
    /// Preserve barriers even when native projection omits their wire event.
    barrier: bool,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum ProjectionPhase {
    Open,
    Finalized,
    Rejected,
}

pub struct ResponseDelivery {
    losses: Vec<crate::lowering::generation::LossEvidence>,
    client: ClientAdapter,
    messages_terminal: Option<crate::protocol::anthropic::messages::client::GoogleTerminalPolicy>,
    public_label: String,
    limits: SseLimits,
    options: StreamOptions,
    padding: Option<Obfuscation>,
    encoder: Option<ClientEncoder>,
    prepared_source: Option<PreparedSource>,
    coalescer: Option<coalescing::Coalescer>,
    source: Option<std::sync::Arc<()>>,
    lifecycle: Lifecycle,
    rendered: bool,
    phase: ProjectionPhase,
    native_created: Option<serde_json::Number>,
    /// Request-local envelope identity, never stored as upstream metadata or history.
    generated_id: Option<String>,
    storage: Option<(String, crate::protocol::ResponseStorage)>,
    presentation: Option<Arc<crate::protocol::GenerationDelivery>>,
    input_estimate: u64,
}
impl ResponseDelivery {
    pub fn new(
        mut client: ClientAdapter,
        public_label: &str,
        limits: SseLimits,
        options: StreamOptions,
        padding: Obfuscation,
    ) -> Self {
        client.bind_replay_origin(&super::replay::CLIENT_ORIGIN);
        Self {
            losses: vec![],
            client,
            messages_terminal: None,
            public_label: public_label.into(),
            limits,
            options,
            padding: Some(padding),
            encoder: None,
            prepared_source: None,
            coalescer: None,
            source: None,
            lifecycle: Lifecycle::new(),
            rendered: false,
            phase: ProjectionPhase::Open,
            native_created: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .ok()
                .map(|time| time.as_secs().into()),
            generated_id: None,
            storage: None,
            presentation: None,
            input_estimate: 0,
        }
    }
    pub fn is_messages(&self) -> bool {
        self.client.protocol() == ClientProtocol::Messages
    }
    pub(crate) fn with_request(
        mut self,
        endpoint: &crate::topology::Endpoint,
        request: &crate::adapter::Request,
    ) -> Self {
        self.presentation = Some(Arc::new(crate::protocol::GenerationDelivery {
            settings: if self.client.protocol() == ClientProtocol::Responses {
                completion::settings(&request.task.semantic)
            } else {
                Default::default()
            },
            usage: None,
        }));
        self.input_estimate = completion::input_estimate(&request.task.semantic);
        if self.is_messages() {
            self.messages_terminal = endpoint.representation.messages_terminal_policy(request);
        }
        self
    }
    pub(crate) fn with_coalescing(mut self, policy: Option<DeltaCoalescing>) -> Self {
        self.coalescer = policy.map(coalescing::Coalescer::new);
        self
    }
    pub(crate) fn flush_deadline(&self) -> Option<std::time::Instant> {
        self.coalescer
            .as_ref()
            .and_then(coalescing::Coalescer::deadline)
    }
    pub(crate) fn coalescing_peak_bytes(&self) -> usize {
        self.coalescer.as_ref().map_or(0, |c| c.peak_bytes)
    }
    pub(crate) fn must_flush_before(&self, attempt: &Attempt, events: &[StreamEvent]) -> bool {
        self.flush_deadline().is_some()
            && (events.iter().any(|event| {
                !matches!(
                    event, StreamEvent::Delta { fragment, logprobs, .. }
                        if !fragment.is_empty() && logprobs.is_empty()
                )
            }) || self.prepared_source.as_ref().is_none_or(|source| {
                attempt.metadata() != Some(&source.metadata)
                    || attempt.fidelity() != Some(source.fidelity.as_ref())
            }))
    }
    pub(crate) fn queue_events(
        &mut self,
        attempt: &Attempt,
        events: &[StreamEvent],
        now: std::time::Instant,
    ) -> Result<Vec<PreparedEvents>, AttemptError> {
        let Some(prepared) = self.prepare_events(attempt, events)? else {
            return Ok(vec![]);
        };
        match &mut self.coalescer {
            Some(coalescer) => Ok(coalescer.push(prepared, now, self.limits.max_event_bytes)),
            None => Ok(vec![prepared]),
        }
    }
    pub(crate) fn flush_pending(&mut self) -> Result<Vec<Bytes>, AttemptError> {
        self.ready()?;
        match self
            .coalescer
            .as_mut()
            .and_then(coalescing::Coalescer::flush)
        {
            Some(batch) => self.encode_prepared(batch),
            None => Ok(vec![]),
        }
    }
    pub(crate) fn losses(&self) -> &[crate::lowering::generation::LossEvidence] {
        &self.losses
    }
    pub(crate) fn with_storage(
        mut self,
        id: &str,
        storage: crate::protocol::ResponseStorage,
    ) -> Self {
        self.storage = Some((id.into(), storage));
        self
    }
    fn managed_responses(&self) -> bool {
        self.client.protocol() == ClientProtocol::Responses
            && self.storage.as_ref().is_some_and(|(_, storage)| {
                storage.store == crate::semantic::value::Presence::Value(true)
            })
    }
    pub fn lifecycle(&self) -> &Lifecycle {
        &self.lifecycle
    }
    // Wire IDs can repeat across attempts; only the execution-owned token binds
    // this delivery. A different attempt needs a fresh uncommitted delivery.
    fn bind(&mut self, attempt: &Attempt) -> Result<(), AttemptError> {
        if self.messages_terminal.is_some()
            && self.messages_terminal != attempt.messages_terminal_source()
        {
            self.phase = ProjectionPhase::Rejected;
            return Err(AttemptError::Delivery("terminal projection source changed"));
        }
        if self
            .source
            .as_ref()
            .is_some_and(|source| !std::sync::Arc::ptr_eq(source, attempt.identity()))
        {
            self.phase = ProjectionPhase::Rejected;
            return Err(AttemptError::Delivery("attempt changed"));
        }
        self.source = Some(attempt.identity().clone());
        if self.generated_id.is_none()
            && self.storage.is_none()
            && attempt
                .metadata()
                .is_some_and(|metadata| metadata.id.is_none())
        {
            self.generated_id = Some(response_id()?);
        }
        Ok(())
    }
    fn metadata(&self, attempt: &Attempt) -> Result<ResponseMetadata, AttemptError> {
        let mut metadata = attempt
            .metadata()
            .cloned()
            .ok_or(AttemptError::Protocol("missing response metadata"))?;
        metadata.model = self.public_label.clone();
        metadata.delivery = self.presentation.clone();
        if let Some((id, storage)) = &self.storage {
            metadata.id = Some(id.clone());
            metadata.storage = Some(storage.clone());
        } else if let Some(id) = &self.generated_id {
            // A later source identity must not replace an already chosen client envelope.
            metadata.id = Some(id.clone());
        }
        if metadata.id.is_none() {
            return Err(AttemptError::Delivery("missing delivery identity"));
        }
        if attempt.is_native() {
            // This is the Gateway envelope's creation time, not an upstream report.
            metadata.created = Some(
                self.native_created
                    .clone()
                    .ok_or(AttemptError::Protocol("gateway clock"))?,
            );
        }
        Ok(metadata)
    }
    fn ready(&self) -> Result<(), AttemptError> {
        if self.phase != ProjectionPhase::Open {
            Err(AttemptError::Protocol("delivery closed"))
        } else {
            Ok(())
        }
    }
    pub(crate) fn client_response(
        &mut self,
        attempt: &Attempt,
    ) -> Result<crate::protocol::DecodedResponse, AttemptError> {
        let (decoded, losses) = self.client_view(attempt)?;
        for loss in losses {
            if !self.losses.contains(&loss) {
                self.losses.push(loss);
            }
        }
        Ok(decoded)
    }
    /// The client-facing response and the losses of producing it, without recording them.
    fn client_view(
        &self,
        attempt: &Attempt,
    ) -> Result<
        (
            crate::protocol::DecodedResponse,
            Vec<crate::lowering::generation::LossEvidence>,
        ),
        AttemptError,
    > {
        let mut decoded = attempt.response()?.clone();
        decoded.metadata = self.metadata(attempt)?;
        super::replay::seal_for_client(&mut decoded, &super::replay::CLIENT_ORIGIN)?;
        let mut losses = if attempt.is_native() {
            self.client.project_native(&mut decoded)?
        } else {
            vec![]
        };
        if let Some(policy) = self.messages_terminal
            && let Some((details, loss)) = policy.project(&decoded.semantic, "response.encode")?
        {
            decoded.semantic = decoded
                .semantic
                .with_details(details)
                .map_err(crate::protocol::CodecError::from)?;
            losses.push(loss);
        }
        if self.client.protocol() == ClientProtocol::Responses || attempt.is_native() {
            for (owner, _) in decoded.semantic.items() {
                if decoded.fidelity.response_item_id(*owner).is_none() {
                    decoded.fidelity.record_response_item_id(
                        *owner,
                        &delivery_item_id(
                            &self.public_label,
                            decoded.metadata.required_id()?,
                            *owner,
                        ),
                    )?;
                }
            }
        }
        Ok((decoded, losses))
    }
    /// Bounded cache-only projection of exactly the delivered values; not stored history.
    pub(crate) fn cache_successor(
        &mut self,
        attempt: &Attempt,
        request: &crate::adapter::Request,
    ) -> Option<crate::adapter::Request> {
        if self.phase == ProjectionPhase::Rejected
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
    pub fn encode_json(&mut self, attempt: &Attempt) -> Result<Vec<u8>, AttemptError> {
        self.encode_json_snapshot(attempt).map(|(_, body)| body)
    }
    /// Finalize public JSON and retain its typed history, including managed-only replay.
    pub(crate) fn encode_json_snapshot(
        &mut self,
        attempt: &Attempt,
    ) -> Result<(crate::protocol::DecodedResponse, Vec<u8>), AttemptError> {
        let result = (|| {
            self.ready()?;
            if self.encoder.is_some() {
                return Err(AttemptError::Delivery("stream already started"));
            }
            self.bind(attempt)?;
            let snapshot = self.snapshot(attempt)?;
            self.rendered = true;
            self.phase = ProjectionPhase::Finalized;
            Ok(snapshot)
        })();
        if result.is_err() {
            self.phase = ProjectionPhase::Rejected;
        }
        result
    }
    /// Creation and retrieval share presentation; typed replay can retain hidden owners.
    fn complete_presentation(&mut self, attempt: &Attempt) -> Result<(), AttemptError> {
        let Some(presentation) = self.presentation.as_mut() else {
            return Ok(());
        };
        let response = attempt.response()?;
        let action = completion::complete(
            Arc::make_mut(presentation),
            &response.semantic,
            self.client.protocol(),
            self.input_estimate,
        )?;
        if let Some(action) = action {
            let loss = crate::lowering::generation::LossEvidence {
                rule: "generation.complete-delivery-usage.v1",
                field: "response.delivery.usage",
                direction: "response.encode",
                action,
            };
            if !self.losses.contains(&loss) {
                self.losses.push(loss);
            }
        }
        if self.client.protocol() == ClientProtocol::Responses
            && response.metadata.context.settings().is_none_or(|s| {
                s.tools.is_none() || s.tool_choice.is_none() || s.parallel_tool_calls.is_none()
            })
        {
            let loss = crate::lowering::generation::LossEvidence {
                rule: "responses.infer-required-settings.v1",
                field: "response.delivery.settings",
                direction: "response.encode",
                action: "infer",
            };
            if !self.losses.contains(&loss) {
                self.losses.push(loss);
            }
        }
        // A final presentation is new even if the source metadata did not change.
        self.prepared_source = None;
        Ok(())
    }
    pub(crate) fn snapshot(
        &mut self,
        attempt: &Attempt,
    ) -> Result<(crate::protocol::DecodedResponse, Vec<u8>), AttemptError> {
        self.complete_presentation(attempt)?;
        let response = self.client_response(attempt)?;
        let (decoded, value, losses) = self
            .client
            .snapshot_with_history(response, self.managed_responses())?;
        for loss in losses {
            if !self.losses.contains(&loss) {
                self.losses.push(loss);
            }
        }
        let body =
            serde_json::to_vec(&value).map_err(|_| AttemptError::Protocol("body serialization"))?;
        Ok((decoded, body))
    }
    pub fn encode_events(
        &mut self,
        attempt: &Attempt,
        events: &[StreamEvent],
    ) -> Result<Vec<Bytes>, AttemptError> {
        match self.prepare_events(attempt, events)? {
            Some(prepared) => self.encode_prepared(prepared),
            None => Ok(vec![]),
        }
    }
    /// Freeze nonterminal projection before intake can advance metadata or fidelity.
    pub(crate) fn prepare_events(
        &mut self,
        attempt: &Attempt,
        events: &[StreamEvent],
    ) -> Result<Option<PreparedEvents>, AttemptError> {
        let result = (|| {
            self.ready()?;
            if events.is_empty() {
                return Ok(None);
            }
            if events
                .iter()
                .any(|e| matches!(e, StreamEvent::Terminal { .. }))
            {
                return Err(AttemptError::Delivery(
                    "terminal requires validated upstream EOF",
                ));
            }
            self.prepare(attempt, events).map(Some)
        })();
        self.collect_losses();
        if result.is_err() {
            self.phase = ProjectionPhase::Rejected;
        }
        result
    }
    /// Encode the terminal only after the upstream framing/body has closed cleanly.
    pub fn finish_stream(&mut self, attempt: &Attempt) -> Result<Vec<Bytes>, AttemptError> {
        let result = (|| {
            self.ready()?;
            let mut terminal = attempt.terminal_event()?.clone();
            self.complete_presentation(attempt)?;
            if self.is_messages() {
                crate::protocol::anthropic::messages::client::validate_tool_calls(
                    attempt.response()?,
                )?;
            }
            if let Some(policy) = self.messages_terminal
                && let Some((projected, loss)) =
                    policy.project(&attempt.response()?.semantic, "event.encode")?
            {
                if let StreamEvent::Terminal { details, .. } = &mut terminal {
                    *details = projected;
                }
                if !self.losses.contains(&loss) {
                    self.losses.push(loss);
                }
            }
            if self
                .encoder
                .as_ref()
                .is_some_and(|e| e.needs_phase_source_check())
            {
                self.client
                    .check_chat_source(&self.client_view(attempt)?.0)?;
            }
            let mut frames = self.flush_pending()?;
            if !self.managed_responses()
                && attempt
                    .response()?
                    .semantic
                    .items()
                    .iter()
                    .any(|(_, item)| item.has_replay_attachment())
            {
                return Err(CodecError::Unsupported(
                    if self.is_messages() {
                        "Messages replay attachment"
                    } else if self.client.protocol() == ClientProtocol::Chat {
                        "Chat replay attachment"
                    } else {
                        "stateless Responses replay attachment"
                    }
                    .into(),
                )
                .into());
            }
            frames.extend(self.encode(attempt, std::slice::from_ref(&terminal))?);
            self.encoder
                .as_ref()
                .ok_or(AttemptError::Delivery("missing stream encoder"))?
                .finish()?;
            self.phase = ProjectionPhase::Finalized;
            if let Some(coalescer) = &mut self.coalescer {
                coalescer.close();
            }
            Ok(frames)
        })();
        if result.is_err() {
            self.phase = ProjectionPhase::Rejected;
        }
        result
    }
    fn encode(
        &mut self,
        attempt: &Attempt,
        events: &[StreamEvent],
    ) -> Result<Vec<Bytes>, AttemptError> {
        let prepared = self.prepare(attempt, events);
        self.collect_losses();
        self.encode_prepared(prepared?)
    }
    fn prepare(
        &mut self,
        attempt: &Attempt,
        events: &[StreamEvent],
    ) -> Result<PreparedEvents, AttemptError> {
        self.bind(attempt)?;
        let barrier = events.iter().any(|event| {
            !matches!(
                event, StreamEvent::Delta { fragment, logprobs, .. }
                    if !fragment.is_empty() && logprobs.is_empty()
            )
        });
        let metadata = attempt
            .metadata()
            .ok_or(AttemptError::Protocol("missing response metadata"))?;
        let fidelity = attempt
            .fidelity()
            .ok_or(AttemptError::Protocol("missing response fidelity"))?;
        if self.prepared_source.as_ref().is_none_or(|source| {
            source.metadata != *metadata || source.fidelity.as_ref() != fidelity
        }) {
            let fidelity = Arc::new(fidelity.clone());
            self.prepared_source = Some(PreparedSource {
                metadata: metadata.clone(),
                fidelity: fidelity.clone(),
                context: Arc::new(EventContext {
                    metadata: self.metadata(attempt)?,
                    fidelity,
                    accounted_bytes: Default::default(),
                    frame_bytes: Default::default(),
                }),
            });
        }
        let mut context = self
            .prepared_source
            .as_ref()
            .expect("preparation context initialized")
            .context
            .clone();
        if self.encoder.is_none() {
            let padding = self
                .padding
                .take()
                .ok_or(AttemptError::Delivery("padding consumed"))?;
            self.encoder = Some(self.client.stream_encoder(
                context.metadata.clone(),
                self.limits,
                self.options.clone(),
                padding,
            )?);
        }
        let managed = self.managed_responses();
        let encoder = self.encoder.as_mut().expect("encoder initialized");
        if !managed
            && events.iter().any(|event| match event {
                StreamEvent::PartReplay { .. } => true,
                StreamEvent::ItemStarted {
                    kind: crate::semantic::task::generation::ItemKind::ToolCall { .. },
                    replay: Some(_),
                    ..
                } => true,
                StreamEvent::ItemFinished {
                    item,
                    replay: Some(_),
                    ..
                } => context.fidelity.is_call_replay(*item),
                _ => false,
            })
        {
            return Err(CodecError::Unsupported(
                if self.client.protocol() == ClientProtocol::Messages {
                    "Messages replay attachment"
                } else if self.client.protocol() == ClientProtocol::Chat {
                    "Chat replay attachment"
                } else {
                    "stateless Responses replay attachment"
                }
                .into(),
            )
            .into());
        }
        let wire_events: Vec<_> = events
            .iter()
            .filter_map(|event| {
                crate::protocol::replay_projection::wire_event(event, &context.fidelity)
            })
            .collect();
        let projected = if attempt.is_native() {
            encoder.project_native(&wire_events)?
        } else if attempt.is_responses() {
            encoder.project_phase(&wire_events)?
        } else {
            None
        };
        let events = projected.as_deref().unwrap_or(&wire_events);
        if self.client.protocol() != ClientProtocol::Messages
            && (self.client.protocol() == ClientProtocol::Responses || attempt.is_native())
        {
            for event in events {
                if let StreamEvent::ItemStarted { item, .. } = event
                    && context.fidelity.response_item_id(*item).is_none()
                {
                    let id = delivery_item_id(
                        &self.public_label,
                        context.metadata.required_id()?,
                        *item,
                    );
                    let context = Arc::make_mut(&mut context);
                    context.accounted_bytes = Default::default();
                    Arc::make_mut(&mut context.fidelity).record_response_item_id(*item, &id)?;
                }
            }
        }
        self.prepared_source
            .as_mut()
            .expect("preparation context initialized")
            .context = context.clone();
        let events = events
            .iter()
            .cloned()
            .map(|event| {
                super::replay::seal_event(event, &super::replay::CLIENT_ORIGIN, &context.fidelity)
            })
            .collect::<Result<_, _>>()?;
        Ok(PreparedEvents {
            events,
            context,
            barrier,
        })
    }
    /// Encoding uses only the prepared context; it never reads or reprojects intake.
    pub(crate) fn encode_prepared(
        &mut self,
        prepared: PreparedEvents,
    ) -> Result<Vec<Bytes>, AttemptError> {
        let result = (|| {
            self.ready()?;
            let encoder = self
                .encoder
                .as_mut()
                .ok_or(AttemptError::Delivery("missing stream encoder"))?;
            let mut frames = vec![];
            encoder.update_metadata(prepared.context.metadata.clone())?;
            for event in &prepared.events {
                frames.extend(encoder.encode(event, &prepared.context.fidelity)?);
            }
            self.rendered |= !frames.is_empty();
            Ok(frames)
        })();
        self.collect_losses();
        if result.is_err() {
            self.phase = ProjectionPhase::Rejected;
        }
        result
    }
    fn collect_losses(&mut self) {
        if let Some(encoder) = &self.encoder {
            for loss in encoder.losses() {
                if !self.losses.contains(&loss) {
                    self.losses.push(loss);
                }
            }
        }
    }
    /// End a stream with a classified protocol error, never a fabricated Response
    /// or successful terminal. Chat gets an error chunk without `[DONE]`.
    pub fn fail_stream(&mut self, error: &serde_json::Value) -> Result<Vec<Bytes>, AttemptError> {
        let result = (|| {
            self.ready()?;
            let mut pending = self.flush_pending()?;
            let (frames, terminal) =
                self.client
                    .failure_frames(self.encoder.as_mut(), error, self.limits)?;
            pending.extend(frames);
            if let Some(coalescer) = &mut self.coalescer {
                coalescer.close();
            }
            self.phase = if terminal {
                ProjectionPhase::Finalized
            } else {
                ProjectionPhase::Rejected
            };
            Ok(pending)
        })();
        self.collect_losses();
        if result.is_err() {
            self.phase = ProjectionPhase::Rejected;
        }
        result
    }
    /// Call at the external visibility boundary, not when bytes are merely encoded.
    pub fn commit(&mut self) -> Result<(), AttemptError> {
        if self.phase == ProjectionPhase::Rejected || !self.rendered {
            return Err(AttemptError::Delivery("no deliverable output"));
        }
        Ok(self.lifecycle.commit()?)
    }
    /// Call after successful final downstream delivery; EOF alone is insufficient.
    pub fn complete(&mut self, attempt: &Attempt) -> Result<(), AttemptError> {
        self.bind(attempt)?;
        attempt.response()?;
        if self.phase != ProjectionPhase::Finalized || self.lifecycle.may_retry_or_fallback() {
            return Err(AttemptError::Delivery("delivery not complete"));
        }
        Ok(self.lifecycle.terminal()?)
    }
    pub fn cancel(&mut self) {
        self.phase = ProjectionPhase::Rejected;
        self.encoder = None;
        self.prepared_source = None;
        if let Some(coalescer) = &mut self.coalescer {
            coalescer.close();
        }
        self.padding = None;
        self.source = None;
        let _ = self.lifecycle.terminal();
    }
}

#[cfg(test)]
mod google_messages_tests;
#[cfg(test)]
mod responses_history_tests;
#[cfg(test)]
mod tests;
