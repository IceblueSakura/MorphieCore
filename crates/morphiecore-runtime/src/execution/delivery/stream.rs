//! SSE output state: one encoder, prepared batch context and coalescing.
use super::*;
use crate::{
    adapter::ClientEncoder, execution::report::EncodedBatch, protocol::fidelity::FidelityRecords,
};
use bytes::Bytes;

#[derive(Clone)]
pub(super) struct EventContext {
    pub(super) metadata: ResponseMetadata,
    pub(super) fidelity: Arc<FidelityRecords>,
    pub(super) accounted_bytes: std::sync::OnceLock<Option<usize>>,
    pub(super) frame_bytes: std::sync::OnceLock<usize>,
}

/// Raw intake values let unchanged batches reuse the effective delivery context.
/// Generated aliases must not make every following delta look like a source change.
pub(super) struct PreparedSource {
    pub(super) metadata: ResponseMetadata,
    pub(super) fidelity: Arc<FidelityRecords>,
    pub(super) context: Arc<EventContext>,
}

/// Already projected and sealed. Consume in order on the delivery that prepared it.
pub struct PreparedEvents {
    pub events: Vec<StreamEvent>,
    pub(super) context: Arc<EventContext>,
    /// Preserve barriers even when native projection omits their wire event.
    pub(super) barrier: bool,
    pub(super) evidence: EvidenceDelta,
}

pub(super) struct StreamDelivery {
    pub(super) options: StreamOptions,
    pub(super) prepared_source: Option<PreparedSource>,
    deferred_evidence: EvidenceDelta,
    padding: Option<Obfuscation>,
    encoder: Option<ClientEncoder>,
    coalescer: Option<coalescing::Coalescer>,
}

impl StreamDelivery {
    pub(super) fn new(options: StreamOptions, padding: Obfuscation) -> Self {
        Self {
            options,
            padding: Some(padding),
            encoder: None,
            coalescer: None,
            prepared_source: None,
            deferred_evidence: EvidenceDelta::default(),
        }
    }

    pub(super) fn cancel(&mut self) {
        self.encoder = None;
        self.prepared_source = None;
        if let Some(coalescer) = &mut self.coalescer {
            coalescer.close();
        }
        self.padding = None;
    }
}

impl ResponseDelivery {
    pub fn with_coalescing(mut self, policy: Option<DeltaCoalescing>) -> Self {
        if let OutputMode::Sse(stream) = &mut self.output {
            stream.coalescer = policy.map(coalescing::Coalescer::new);
        }
        self
    }
    pub fn has_open_pending(&self) -> bool {
        self.projection.phase == ProjectionPhase::Open && self.flush_deadline().is_some()
    }
    pub fn flush_deadline(&self) -> Option<std::time::Instant> {
        self.output
            .stream()
            .and_then(|stream| stream.coalescer.as_ref())
            .and_then(coalescing::Coalescer::deadline)
    }
    #[cfg(test)]
    pub fn coalescing_peak_bytes(&self) -> usize {
        self.output
            .stream()
            .and_then(|stream| stream.coalescer.as_ref())
            .map_or(0, |c| c.peak_bytes)
    }
    pub fn must_flush_before(&self, attempt: &Attempt, events: &[StreamEvent]) -> bool {
        self.flush_deadline().is_some()
            && (events.iter().any(|event| {
                !matches!(
                    event, StreamEvent::Delta { fragment, logprobs, .. }
                        if !fragment.is_empty() && logprobs.is_empty()
                )
            }) || self
                .output
                .stream()
                .and_then(|stream| stream.prepared_source.as_ref())
                .is_none_or(|source| {
                    attempt.metadata() != Some(&source.metadata)
                        || attempt.fidelity() != Some(source.fidelity.as_ref())
                }))
    }
    pub fn queue_events(
        &mut self,
        attempt: &Attempt,
        events: &[StreamEvent],
        now: std::time::Instant,
    ) -> Result<Vec<PreparedEvents>, AttemptError> {
        let Some(prepared) = self.prepare_events(attempt, events)? else {
            return Ok(vec![]);
        };
        match &mut self.output.stream_mut()?.coalescer {
            Some(coalescer) => Ok(coalescer.push(prepared, now, self.limits.max_event_bytes)),
            None => Ok(vec![prepared]),
        }
    }
    pub fn flush_pending(&mut self) -> Result<EncodedBatch, AttemptError> {
        self.ready()?;
        let OutputMode::Sse(stream) = &mut self.output else {
            return Ok(EncodedBatch::empty());
        };
        match stream
            .coalescer
            .as_mut()
            .and_then(coalescing::Coalescer::flush)
        {
            Some(batch) => self.encode_prepared(batch),
            None => Ok(EncodedBatch::empty()),
        }
    }
    pub fn encode_events(
        &mut self,
        attempt: &Attempt,
        events: &[StreamEvent],
    ) -> Result<EncodedBatch, AttemptError> {
        match self.prepare_events(attempt, events)? {
            Some(prepared) => self.encode_prepared(prepared),
            None => Ok(EncodedBatch::empty()),
        }
    }
    /// Freeze nonterminal projection before intake can advance metadata or fidelity.
    pub fn prepare_events(
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
                    crate::execution::attempt::DeliveryReason::TerminalRequiresValidatedUpstreamEOF,
                ));
            }
            self.prepare(attempt, events).map(Some)
        })();
        if result.is_err() {
            self.projection.phase = ProjectionPhase::Rejected;
        }
        result
    }
    /// Encode the terminal only after the upstream framing/body has closed cleanly.
    pub fn finish_stream(&mut self, attempt: &Attempt) -> Result<EncodedBatch, AttemptError> {
        let result = (|| {
            self.ready()?;
            if self.flush_deadline().is_some() {
                return Err(AttemptError::Delivery(
                    crate::execution::attempt::DeliveryReason::PendingContentNeedsDrain,
                ));
            }
            let terminal = attempt.terminal_event()?.clone();
            let evidence = self.complete_presentation(attempt, View::DownstreamSse)?;
            if self.is_messages() {
                crate::protocol::anthropic::messages::client::validate_tool_calls(
                    attempt.response()?,
                )?;
            }
            if attempt
                .response()?
                .semantic
                .items()
                .iter()
                .any(|(_, item)| item.has_replay_attachment())
            {
                return Err(CodecError::Unsupported(CarrierReason::Replay).into());
            }
            let terminal_events = [terminal];
            let mut prepared = self.prepare(attempt, &terminal_events)?;
            prepared.evidence.extend(evidence);
            let (frames, evidence) = self.encode_prepared_raw(prepared)?;
            self.output
                .stream_mut()?
                .encoder
                .as_ref()
                .ok_or(AttemptError::Delivery(
                    crate::execution::attempt::DeliveryReason::MissingStreamEncoder,
                ))?
                .finish()?;
            self.projection.phase = ProjectionPhase::Finalized;
            if let Some(c) = &mut self.output.stream_mut()?.coalescer {
                c.close();
            }
            Ok(self.checked_output(frames, evidence))
        })();
        if result.is_err() {
            self.projection.phase = ProjectionPhase::Rejected;
        }
        result
    }

    pub(super) fn prepare(
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
        let metadata = attempt.metadata().ok_or(AttemptError::Protocol(
            crate::execution::attempt::ProtocolReason::MissingResponseMetadata,
        ))?;
        let fidelity = attempt.fidelity().ok_or(AttemptError::Protocol(
            crate::execution::attempt::ProtocolReason::MissingResponseFidelity,
        ))?;
        if self
            .output
            .stream_mut()?
            .prepared_source
            .as_ref()
            .is_none_or(|source| {
                source.metadata != *metadata || source.fidelity.as_ref() != fidelity
            })
        {
            let fidelity = Arc::new(fidelity.clone());
            self.output.stream_mut()?.prepared_source = Some(PreparedSource {
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
            .output
            .stream_mut()?
            .prepared_source
            .as_ref()
            .expect("preparation context initialized")
            .context
            .clone();
        self.project_events(attempt, &[], true)?;
        let metadata = self
            .projection
            .adapter
            .as_mut()
            .expect("projection")
            .metadata(&context.metadata)?;
        let stream = self.output.stream_mut()?;
        if stream.encoder.is_none() {
            let padding = stream.padding.take().ok_or(AttemptError::Delivery(
                crate::execution::attempt::DeliveryReason::PaddingConsumed,
            ))?;
            stream.encoder = Some(self.client.stream_encoder(
                metadata,
                self.limits,
                stream.options.clone(),
                padding,
            )?);
        }
        if events.iter().any(|event| match event {
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
        }) {
            return Err(CodecError::Unsupported(CarrierReason::Replay).into());
        }
        let wire_events: Vec<_> = events
            .iter()
            .filter_map(|event| {
                crate::protocol::replay_projection::wire_event(event, &context.fidelity)
            })
            .collect();
        self.projection
            .adapter
            .as_mut()
            .expect("projection")
            .update_fidelity(&context.fidelity)?;
        let sealed = wire_events
            .into_iter()
            .map(|event| {
                super::super::replay::seal_event(
                    event,
                    &crate::protocol::replay_envelope::CLIENT_ORIGIN,
                    &context.fidelity,
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        let projected = self.project_events(attempt, &sealed, true)?;
        self.projection
            .adapter
            .as_mut()
            .expect("projection")
            .update_fidelity(&context.fidelity)?;
        let events = &projected;
        if self.client.protocol() != ClientProtocol::Messages
            && (self.client.protocol() == ClientProtocol::Responses || attempt.is_native())
        {
            for event in events {
                if let StreamEvent::ItemStarted { item, kind, .. } = event
                    && context.fidelity.response_item_id(*item).is_none()
                {
                    let id = delivery_item_id(
                        &self.envelope.public_label,
                        context.metadata.required_id()?,
                        *item,
                        if matches!(kind, ItemKind::Message { .. }) {
                            "msg"
                        } else {
                            "item"
                        },
                    );
                    let context = Arc::make_mut(&mut context);
                    context.accounted_bytes = Default::default();
                    Arc::make_mut(&mut context.fidelity).record_response_item_id(*item, &id)?;
                }
            }
        }
        self.projection
            .adapter
            .as_mut()
            .expect("projection")
            .update_fidelity(&context.fidelity)?;
        let events = events.clone();
        let mut evidence = self
            .output
            .stream_mut()?
            .encoder
            .as_mut()
            .expect("encoder initialized")
            .take_evidence();
        evidence.extend(
            self.projection
                .adapter
                .as_mut()
                .expect("projection")
                .take_evidence(),
        );
        let metadata = self
            .projection
            .adapter
            .as_mut()
            .expect("projection")
            .metadata(&context.metadata)?;
        if context.metadata != metadata {
            let updated = Arc::make_mut(&mut context);
            updated.metadata = metadata;
            updated.accounted_bytes = Default::default();
            updated.frame_bytes = Default::default();
        }
        self.output
            .stream_mut()?
            .prepared_source
            .as_mut()
            .expect("preparation context initialized")
            .context = context.clone();
        Ok(PreparedEvents {
            events,
            context,
            barrier,
            evidence,
        })
    }
    /// Encoding uses only the prepared context; it never reads or reprojects intake.
    pub fn encode_prepared(
        &mut self,
        prepared: PreparedEvents,
    ) -> Result<EncodedBatch, AttemptError> {
        let (frames, evidence) = self.encode_prepared_raw(prepared)?;
        Ok(self.checked_output(frames, evidence))
    }
    fn encode_prepared_raw(
        &mut self,
        prepared: PreparedEvents,
    ) -> Result<(Vec<Bytes>, EvidenceDelta), AttemptError> {
        let result = (|| {
            self.ready()?;
            let stream = self.output.stream_mut()?;
            let encoder = stream.encoder.as_mut().ok_or(AttemptError::Delivery(
                crate::execution::attempt::DeliveryReason::MissingStreamEncoder,
            ))?;
            let mut evidence = prepared.evidence;
            encoder.update_metadata(prepared.context.metadata.clone())?;
            evidence.extend(encoder.take_evidence());
            if prepared
                .events
                .iter()
                .any(|e| matches!(e, StreamEvent::Terminal { .. }))
            {
                let mut current = evidence.clone();
                current.extend(stream.deferred_evidence.clone());
                encoder.set_notice(
                    self.report
                        .preview_notice(View::DownstreamSse, &current)
                        .unwrap_or_default(),
                );
            }
            let mut frames = Vec::new();
            for event in &prepared.events {
                frames.extend(encoder.encode(event, &prepared.context.fidelity)?);
            }
            evidence.extend(encoder.take_evidence());
            if self.client.protocol() == ClientProtocol::Chat && !stream.options.usage() {
                evidence.retain(|effect| {
                    !effect.semantic_field().is_usage()
                        || effect.change() == crate::protocol::conversion::generation::Change::Omit
                });
                if prepared
                    .events
                    .iter()
                    .any(|e| matches!(e, StreamEvent::Usage(_) | StreamEvent::UsageReport(_)))
                {
                    evidence.push(Effect::new(
                        "chat.omit-unrequested-usage.v1",
                        crate::protocol::conversion::generation::Field::UsageReports,
                        crate::protocol::conversion::generation::ConversionBoundary::EventEncode,
                        crate::protocol::conversion::generation::Change::Omit,
                    ));
                }
            }
            self.projection.rendered |= !frames.is_empty();
            Ok((frames, evidence))
        })();
        if result.is_err() {
            self.projection.phase = ProjectionPhase::Rejected;
            if let Ok(stream) = self.output.stream_mut()
                && let Some(e) = &mut stream.encoder
            {
                let _ = e.take_evidence();
            }
        }
        result
    }
    fn checked_output(&mut self, frames: Vec<Bytes>, mut evidence: EvidenceDelta) -> EncodedBatch {
        let stream = self
            .output
            .stream_mut()
            .expect("SSE batch belongs to stream mode");
        evidence.extend(std::mem::take(&mut stream.deferred_evidence));
        if frames.is_empty() {
            let (completed, deferred) = evidence.partition_omissions();
            stream.deferred_evidence = deferred;
            if completed.is_empty() {
                EncodedBatch::empty()
            } else {
                self.report.batch(View::DownstreamSse, frames, completed)
            }
        } else {
            self.report.batch(View::DownstreamSse, frames, evidence)
        }
    }

    /// End a stream with a classified protocol error, never a fabricated Response
    /// or successful terminal. Chat gets an error chunk without `[DONE]`.
    pub fn fail_stream(&mut self, error: &serde_json::Value) -> Result<EncodedBatch, AttemptError> {
        let result = (|| {
            self.ready()?;
            if self.flush_deadline().is_some() {
                return Err(AttemptError::Delivery(
                    crate::execution::attempt::DeliveryReason::PendingContentNeedsDrain,
                ));
            }
            let notice = self.report.notice(View::ErrorDelivery);
            let stream = self.output.stream_mut()?;
            let frames = self.client.failure_frames(
                stream.encoder.as_mut(),
                error,
                self.limits,
                notice.as_ref(),
            )?;
            if let Some(c) = &mut stream.coalescer {
                c.close();
            }
            self.projection.phase = ProjectionPhase::Rejected;
            Ok(self
                .report
                .batch(View::ErrorDelivery, frames, EvidenceDelta::default()))
        })();
        if result.is_err() {
            self.projection.phase = ProjectionPhase::Rejected;
        }
        result
    }
}
