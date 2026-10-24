//! Client codec dispatch. Publication, attempt identity and entropy belong to execution.
use super::{ClientProtocol, Request};
use crate::{
    lowering::generation::Effect,
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
    /// Speculative streaming grouping must not admit an unphased source that the
    /// original Chat representation rejects. No wire body or second stream is built.
    pub(crate) fn check_chat_source(
        &self,
        decoded: &DecodedResponse,
    ) -> Result<(), super::ConversionFailure> {
        let result = (|| {
            let (adapter, contract) = self.as_openai()?;
            crate::protocol::openai::lowering::lower_response(
                &decoded.semantic,
                &decoded.fidelity,
                &decoded.metadata,
                Profile::Chat,
                adapter.contract(contract),
            )
            .map_err(|error| {
                crate::adapter::ConversionFailure::from(error)
                    .at(crate::lowering::generation::ConversionBoundary::ResponseProject)
            })?;
            Ok(())
        })();
        result.map_err(|error: crate::adapter::ConversionFailure| {
            error.at(crate::lowering::generation::ConversionBoundary::ResponseProject)
        })
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
    ) -> Result<Vec<crate::lowering::generation::Effect>, super::ConversionFailure> {
        let result = (|| {
            if let Self::OpenAi { adapter, .. } = self {
                return Ok(crate::protocol::openai::lowering::native_response::project(
                    response,
                    adapter.protocol,
                )?);
            }
            Ok(vec![])
        })();
        result.map_err(|error: crate::adapter::ConversionFailure| {
            error.at(crate::lowering::generation::ConversionBoundary::ResponseProject)
        })
    }
    /// Return the encoded body and its represented history. Hidden continuation
    /// requires the separate execution-authorized managed path.
    pub fn snapshot(
        &self,
        decoded: DecodedResponse,
    ) -> Result<(DecodedResponse, Value), super::ConversionFailure> {
        self.snapshot_with_evidence(decoded)
            .map(|(decoded, value, _)| (decoded, value))
    }
    pub(crate) fn snapshot_with_evidence(
        &self,
        decoded: DecodedResponse,
    ) -> Result<(DecodedResponse, Value, Vec<Effect>), super::ConversionFailure> {
        self.snapshot_with_history(decoded, false)
    }
    /// Only execution-owned storage authorizes a richer typed continuation than
    /// the public response. Upstream metadata cannot opt into this path.
    pub(crate) fn snapshot_with_history(
        &self,
        mut decoded: DecodedResponse,
        managed: bool,
    ) -> Result<
        (
            DecodedResponse,
            Value,
            Vec<crate::lowering::generation::Effect>,
        ),
        super::ConversionFailure,
    > {
        let result = (|| {
            let hidden = decoded
                .semantic
                .items()
                .iter()
                .any(|(_, item)| item.has_replay_attachment());
            if self.protocol() != ClientProtocol::Responses {
                if hidden {
                    return Err(CodecError::Unsupported(
                        crate::protocol::failure::CarrierReason::Replay,
                    )
                    .into());
                }
                if crate::protocol::phase::has_phase(decoded.semantic.items()) {
                    let mut wire = crate::protocol::replay_projection::strip(&decoded, true)?;
                    let mut losses = self.project_native(&mut wire)?;
                    let (projected, value, extra) = self.encode_snapshot(wire)?;
                    losses.extend(extra);
                    losses.push(crate::protocol::phase::delivery_omission("response.encode"));
                    return Ok((projected, value, losses));
                }
            }
            if self.protocol() == ClientProtocol::Responses && hidden {
                if !managed {
                    return Err(CodecError::Unsupported(
                        crate::protocol::failure::CarrierReason::Replay,
                    )
                    .into());
                }
                decoded
                    .fidelity
                    .check_attachment_bindings(decoded.semantic.items())?;
                let mut wire = crate::protocol::replay_projection::strip(&decoded, false)?;
                let mut losses = self.project_native(&mut wire)?;
                let (projected, value, extra) = self.encode_snapshot(wire)?;
                losses.extend(extra);
                decoded.semantic = decoded
                    .semantic
                    .with_usage_reports(projected.semantic.usage_reports().clone())
                    .map_err(CodecError::from)?
                    .with_details(projected.semantic.details().clone())
                    .map_err(CodecError::from)?
                    .with_progress(projected.semantic.progress())
                    .map_err(CodecError::from)?
                    .with_safety_reports(projected.semantic.safety_reports().to_vec())
                    .map_err(CodecError::from)?;
                decoded.metadata = projected.metadata;
                return Ok((decoded, value, losses));
            }
            self.encode_snapshot(decoded)
        })();
        result.map_err(|error: crate::adapter::ConversionFailure| {
            error.at(crate::lowering::generation::ConversionBoundary::ResponseEncode)
        })
    }
    fn encode_snapshot(
        &self,
        mut decoded: DecodedResponse,
    ) -> Result<
        (
            DecodedResponse,
            Value,
            Vec<crate::lowering::generation::Effect>,
        ),
        super::ConversionFailure,
    > {
        let result = (|| {
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
                        crate::protocol::anthropic::messages::client::content_losses(
                            &decoded.semantic,
                        ),
                    );
                    let usage = decoded.semantic.usage();
                    crate::protocol::anthropic::messages::client::project_snapshot(&mut decoded)?;
                    if usage != decoded.semantic.usage() {
                        losses.push(crate::protocol::anthropic::messages::client::usage_loss());
                    }
                    crate::protocol::anthropic::messages::client::encode_client_response(
                        &decoded,
                        &decoded.metadata.model,
                    )?
                }
                Self::OpenAi { adapter, contract } => {
                    let target = crate::protocol::openai::lowering::lower_response(
                        &decoded.semantic,
                        &decoded.fidelity,
                        &decoded.metadata,
                        adapter.protocol,
                        adapter.contract(contract),
                    )
                    .map_err(|error| {
                        crate::adapter::ConversionFailure::from(error)
                            .at(crate::lowering::generation::ConversionBoundary::ResponseProject)
                    })?;
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
                    losses.extend(target.projection().iter().cloned());
                    decoded.semantic = semantic;
                    decoded.metadata = metadata;
                    value
                }
            };
            Ok((decoded, value, losses))
        })();
        result.map_err(|error: crate::adapter::ConversionFailure| {
            error.at(crate::lowering::generation::ConversionBoundary::ResponseEncode)
        })
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
            return Err(
                CodecError::Unsupported(crate::protocol::failure::CarrierReason::Safety).into(),
            );
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
        })
    }
    /// Whether an error frame closes this protocol's stream is a carrier property.
    pub(crate) fn failure_frames(
        &self,
        encoder: Option<&mut ClientEncoder>,
        error: &Value,
        limits: SseLimits,
        notice: Option<&crate::protocol::diagnostics::AdaptationNotice>,
    ) -> Result<Vec<Bytes>, SseError> {
        if let Some(encoder) = encoder {
            if let Some(notice) = notice {
                encoder.set_notice(notice.clone());
            }
            return match (&mut encoder.encoder, self.protocol()) {
                (Encoder::Messages(e), ClientProtocol::Messages) => Ok(e.fail(error)?),
                (Encoder::Chat(e), ClientProtocol::Chat) => Ok(vec![e.fail(error)?]),
                (Encoder::Responses(e), ClientProtocol::Responses) => Ok(vec![e.fail(error)?]),
                _ => Err(CodecError::ProfileMismatch.into()),
            };
        }
        limits.validate()?;
        let max = limits.max_event_bytes.min(limits.max_wire_bytes);
        if self.protocol() == ClientProtocol::Responses {
            return Ok(vec![crate::protocol::openai::responses::sse::error_frame(
                error, 0, max, notice,
            )?]);
        }
        let mut value = error.clone();
        let prefix = if self.protocol() == ClientProtocol::Messages {
            "event: error\ndata: "
        } else {
            "data: "
        };
        if let Some(notice) = notice {
            notice.attach(&mut value, max.saturating_sub(prefix.len() + 2), false);
        }
        crate::semantic::value::json_size(&value, max.saturating_sub(prefix.len() + 2))
            .map_err(|_| SseError::Limit)?;
        Ok(vec![Bytes::from(format!("{prefix}{value}\n\n"))])
    }
}

/// Client carriers have no safety assessment slot. Assessment-only reports are
/// omitted under a named rule; blocking reports remain protected and reject.
pub(crate) const SAFETY_ASSESSMENT_RULE: &str = "generation.omit-safety-assessment.v1";
fn safety_assessment_loss(direction: &'static str) -> Effect {
    Effect::Field {
        rule: crate::lowering::generation::RuleId::new(SAFETY_ASSESSMENT_RULE),
        field: ("response.safety_reports").into(),
        direction: (direction).into(),
        action: ("omit").into(),
        owner: crate::lowering::generation::Owner::Root,
    }
}

enum Encoder {
    Responses(ResponsesSseEncoder),
    Chat(ChatSseEncoder),
    Messages(crate::protocol::anthropic::messages::client::sse::ClientEventEncoder),
}
pub(crate) struct ClientEncoder {
    losses: Vec<crate::lowering::generation::Effect>,
    encoder: Encoder,
    native_projection: Option<crate::protocol::openai::lowering::native_response::EventProjection>,
}
impl ClientEncoder {
    pub(crate) fn set_notice(&mut self, notice: crate::protocol::diagnostics::AdaptationNotice) {
        match &mut self.encoder {
            Encoder::Chat(e) => e.set_notice(notice),
            Encoder::Responses(e) => e.set_notice(notice),
            Encoder::Messages(e) => e.set_notice(notice),
        }
    }

    /// Track Chat grouping from the first item, including an unlabelled prefix.
    /// Absence of phase is decided at EOF, before a successful terminal.
    pub(crate) fn project_phase(
        &mut self,
        events: &[StreamEvent],
    ) -> Result<Option<Vec<StreamEvent>>, super::ConversionFailure> {
        let result = (|| {
            if !matches!(self.encoder, Encoder::Chat(_)) {
                return Ok(None);
            }
            let projection = self.native_projection.get_or_insert_with(
                crate::protocol::openai::lowering::native_response::EventProjection::for_phase,
            );
            let projected = projection.project(events)?;
            for loss in projection.take_evidence() {
                if !self.losses.contains(&loss) {
                    self.losses.push(loss);
                }
            }
            Ok(Some(projected))
        })();
        result.map_err(|error: crate::adapter::ConversionFailure| {
            error.at(crate::lowering::generation::ConversionBoundary::EventProject)
        })
    }
    pub(crate) fn needs_phase_source_check(&self) -> bool {
        self.native_projection
            .as_ref()
            .is_some_and(|p| p.needs_source_check())
    }
    pub(crate) fn project_native(
        &mut self,
        events: &[StreamEvent],
    ) -> Result<Option<Vec<StreamEvent>>, super::ConversionFailure> {
        let result = (|| {
            let profile = match self.encoder {
                Encoder::Chat(_) => Profile::Chat,
                Encoder::Responses(_) => Profile::Responses,
                Encoder::Messages(_) => return Ok(None),
            };
            let projection = self.native_projection.get_or_insert_with(|| {
                crate::protocol::openai::lowering::native_response::EventProjection::new(profile)
            });
            let result = projection.project(events)?;
            for loss in projection.take_evidence() {
                if !self.losses.contains(&loss) {
                    self.losses.push(loss);
                }
            }
            Ok(Some(result))
        })();
        result.map_err(|error: crate::adapter::ConversionFailure| {
            error.at(crate::lowering::generation::ConversionBoundary::EventProject)
        })
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
                    return Err(CodecError::Unsupported(
                        crate::protocol::failure::CarrierReason::Safety,
                    )
                    .into());
                }
                if metadata.context.execution != crate::semantic::context::ExecutionHints::default()
                {
                    e.reject();
                    return Err(CodecError::Unsupported(
                        crate::protocol::failure::CarrierReason::Unavailable,
                    )
                    .into());
                }
                e.update_service_class(metadata.context.service_class().clone())?;
                e.update_delivery(metadata.delivery.as_deref());
                for mut loss in
                    crate::protocol::anthropic::messages::client::metadata_losses(&metadata)
                {
                    loss = loss.with_boundary(("event.encode").into());
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
            let wire = StreamEvent::ItemStarted {
                item: *item,
                kind: crate::semantic::task::generation::ItemKind::Message { phase: None },
                replay: replay.clone(),
            };
            let frames = self.encode(&wire, fidelity)?;
            let loss = crate::protocol::phase::delivery_omission("event.encode")
                .at(crate::lowering::generation::Owner::item(*item));
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
    #[cfg(test)]
    pub(crate) fn effects(&self) -> Vec<Effect> {
        let mut effects = self.losses.clone();
        match &self.encoder {
            Encoder::Chat(e) => effects.extend_from_slice(e.projection()),
            Encoder::Responses(e) => effects.extend_from_slice(e.projection()),
            Encoder::Messages(e) => effects.extend_from_slice(e.losses()),
        }
        effects
    }
    pub(crate) fn take_evidence(&mut self) -> crate::lowering::generation::EvidenceDelta {
        let mut evidence: crate::lowering::generation::EvidenceDelta =
            std::mem::take(&mut self.losses).into_iter().collect();
        evidence.extend(match &mut self.encoder {
            Encoder::Chat(e) => e.take_evidence(),
            Encoder::Responses(e) => e.take_evidence(),
            Encoder::Messages(e) => e.take_evidence(),
        });
        evidence
    }
}
