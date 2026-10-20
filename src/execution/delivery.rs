//! Incremental projection and explicit I/O-owned commit. Encoding is not delivery.
mod coalescing;
use crate::{
    adapter::{ClientAdapter, ClientEncoder, ClientProtocol},
    execution::{Attempt, AttemptError, Lifecycle},
    protocol::{
        ResponseMetadata, fidelity::FidelityRecords, openai::stream::Obfuscation, stream::SseLimits,
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

pub struct ResponseDelivery {
    losses: Vec<crate::lowering::generation::LossEvidence>,
    client: ClientAdapter,
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
    terminal: bool,
    rejected: bool,
    native_created: Option<serde_json::Number>,
    /// Request-local envelope identity, never stored as upstream metadata or history.
    generated_id: Option<String>,
    storage: Option<(String, crate::protocol::ResponseStorage)>,
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
            terminal: false,
            rejected: false,
            native_created: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .ok()
                .map(|time| time.as_secs().into()),
            generated_id: None,
            storage: None,
        }
    }
    pub fn is_messages(&self) -> bool {
        self.client.protocol() == ClientProtocol::Messages
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
    pub fn lifecycle(&self) -> &Lifecycle {
        &self.lifecycle
    }
    // Wire IDs can repeat across attempts; only the execution-owned token binds
    // this delivery. A different attempt needs a fresh uncommitted delivery.
    fn bind(&mut self, attempt: &Attempt) -> Result<(), AttemptError> {
        if self
            .source
            .as_ref()
            .is_some_and(|source| !std::sync::Arc::ptr_eq(source, attempt.identity()))
        {
            self.rejected = true;
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
        if let Some((id, storage)) = &self.storage {
            metadata.id = Some(id.clone());
            metadata.storage = Some(storage.clone());
        }
        if metadata.id.is_none() {
            metadata.id = Some(
                self.generated_id
                    .clone()
                    .ok_or(AttemptError::Delivery("missing delivery identity"))?,
            );
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
        if self.rejected || self.terminal {
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
        let losses = if attempt.is_native() {
            self.client.project_native(&mut decoded)?
        } else {
            vec![]
        };
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
        if self.rejected
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
    /// Finalize JSON delivery and retain the exact projection for stored history.
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
            self.terminal = true;
            Ok(snapshot)
        })();
        if result.is_err() {
            self.rejected = true;
        }
        result
    }
    /// The same final representation serves creation, retrieval and stored replay.
    pub(crate) fn snapshot(
        &mut self,
        attempt: &Attempt,
    ) -> Result<(crate::protocol::DecodedResponse, Vec<u8>), AttemptError> {
        let response = self.client_response(attempt)?;
        let (decoded, value, losses) = self.client.snapshot_with_evidence(response)?;
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
            self.rejected = true;
        }
        result
    }
    /// Encode the terminal only after the upstream framing/body has closed cleanly.
    pub fn finish_stream(&mut self, attempt: &Attempt) -> Result<Vec<Bytes>, AttemptError> {
        let result = (|| {
            self.ready()?;
            let terminal = attempt.terminal_event()?;
            if self.is_messages() {
                crate::protocol::anthropic::messages::client::validate_tool_calls(
                    attempt.response()?,
                )?;
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
            // Replay and phase return use the existing EOF-validated materialization.
            if self.client.needs_attachment(&attempt.response()?.semantic)
                // The streamed events already recorded their own projection losses.
                && let Some(packet) = crate::protocol::attachments::Packet::from_response(
                    &self.client_view(attempt)?.0,
                )?
            {
                self.encoder
                    .as_mut()
                    .ok_or(AttemptError::Delivery("missing stream encoder"))?
                    .set_attachment(packet.encode()?)?;
            }
            frames.extend(self.encode(attempt, std::slice::from_ref(terminal))?);
            self.encoder
                .as_ref()
                .ok_or(AttemptError::Delivery("missing stream encoder"))?
                .finish()?;
            self.terminal = true;
            if let Some(coalescer) = &mut self.coalescer {
                coalescer.close();
            }
            Ok(frames)
        })();
        if result.is_err() {
            self.rejected = true;
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
        let encoder = self.encoder.as_mut().expect("encoder initialized");
        if events.iter().any(|event| {
            matches!(
                event,
                StreamEvent::ItemStarted {
                    kind: crate::semantic::task::generation::ItemKind::Message { phase: Some(_) },
                    ..
                }
            )
        }) {
            self.client.check_phase_carrier()?;
        }
        let wire_events: Vec<_> = events
            .iter()
            .filter_map(|event| crate::protocol::attachments::wire_event(event, &context.fidelity))
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
            self.rejected = true;
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
    /// End a stream with a classified failure (`{"error":{...}}`), never a success
    /// terminal. Responses gets `response.failed`, opening the response when nothing
    /// was sent, since Codex reads `context_length_exceeded` and `insufficient_quota`
    /// only there; Chat gets an error chunk and no `[DONE]`, as OpenAI sends.
    pub fn fail_stream(&mut self, error: &serde_json::Value) -> Result<Vec<Bytes>, AttemptError> {
        let result = (|| {
            self.ready()?;
            let mut pending = self.flush_pending()?;
            let unopened =
                if self.client.protocol() == ClientProtocol::Responses && self.encoder.is_none() {
                    Some(ResponseMetadata {
                        storage: None,
                        id: Some(response_id()?),
                        model: self.public_label.clone(),
                        created: self.native_created.clone(),
                        context: Default::default(),
                        instruction_fidelity: Default::default(),
                    })
                } else {
                    None
                };
            let (frames, terminal) =
                self.client
                    .failure_frames(self.encoder.as_mut(), error, unopened, self.limits)?;
            pending.extend(frames);
            if let Some(coalescer) = &mut self.coalescer {
                coalescer.close();
            }
            self.terminal = terminal;
            if !terminal {
                self.rejected = true;
            }
            Ok(pending)
        })();
        self.collect_losses();
        if result.is_err() {
            self.rejected = true;
        }
        result
    }
    /// Call at the external visibility boundary, not when bytes are merely encoded.
    pub fn commit(&mut self) -> Result<(), AttemptError> {
        if self.rejected || !self.rendered {
            return Err(AttemptError::Delivery("no deliverable output"));
        }
        Ok(self.lifecycle.commit()?)
    }
    /// Call after successful final downstream delivery; EOF alone is insufficient.
    pub fn complete(&mut self, attempt: &Attempt) -> Result<(), AttemptError> {
        self.bind(attempt)?;
        attempt.response()?;
        if self.rejected || !self.terminal || self.lifecycle.may_retry_or_fallback() {
            return Err(AttemptError::Delivery("delivery not complete"));
        }
        Ok(self.lifecycle.terminal()?)
    }
    pub fn cancel(&mut self) {
        self.rejected = true;
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
mod tests;
