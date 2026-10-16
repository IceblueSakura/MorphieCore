//! Client codec dispatch. Publication, attempt identity and entropy belong to execution.
use super::{ClientProtocol, Request};
use crate::{
    protocol::{
        CodecError, DecodedResponse, ResponseMetadata,
        fidelity::FidelityRecords,
        openai::{
            Adapter, Profile, TargetCaps, chat::sse::ChatSseEncoder,
            responses::sse::ResponsesSseEncoder, stream::Obfuscation,
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
    pub(crate) fn bind_replay_origin(&mut self, origin: &ReplayOrigin) {
        if let Self::OpenAi { adapter, contract } = self {
            adapter.adaptation.scope = Some(origin.clone());
            contract.replay_origin = Some(origin.clone());
        }
    }
    pub fn decode_client_request(&self, bytes: &[u8]) -> Result<Request, CodecError> {
        match self {
            Self::OpenAi { adapter, .. } => adapter.decode_client_request(bytes),
            Self::Messages => crate::protocol::anthropic::client::decode_client_request(bytes),
        }
    }
    pub(crate) fn project_native(
        &self,
        response: &mut DecodedResponse,
    ) -> Result<Vec<crate::lowering::generation::LossEvidence>, super::AdapterError> {
        if let Self::OpenAi { adapter, .. } = self {
            return Ok(crate::protocol::openai::lowering::native_messages::project(
                response,
                adapter.protocol,
            )?);
        }
        Ok(vec![])
    }
    /// Return exactly the representation encoded, so stored history agrees with delivery.
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
        let mut losses = vec![];
        let value = match self {
            Self::Messages => {
                losses.extend(crate::protocol::anthropic::client::metadata_losses(
                    &decoded.metadata,
                ));
                let usage = decoded.semantic.usage();
                crate::protocol::anthropic::client::project_snapshot(&mut decoded)?;
                if usage != decoded.semantic.usage() {
                    losses.push(crate::protocol::anthropic::client::usage_loss());
                }
                crate::protocol::anthropic::client::encode_client_response(
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
                )?;
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
        use crate::protocol::cache_affinity::{MAX_PROMPT_BYTES, prefixes};
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
        let encoder = match self {
            Self::Messages => Encoder::Messages(
                crate::protocol::anthropic::client_events::ClientEventEncoder::new(
                    metadata.required_id()?,
                    &metadata.model,
                ),
            ),
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
        unopened: Option<ResponseMetadata>,
        limits: SseLimits,
    ) -> Result<(Vec<Bytes>, bool), SseError> {
        use crate::semantic::{
            task::generation::{ResponseError, StreamTerminal, TerminalDetails},
            value::Text,
        };
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
        let field = |key: &str| {
            error["error"][key]
                .as_str()
                .and_then(|v| Text::new(v, "error", 4096).ok())
        };
        let failed = StreamEvent::Terminal {
            terminal: StreamTerminal::Failed,
            details: TerminalDetails {
                error: Some(ResponseError {
                    code: field("code"),
                    message: field("message").ok_or(CodecError::Invalid("failure message"))?,
                    param: None,
                }),
                incomplete: None,
                completion: None,
            },
        };
        let source = FidelityRecords::default();
        let frames = if let Some(encoder) = encoder {
            encoder.encode(&failed, &source)?
        } else {
            let mut encoder = self.stream_encoder(
                unopened.ok_or(CodecError::Invalid("failure metadata"))?,
                limits,
                StreamOptions::default(),
                Obfuscation::Disabled,
            )?;
            let mut frames = encoder.encode(&StreamEvent::Started, &source)?;
            frames.extend(encoder.encode(&failed, &source)?);
            frames
        };
        Ok((frames, true))
    }
}

enum Encoder {
    Responses(ResponsesSseEncoder),
    Chat(ChatSseEncoder),
    Messages(crate::protocol::anthropic::client_events::ClientEventEncoder),
}
pub(crate) struct ClientEncoder {
    losses: Vec<crate::lowering::generation::LossEvidence>,
    encoder: Encoder,
    native_projection: Option<crate::protocol::openai::lowering::native_messages::EventProjection>,
}
impl ClientEncoder {
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
            crate::protocol::openai::lowering::native_messages::EventProjection::new(profile)
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
            Encoder::Messages(_) => {
                metadata.context.validate().map_err(CodecError::from)?;
                if metadata.context.execution != crate::semantic::context::ExecutionHints::default()
                {
                    return Err(CodecError::Unsupported(
                        "Messages reported execution context".into(),
                    )
                    .into());
                }
                for mut loss in crate::protocol::anthropic::client::metadata_losses(&metadata) {
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
