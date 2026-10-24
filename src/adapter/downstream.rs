//! Client codec dispatch. Publication, attempt identity and entropy belong to execution.
use super::{ClientProtocol, Request};
use crate::{
    lowering::generation::LossEvidence,
    protocol::{
        CodecError, DecodedResponse, ResponseMetadata,
        fidelity::FidelityRecords,
        openai::{
            Adapter, Profile, TargetCaps, chat::sse::ChatSseEncoder, obfuscation::Obfuscation,
            responses::sse::ResponsesSseEncoder,
        },
        stream::{SseError, SseLimits},
    },
    semantic::{context::StreamOptions, task::generation::StreamEvent, value::ReplayOrigin},
};
use bytes::Bytes;
use serde_json::Value;

#[derive(Clone)]
pub enum ClientAdapter {
    OpenAi {
        adapter: Box<Adapter>,
        contract: Box<TargetCaps>,
    },
    Messages,
}
impl ClientAdapter {
    pub fn openai(adapter: Adapter, contract: TargetCaps) -> Self {
        Self::OpenAi {
            adapter: Box::new(adapter),
            contract: Box::new(contract),
        }
    }
    pub fn protocol(&self) -> ClientProtocol {
        match self {
            Self::OpenAi { adapter, .. } => adapter.protocol.into(),
            Self::Messages => ClientProtocol::Messages,
        }
    }
    pub fn as_openai(&self) -> Result<(&Adapter, &TargetCaps), CodecError> {
        match self {
            Self::OpenAi { adapter, contract } => Ok((adapter, contract)),
            Self::Messages => Err(CodecError::ProfileMismatch),
        }
    }
    pub(crate) fn needs_attachment(
        &self,
        response: &crate::semantic::task::generation::GenerationResponse,
    ) -> bool {
        response
            .items()
            .iter()
            .any(|(_, item)| item.has_replay_attachment())
            || self.protocol() != ClientProtocol::Responses
                && crate::protocol::phase::has_phase(response.items())
    }
    fn phase_carrier(&self) -> bool {
        match self {
            Self::Messages => true,
            Self::OpenAi { adapter, contract } => {
                adapter.protocol == Profile::Chat
                    && adapter
                        .contract(contract)
                        .adaptation
                        .rules
                        .structured_chat_reasoning
            }
        }
    }
    pub(crate) fn check_phase_carrier(&self) -> Result<(), CodecError> {
        if self.protocol() != ClientProtocol::Responses && !self.phase_carrier() {
            return Err(CodecError::Unsupported(
                "message phase return carrier".into(),
            ));
        }
        Ok(())
    }
    /// Speculative streaming grouping must not admit an unphased source that the
    /// original Chat representation rejects. No wire body or second stream is built.
    pub(crate) fn check_chat_source(
        &self,
        decoded: &DecodedResponse,
    ) -> Result<(), super::AdapterError> {
        let (adapter, contract) = self.as_openai()?;
        crate::protocol::openai::lowering::lower_response(
            &decoded.semantic,
            &decoded.fidelity,
            &decoded.metadata,
            Profile::Chat,
            adapter.contract(contract),
        )?;
        Ok(())
    }
    pub(crate) fn bind_replay_origin(&mut self, origin: &ReplayOrigin) {
        if let Self::OpenAi { adapter, contract } = self {
            adapter.adaptation.scope = Some(origin.clone());
            contract.replay_origin = Some(origin.clone());
        }
    }
    pub fn decode_client_request(&self, bytes: &[u8]) -> Result<Request, CodecError> {
        match self {
            Self::OpenAi { adapter, .. } => adapter.decode_client_request(bytes),
            Self::Messages => {
                crate::protocol::anthropic::messages::client::decode_client_request(bytes)
            }
        }
    }
    pub(crate) fn project_native(
        &self,
        response: &mut DecodedResponse,
    ) -> Result<Vec<crate::lowering::generation::LossEvidence>, super::AdapterError> {
        if let Self::OpenAi { adapter, .. } = self {
            return Ok(crate::protocol::openai::lowering::native_response::project(
                response,
                adapter.protocol,
            )?);
        }
        Ok(vec![])
    }
    /// Return the encoded body and the history it represents, so stored history agrees
    /// with delivery. With a replay carrier, that history is what the carrier restores.
    pub fn snapshot(
        &self,
        decoded: DecodedResponse,
    ) -> Result<(DecodedResponse, Value), super::AdapterError> {
        self.snapshot_with_evidence(decoded)
            .map(|(decoded, value, _)| (decoded, value))
    }
    pub(crate) fn snapshot_with_evidence(
        &self,
        mut decoded: DecodedResponse,
    ) -> Result<
        (
            DecodedResponse,
            Value,
            Vec<crate::lowering::generation::LossEvidence>,
        ),
        super::AdapterError,
    > {
        if !self.needs_attachment(&decoded.semantic) {
            return self.encode_snapshot(decoded, None);
        }
        let packet = crate::protocol::attachments::Packet::from_response(&decoded)?
            .ok_or(CodecError::Invalid("missing attachment manifest"))?;
        if let Self::OpenAi { adapter, contract } = self {
            crate::protocol::attachments::check_chat_support(
                adapter.protocol,
                adapter
                    .contract(contract)
                    .adaptation
                    .rules
                    .structured_chat_reasoning,
            )?;
        }
        let phase = self.protocol() != ClientProtocol::Responses
            && crate::protocol::phase::has_phase(decoded.semantic.items());
        let mut wire = crate::protocol::attachments::strip(&decoded, phase)?;
        let mut losses = self.project_native(&mut wire)?;
        let (projected, value, extra) = self.encode_snapshot(wire, Some(packet.encode()?))?;
        if phase {
            losses.push(crate::protocol::phase::carried("response.encode"));
        }
        losses.extend(extra);
        // Stored history keeps the real owners and original groups that the returned
        // carrier restores, although the visible copy still records its grouping loss.
        // Item content is unchanged by these client projections, so only reports,
        // terminal details and metadata come from the encoded copy.
        decoded.semantic = decoded
            .semantic
            .with_usage_reports(projected.semantic.usage_reports().clone())
            .map_err(CodecError::from)?
            .with_details(projected.semantic.details().clone())
            .map_err(CodecError::from)?
            .with_progress(projected.semantic.progress())
            .map_err(CodecError::from)?;
        decoded.metadata = projected.metadata;
        Ok((decoded, value, losses))
    }
    fn encode_snapshot(
        &self,
        mut decoded: DecodedResponse,
        attachment: Option<crate::protocol::attachments::Carrier>,
    ) -> Result<
        (
            DecodedResponse,
            Value,
            Vec<crate::lowering::generation::LossEvidence>,
        ),
        super::AdapterError,
    > {
        let mut losses = vec![];
        if decoded
            .semantic
            .safety_reports()
            .iter()
            .any(|report| report.is_assessment_only())
        {
            let blocking = decoded
                .semantic
                .safety_reports()
                .iter()
                .filter(|report| !report.is_assessment_only())
                .cloned()
                .collect();
            decoded.semantic = decoded
                .semantic
                .with_safety_reports(blocking)
                .map_err(CodecError::from)?;
            losses.push(safety_assessment_loss("response.encode"));
        }
        let value = match self {
            Self::Messages => {
                losses.extend(
                    crate::protocol::anthropic::messages::client::metadata_losses(
                        &decoded.metadata,
                    ),
                );
                losses.extend(
                    crate::protocol::anthropic::messages::client::content_losses(&decoded.semantic),
                );
                let usage = decoded.semantic.usage();
                crate::protocol::anthropic::messages::client::project_snapshot(&mut decoded)?;
                if usage != decoded.semantic.usage() {
                    losses.push(crate::protocol::anthropic::messages::client::usage_loss());
                }
                crate::protocol::anthropic::messages::client::encode_client_response_with(
                    &decoded,
                    &decoded.metadata.model,
                    attachment.as_ref(),
                )?
            }
            Self::OpenAi { adapter, contract } => {
                let mut target = crate::protocol::openai::lowering::lower_response(
                    &decoded.semantic,
                    &decoded.fidelity,
                    &decoded.metadata,
                    adapter.protocol,
                    adapter.contract(contract),
                )?;
                target.attachment = attachment;
                let value = match adapter.protocol {
                    Profile::Chat => {
                        crate::protocol::openai::chat::envelope::encode_response(&target)?
                    }
                    Profile::Responses => {
                        crate::protocol::openai::responses::envelope::encode_response(&target)?
                    }
                };
                let semantic = target.semantic().clone();
                let metadata = target.metadata().clone();
                losses.extend(
                    target
                        .projection()
                        .iter()
                        .filter_map(|stage| stage.evidence()),
                );
                decoded.semantic = semantic;
                decoded.metadata = metadata;
                value
            }
        };
        Ok((decoded, value, losses))
    }
    /// Protocol-shaped cache continuation, not conversation storage or target selection.
    pub(crate) fn cache_successor(
        &self,
        request: &Request,
        decoded: &DecodedResponse,
    ) -> Option<Request> {
        use crate::protocol::openai::cache_affinity::{MAX_PROMPT_BYTES, prefixes};
        let (adapter, contract) = self.as_openai().ok()?;
        let mut input = adapter
            .encode_request(request, &decoded.metadata.model, contract)
            .ok()?;
        prefixes(&input, adapter.protocol)?;
        let output = adapter.encode_response(decoded, contract).ok()?;
        crate::semantic::value::json_size(&output, MAX_PROMPT_BYTES).ok()?;
        let (field, items) = match adapter.protocol {
            Profile::Chat => (
                "messages",
                vec![output.get("choices")?.get(0)?.get("message")?.clone()],
            ),
            Profile::Responses => ("input", output.get("output")?.as_array()?.clone()),
        };
        input.get_mut(field)?.as_array_mut()?.extend(items);
        prefixes(&input, adapter.protocol)?;
        crate::semantic::value::json_size(&input, MAX_PROMPT_BYTES).ok()?;
        adapter
            .decode_request(&serde_json::to_vec(&input).ok()?)
            .ok()
    }
    pub(crate) fn stream_encoder(
        &self,
        metadata: ResponseMetadata,
        limits: SseLimits,
        options: StreamOptions,
        padding: Obfuscation,
    ) -> Result<ClientEncoder, SseError> {
        if metadata
            .context
            .reports
            .iter()
            .any(|r| r.settings.as_ref().is_some_and(|s| !s.safety.is_empty()))
        {
            return Err(CodecError::Unsupported("reported safety settings".into()).into());
        }
        let encoder = match self {
            Self::Messages => {
                let mut encoder =
                    crate::protocol::anthropic::messages::client::sse::ClientEventEncoder::new(
                        metadata.required_id()?,
                        &metadata.model,
                    )
                    .with_limits(limits)?;
                crate::protocol::reports::validate_single(&metadata.context.reports)?;
                encoder.update_service_class(metadata.context.service_class().clone())?;
                encoder.update_delivery(metadata.delivery.as_deref());
                Encoder::Messages(encoder)
            }
            Self::OpenAi { adapter, contract } => match adapter.protocol {
                Profile::Chat => Encoder::Chat(ChatSseEncoder::new(
                    metadata,
                    adapter.contract(contract),
                    limits,
                    options,
                    padding,
                )?),
                Profile::Responses => Encoder::Responses(ResponsesSseEncoder::new(
                    metadata,
                    adapter.contract(contract),
                    limits,
                    padding,
                )?),
            },
        };
        Ok(ClientEncoder {
            encoder,
            native_projection: None,
            losses: vec![],
            phase_carrier: self.phase_carrier(),
        })
    }
    /// Whether an error frame closes this protocol's stream is a carrier property.
    pub(crate) fn failure_frames(
        &self,
        encoder: Option<&mut ClientEncoder>,
        error: &Value,
        limits: SseLimits,
    ) -> Result<(Vec<Bytes>, bool), SseError> {
        match self.protocol() {
            ClientProtocol::Messages => {
                return Ok((
                    vec![Bytes::from(format!("event: error\ndata: {error}\n\n"))],
                    true,
                ));
            }
            ClientProtocol::Chat => {
                return Ok((vec![Bytes::from(format!("data: {error}\n\n"))], false));
            }
            ClientProtocol::Responses => {}
        }
        let frame = if let Some(ClientEncoder {
            encoder: Encoder::Responses(encoder),
            ..
        }) = encoder
        {
            encoder.fail(error)?
        } else {
            limits.validate()?;
            let frame = crate::protocol::openai::responses::sse::error_frame(
                error,
                0,
                limits.max_event_bytes,
            )?;
            if frame.len() > limits.max_wire_bytes {
                return Err(SseError::Limit);
            }
            frame
        };
        Ok((vec![frame], true))
    }
}

/// Client carriers have no safety assessment slot. Assessment-only reports are
/// omitted under a named rule; blocking reports remain protected and reject.
pub(crate) const SAFETY_ASSESSMENT_RULE: &str = "generation.omit-safety-assessment.v1";
fn safety_assessment_loss(direction: &'static str) -> LossEvidence {
    LossEvidence {
        rule: SAFETY_ASSESSMENT_RULE,
        field: "response.safety_reports",
        direction,
        action: "omit",
    }
}

enum Encoder {
    Responses(ResponsesSseEncoder),
    Chat(ChatSseEncoder),
    Messages(crate::protocol::anthropic::messages::client::sse::ClientEventEncoder),
}
pub(crate) struct ClientEncoder {
    losses: Vec<crate::lowering::generation::LossEvidence>,
    phase_carrier: bool,
    encoder: Encoder,
    native_projection: Option<crate::protocol::openai::lowering::native_response::EventProjection>,
}
impl ClientEncoder {
    /// Track Chat grouping from the first item, including an unlabelled prefix.
    /// Absence of phase is decided at EOF, before a successful terminal.
    pub(crate) fn project_phase(
        &mut self,
        events: &[StreamEvent],
    ) -> Result<Option<Vec<StreamEvent>>, super::AdapterError> {
        if !self.phase_carrier || !matches!(self.encoder, Encoder::Chat(_)) {
            return Ok(None);
        }
        let projection = self.native_projection.get_or_insert_with(
            crate::protocol::openai::lowering::native_response::EventProjection::for_phase,
        );
        let projected = projection.project(events)?;
        for loss in projection.losses() {
            if !self.losses.contains(loss) {
                self.losses.push(loss.clone());
            }
        }
        Ok(Some(projected))
    }
    pub(crate) fn needs_phase_source_check(&self) -> bool {
        self.native_projection
            .as_ref()
            .is_some_and(|p| p.needs_source_check())
    }
    pub(crate) fn set_attachment(
        &mut self,
        value: crate::protocol::attachments::Carrier,
    ) -> Result<(), SseError> {
        match &mut self.encoder {
            Encoder::Chat(encoder) => encoder.set_attachment(value),
            Encoder::Responses(encoder) => encoder.set_attachment(value),
            Encoder::Messages(encoder) => Ok(encoder.set_attachment(value)?),
        }
    }
    pub(crate) fn project_native(
        &mut self,
        events: &[StreamEvent],
    ) -> Result<Option<Vec<StreamEvent>>, super::AdapterError> {
        let profile = match self.encoder {
            Encoder::Chat(_) => Profile::Chat,
            Encoder::Responses(_) => Profile::Responses,
            Encoder::Messages(_) => return Ok(None),
        };
        let projection = self.native_projection.get_or_insert_with(|| {
            crate::protocol::openai::lowering::native_response::EventProjection::new(profile)
        });
        let result = projection.project(events);
        for loss in projection.losses() {
            if !self.losses.contains(loss) {
                self.losses.push(loss.clone());
            }
        }
        Ok(Some(result?))
    }
    pub(crate) fn update_metadata(&mut self, metadata: ResponseMetadata) -> Result<(), SseError> {
        match &mut self.encoder {
            Encoder::Chat(e) => e.update_metadata(metadata),
            Encoder::Responses(e) => e.update_metadata(metadata),
            Encoder::Messages(e) => {
                if let Err(error) = metadata.context.validate() {
                    e.reject();
                    return Err(CodecError::from(error).into());
                }
                if let Err(error) =
                    crate::protocol::reports::validate_single(&metadata.context.reports)
                {
                    e.reject();
                    return Err(error.into());
                }
                if metadata
                    .context
                    .reports
                    .iter()
                    .any(|r| r.settings.as_ref().is_some_and(|s| !s.safety.is_empty()))
                {
                    e.reject();
                    return Err(CodecError::Unsupported("reported safety settings".into()).into());
                }
                if metadata.context.execution != crate::semantic::context::ExecutionHints::default()
                {
                    e.reject();
                    return Err(CodecError::Unsupported(
                        "Messages reported execution context".into(),
                    )
                    .into());
                }
                e.update_service_class(metadata.context.service_class().clone())?;
                e.update_delivery(metadata.delivery.as_deref());
                for mut loss in
                    crate::protocol::anthropic::messages::client::metadata_losses(&metadata)
                {
                    loss.direction = "event.encode";
                    if !self.losses.contains(&loss) {
                        self.losses.push(loss);
                    }
                }
                Ok(())
            }
        }
    }
    pub(crate) fn encode(
        &mut self,
        event: &StreamEvent,
        fidelity: &FidelityRecords,
    ) -> Result<Vec<Bytes>, SseError> {
        if let StreamEvent::ItemStarted {
            item,
            kind: crate::semantic::task::generation::ItemKind::Message { phase: Some(_) },
            replay,
        } = event
            && !matches!(self.encoder, Encoder::Responses(_))
        {
            if !self.phase_carrier {
                return Err(CodecError::Unsupported("message phase return carrier".into()).into());
            }
            let wire = StreamEvent::ItemStarted {
                item: *item,
                kind: crate::semantic::task::generation::ItemKind::Message { phase: None },
                replay: replay.clone(),
            };
            let frames = self.encode(&wire, fidelity)?;
            let loss = crate::protocol::phase::carried("event.encode");
            if !self.losses.contains(&loss) {
                self.losses.push(loss);
            }
            return Ok(frames);
        }
        if let StreamEvent::SafetyReport(report) = event
            && report.is_assessment_only()
        {
            let loss = safety_assessment_loss("event.encode");
            if !self.losses.contains(&loss) {
                self.losses.push(loss);
            }
            return Ok(vec![]);
        }
        match &mut self.encoder {
            Encoder::Chat(e) => e.encode(event, fidelity),
            Encoder::Responses(e) => e.encode(event, fidelity),
            Encoder::Messages(e) => Ok(e.encode(event)?),
        }
    }
    pub(crate) fn finish(&self) -> Result<(), SseError> {
        match &self.encoder {
            Encoder::Chat(e) => e.finish(),
            Encoder::Responses(e) => e.finish(),
            Encoder::Messages(e) => Ok(e.finish()?),
        }
    }
    pub(crate) fn losses(&self) -> Vec<crate::lowering::generation::LossEvidence> {
        let mut losses = self.losses.clone();
        match &self.encoder {
            Encoder::Chat(e) => {
                losses.extend(e.projection().iter().filter_map(|stage| stage.evidence()))
            }
            Encoder::Responses(e) => {
                losses.extend(e.projection().iter().filter_map(|stage| stage.evidence()))
            }
            Encoder::Messages(e) => losses.extend_from_slice(e.losses()),
        }
        losses
    }
}
