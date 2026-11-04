//! Client codec dispatch. Publication, attempt identity and entropy belong to execution.
use crate::{
    protocol::{
        ClientProtocol, CodecError, DecodedResponse, GenerationRequestEnvelope as Request,
        ResponseMetadata,
        conversion::generation::Effect,
        failure::CarrierReason,
        fidelity::FidelityRecords,
        openai::{
            GenerationApi, GenerationCodec, TargetContract, chat::sse::ChatSseEncoder,
            obfuscation::Obfuscation, responses::sse::ResponsesSseEncoder,
        },
        stream::{SseError, SseLimits},
    },
    semantic::{context::StreamOptions, task::generation::StreamEvent, value::ReplayOrigin},
};
use bytes::Bytes;
use serde_json::Value;
pub(crate) mod projection;

#[derive(Clone)]
pub enum ClientAdapter {
    OpenAi {
        adapter: Box<GenerationCodec>,
        contract: Box<TargetContract>,
    },
    Messages,
}
impl ClientAdapter {
    pub fn openai(adapter: GenerationCodec, contract: TargetContract) -> Self {
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
    pub fn as_openai(&self) -> Result<(&GenerationCodec, &TargetContract), CodecError> {
        match self {
            Self::OpenAi { adapter, contract } => Ok((adapter, contract)),
            Self::Messages => Err(CodecError::ProfileMismatch),
        }
    }
    fn omits_readable_reasoning(&self) -> bool {
        matches!(self, Self::OpenAi { adapter, .. }
            if crate::protocol::openai::lowering::readable_reasoning::selected(
                adapter.protocol, &adapter.adaptation.rules))
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
    /// Return the encoded body and its represented history. Hidden continuation
    /// requires the separate execution-authorized managed path.
    pub fn snapshot(
        &self,
        decoded: DecodedResponse,
    ) -> Result<(DecodedResponse, Value), crate::protocol::ConversionFailure> {
        self.snapshot_with_evidence(decoded)
            .map(|(decoded, value, _)| (decoded, value))
    }
    pub(crate) fn snapshot_with_evidence(
        &self,
        decoded: DecodedResponse,
    ) -> Result<(DecodedResponse, Value, Vec<Effect>), crate::protocol::ConversionFailure> {
        let (projected, losses) = self.project_response(&decoded)?;
        let (projected, value) = self.render_projected(projected)?;
        Ok((projected, value, losses))
    }
    pub(crate) fn project_response(
        &self,
        decoded: &DecodedResponse,
    ) -> Result<(DecodedResponse, Vec<Effect>), crate::protocol::ConversionFailure> {
        let hidden = decoded
            .semantic
            .items()
            .iter()
            .any(|(_, item)| item.has_replay_attachment());
        if hidden {
            return Err(CodecError::Unsupported(CarrierReason::Replay).into());
        }
        decoded
            .fidelity
            .check_attachment_bindings(decoded.semantic.items())?;
        let mut wire = crate::protocol::replay_projection::strip(decoded, false)?;
        let native = wire.semantic.progress() != crate::semantic::task::generation::InteractionProgress::Unreported
            || wire.semantic.items().iter().any(|(_, item)|
                matches!(item, crate::semantic::task::generation::Item::ToolCall(c)
                    if !matches!(c.arguments, crate::semantic::task::generation::ToolArguments::Raw(_))));
        let messages: Vec<_> = wire
            .semantic
            .items()
            .iter()
            .filter_map(|(_, item)| match item {
                crate::semantic::task::generation::Item::Message(message) => Some(message),
                _ => None,
            })
            .collect();
        let group = native
            || messages.is_empty()
            || messages.len() > 1
            || (wire.semantic.outcome() == crate::semantic::task::generation::Outcome::Incomplete
                && messages.iter().any(|message| {
                    message.status == crate::semantic::task::generation::ItemLifecycle::Completed
                }))
            || messages.iter().any(|message| {
                message
                    .parts
                    .iter()
                    .filter(|part| {
                        !matches!(
                            part.content,
                            crate::semantic::task::generation::ContentPart::Audio(_)
                        )
                    })
                    .count()
                    > 1
            })
            || (!messages.is_empty()
                && wire.semantic.message_envelopes().is_empty()
                && wire.semantic.items().iter().any(|(_, item)| {
                    matches!(
                        item,
                        crate::semantic::task::generation::Item::ToolCall(_)
                            | crate::semantic::task::generation::Item::CustomCall(_)
                    )
                }));
        let mut projection =
            projection::ClientProjection::new(self.clone(), native, false).with_grouping(group);
        projection.update_fidelity(&wire.fidelity)?;
        wire.metadata = projection.metadata(&wire.metadata)?;
        projection::emit(&wire, |event| projection.project(&[event]).map(|_| ()))?;
        wire.semantic = projection.snapshot()?;
        projection.update_fidelity(&wire.fidelity)?;
        wire.fidelity = projection.fidelity().clone();
        wire.fidelity.retain_owners(wire.semantic.items());
        wire.metadata = projection.snapshot_metadata(&wire.metadata)?;
        let effects = projection.take_evidence().iter().cloned().collect();
        Ok((wire, effects))
    }
    pub(crate) fn render_projected(
        &self,
        decoded: DecodedResponse,
    ) -> Result<(DecodedResponse, Value), crate::protocol::ConversionFailure> {
        let value = match self {
            Self::Messages => crate::protocol::anthropic::messages::client::encode_client_response(
                &decoded,
                &decoded.metadata.model,
            )?,
            Self::OpenAi { adapter, contract } => {
                let target = crate::protocol::openai::ResponseRepresentation {
                    semantic: std::borrow::Cow::Borrowed(&decoded.semantic),
                    metadata: std::borrow::Cow::Borrowed(&decoded.metadata),
                    fidelity: &decoded.fidelity,
                    profile: adapter.protocol,
                    adaptation: adapter.contract(contract).adaptation,
                    projection: vec![],
                    projection_steps: 1,
                };
                match adapter.protocol {
                    GenerationApi::Chat => {
                        crate::protocol::openai::chat::envelope::encode_response(&target)?
                    }
                    GenerationApi::Responses => {
                        crate::protocol::openai::responses::envelope::encode_response(&target)?
                    }
                }
            }
        };
        Ok((decoded, value))
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
            GenerationApi::Chat => (
                "messages",
                vec![output.get("choices")?.get(0)?.get("message")?.clone()],
            ),
            GenerationApi::Responses => ("input", output.get("output")?.as_array()?.clone()),
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
            return Err(CodecError::Unsupported(CarrierReason::Safety).into());
        }
        let encoder = match self {
            Self::Messages => {
                let encoder =
                    crate::protocol::anthropic::messages::client::sse::ClientEventEncoder::new(
                        metadata.required_id()?,
                        &metadata.model,
                    )
                    .with_limits(limits)?;
                let mut client = ClientEncoder::Messages(encoder);
                client.update_metadata(metadata)?;
                return Ok(client);
            }
            Self::OpenAi { adapter, contract } => match adapter.protocol {
                GenerationApi::Chat => ClientEncoder::Chat(ChatSseEncoder::new(
                    metadata,
                    adapter.contract(contract),
                    limits,
                    options,
                    padding,
                )?),
                GenerationApi::Responses => ClientEncoder::Responses(ResponsesSseEncoder::new(
                    metadata,
                    adapter.contract(contract),
                    limits,
                    padding,
                )?),
            },
        };
        Ok(encoder)
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
            return match (encoder, self.protocol()) {
                (ClientEncoder::Messages(e), ClientProtocol::Messages) => Ok(e.fail(error)?),
                (ClientEncoder::Chat(e), ClientProtocol::Chat) => Ok(vec![e.fail(error)?]),
                (ClientEncoder::Responses(e), ClientProtocol::Responses) => {
                    Ok(vec![e.fail(error)?])
                }
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
fn safety_assessment_loss() -> Effect {
    Effect::Field {
        rule: crate::protocol::conversion::generation::RuleId::new(SAFETY_ASSESSMENT_RULE),
        field: ("response.safety_reports").into(),
        boundary: ("event.project").into(),
        change: ("omit").into(),
        owner: crate::protocol::conversion::generation::Owner::Root,
    }
}

pub(crate) enum ClientEncoder {
    Responses(ResponsesSseEncoder),
    Chat(ChatSseEncoder),
    Messages(crate::protocol::anthropic::messages::client::sse::ClientEventEncoder),
}
impl ClientEncoder {
    pub(crate) fn set_notice(&mut self, notice: crate::protocol::diagnostics::AdaptationNotice) {
        match self {
            Self::Chat(e) => e.set_notice(notice),
            Self::Responses(e) => e.set_notice(notice),
            Self::Messages(e) => e.set_notice(notice),
        }
    }

    pub(crate) fn update_metadata(&mut self, metadata: ResponseMetadata) -> Result<(), SseError> {
        match self {
            Self::Chat(e) => e.update_metadata(metadata),
            Self::Responses(e) => e.update_metadata(metadata),
            Self::Messages(e) => {
                if let Err(error) =
                    crate::protocol::anthropic::messages::client::validate_metadata(&metadata)
                {
                    e.reject();
                    return Err(error.into());
                }
                e.update_service_class(*metadata.context.service_class())?;
                e.update_delivery(metadata.delivery.as_deref());
                Ok(())
            }
        }
    }
    pub(crate) fn encode(
        &mut self,
        event: &StreamEvent,
        fidelity: &FidelityRecords,
    ) -> Result<Vec<Bytes>, SseError> {
        match self {
            Self::Chat(e) => e.encode(event, fidelity),
            Self::Responses(e) => e.encode(event, fidelity),
            Self::Messages(e) => Ok(e.encode(event)?),
        }
    }
    pub(crate) fn finish(&self) -> Result<(), SseError> {
        match self {
            Self::Chat(e) => e.finish(),
            Self::Responses(e) => e.finish(),
            Self::Messages(e) => Ok(e.finish()?),
        }
    }
    #[cfg(test)]
    pub(crate) fn effects(&self) -> Vec<Effect> {
        match self {
            Self::Chat(e) => e.projection().to_vec(),
            Self::Responses(e) => e.projection().to_vec(),
            Self::Messages(e) => e.losses().to_vec(),
        }
    }
    pub(crate) fn take_evidence(
        &mut self,
    ) -> crate::protocol::conversion::generation::EvidenceDelta {
        match self {
            Self::Chat(e) => e.take_evidence(),
            Self::Responses(e) => e.take_evidence(),
            Self::Messages(e) => e.take_evidence(),
        }
    }
}
