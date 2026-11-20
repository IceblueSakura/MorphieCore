//! Upstream response dispatch, independent of the standard client's OpenAI profile.
use crate::{
    protocol::{
        CodecError, DecodedResponse, ResponseMetadata, anthropic,
        failure::CarrierReason,
        fidelity::FidelityRecords,
        openai::{
            GenerationApi, GenerationCodec, chat::sse::ChatSseDecoder,
            responses::sse::ResponsesSseDecoder,
        },
        stream::{SseError, SseLimits},
    },
    semantic::task::generation::{LocalScope, StreamEvent},
};

const NATIVE_SCOPE: LocalScope = LocalScope::new(2);

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UpstreamAdapter {
    OpenAi(Box<GenerationCodec>),
    /// Native intake labels opaque values with the trusted Provider/model binding.
    AnthropicMessages {
        profile: anthropic::messages::Profile,
        dialect: anthropic::messages::Dialect,
        target: crate::protocol::native::ReplayTarget,
    },
    GoogleGenerateContent {
        target: crate::protocol::native::ReplayTarget,
    },
    GoogleInteractions {
        target: crate::protocol::native::ReplayTarget,
    },
}
impl From<GenerationCodec> for UpstreamAdapter {
    fn from(adapter: GenerationCodec) -> Self {
        Self::OpenAi(Box::new(adapter))
    }
}
impl UpstreamAdapter {
    pub fn openai(&self) -> Result<&GenerationCodec, CodecError> {
        match self {
            Self::OpenAi(adapter) => Ok(adapter),
            Self::AnthropicMessages { .. }
            | Self::GoogleGenerateContent { .. }
            | Self::GoogleInteractions { .. } => Err(CodecError::ProfileMismatch),
        }
    }
    pub fn decode_response(
        &self,
        bytes: &[u8],
    ) -> Result<DecodedResponse, crate::protocol::ConversionFailure> {
        let result = match self {
            Self::OpenAi(adapter) if adapter.protocol == GenerationApi::Chat => {
                adapter.decode_response(bytes)
            }
            _ => Err(CodecError::Unsupported(CarrierReason::Unavailable)),
        };
        result.map_err(|error: CodecError| {
            crate::protocol::ConversionFailure::from(error)
                .at(crate::protocol::conversion::generation::ConversionBoundary::ResponseDecode)
        })
    }
    pub(crate) fn is_native(&self) -> bool {
        !matches!(self, Self::OpenAi(_))
    }
    pub(crate) fn accepts_json(&self) -> bool {
        matches!(self, Self::OpenAi(adapter) if adapter.protocol == GenerationApi::Chat)
    }
    pub(crate) fn stream_decoder(
        &self,
        status: u16,
        content_type: &str,
        limits: SseLimits,
        body_limit: usize,
    ) -> Result<GenerationDecoder, SseError> {
        let native_limits = SseLimits {
            max_wire_bytes: limits.max_wire_bytes.min(body_limit),
            max_event_bytes: limits.max_event_bytes.min(body_limit),
            ..limits
        };
        Ok(match self {
            Self::OpenAi(adapter) => match adapter.protocol {
                GenerationApi::Chat => GenerationDecoder::Chat(ChatSseDecoder::with_decoder(
                    status,
                    content_type,
                    limits,
                    adapter.event_decoder(),
                )?),
                GenerationApi::Responses => {
                    GenerationDecoder::Responses(ResponsesSseDecoder::with_decoder(
                        status,
                        content_type,
                        limits,
                        adapter.event_decoder(),
                    )?)
                }
            },
            Self::AnthropicMessages {
                profile,
                dialect,
                target,
            } => GenerationDecoder::Messages {
                decoder: anthropic::messages::GenerationSseDecoder::with_dialect(
                    status,
                    content_type,
                    native_limits,
                    NATIVE_SCOPE,
                    target.clone(),
                    *dialect,
                )?,
                profile: *profile,
                target: target.clone(),
            },
            Self::GoogleGenerateContent { target } => GenerationDecoder::Google(
                crate::protocol::google::generate_content::GenerationSseDecoder::new(
                    status,
                    content_type,
                    native_limits,
                    NATIVE_SCOPE,
                    target.clone(),
                )?,
            ),
            Self::GoogleInteractions { target } => GenerationDecoder::Interactions(
                crate::protocol::google::interactions::GenerationSseDecoder::new(
                    status,
                    content_type,
                    native_limits,
                    NATIVE_SCOPE,
                    target.clone(),
                )?,
            ),
        })
    }
}

/// Protocol dispatch only. The execution owner gates terminal publication on EOF.
pub(crate) enum GenerationDecoder {
    Responses(ResponsesSseDecoder),
    Chat(ChatSseDecoder),
    Messages {
        decoder: anthropic::messages::GenerationSseDecoder,
        profile: anthropic::messages::Profile,
        target: crate::protocol::native::ReplayTarget,
    },
    Google(crate::protocol::google::generate_content::GenerationSseDecoder),
    Interactions(crate::protocol::google::interactions::GenerationSseDecoder),
}
impl GenerationDecoder {
    pub(crate) fn last_event(&self) -> Option<crate::protocol::stream::SourceEvent> {
        match self {
            Self::Responses(decoder) => decoder.last_event(),
            _ => None,
        }
    }
    pub(crate) fn event_failure(&self) -> Option<crate::protocol::failure::SourceFailureEvidence> {
        match self {
            Self::Responses(d) => d.event_failure(),
            _ => None,
        }
    }
    pub(crate) fn take_evidence(
        &mut self,
    ) -> crate::protocol::conversion::generation::EvidenceDelta {
        match self {
            Self::Responses(d) => d.take_intake_evidence(),
            Self::Chat(d) => d.take_intake_evidence(),
            Self::Messages { decoder, .. } => decoder.take_intake_evidence(),
            Self::Google(_) | Self::Interactions(_) => Default::default(),
        }
    }

    pub(crate) fn consume(&mut self, bytes: &[u8]) -> Result<(usize, Vec<StreamEvent>), SseError> {
        match self {
            Self::Responses(d) => d.consume(bytes),
            Self::Chat(d) => d.consume(bytes),
            Self::Messages { decoder, .. } => decoder.consume(bytes),
            Self::Google(d) => d.consume(bytes),
            Self::Interactions(d) => d.consume(bytes),
        }
    }
    pub(crate) fn ignored_events(&self) -> usize {
        match self {
            Self::Responses(d) => d.ignored_events(),
            _ => 0,
        }
    }
    pub(crate) fn metadata(&self) -> Option<&ResponseMetadata> {
        match self {
            Self::Responses(d) => d.metadata(),
            Self::Chat(d) => d.metadata(),
            Self::Messages { decoder, .. } => decoder.metadata(),
            Self::Google(d) => d.metadata(),
            Self::Interactions(d) => d.metadata(),
        }
    }
    pub(crate) fn fidelity(&self) -> &FidelityRecords {
        match self {
            Self::Responses(d) => d.fidelity(),
            Self::Chat(d) => d.fidelity(),
            Self::Messages { decoder, .. } => decoder.fidelity(),
            Self::Google(d) => d.fidelity(),
            Self::Interactions(d) => d.fidelity(),
        }
    }
    pub(crate) fn finish(&mut self) -> Result<DecodedResponse, SseError> {
        match self {
            Self::Responses(d) => {
                d.finish()?;
                if let Some(source) = d.event_failure() {
                    return Err(crate::protocol::CodecError::StreamFailure(source).into());
                }
                d.materialize()
            }
            Self::Chat(d) => {
                d.finish()?;
                d.materialize()
            }
            Self::Messages {
                decoder,
                profile,
                target,
            } => {
                let mut response =
                    profile.decode_generation_message(decoder.finish()?, NATIVE_SCOPE, target)?;
                response.evidence = decoder.take_intake_evidence();
                Ok(response)
            }
            Self::Google(d) => d.finish(),
            Self::Interactions(d) => d.finish(),
        }
    }
}
