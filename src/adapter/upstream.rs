//! Upstream response dispatch, independent of the standard client's OpenAI profile.
use crate::{
    protocol::{
        CodecError, DecodedResponse, ResponseMetadata, anthropic,
        fidelity::FidelityRecords,
        openai::{
            Adapter, Profile, chat::sse::ChatSseDecoder, responses::sse::ResponsesSseDecoder,
        },
        stream::{SseError, SseLimits},
    },
    semantic::task::generation::{LocalScope, StreamEvent},
};

const NATIVE_SCOPE: LocalScope = LocalScope::new(2);

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UpstreamAdapter {
    OpenAi(Box<Adapter>),
    /// Native intake labels opaque values with the trusted Provider/model binding.
    AnthropicMessages {
        profile: anthropic::messages::Profile,
        target: crate::protocol::native::ReplayTarget,
    },
    GoogleGenerateContent {
        target: crate::protocol::native::ReplayTarget,
    },
    GoogleInteractions {
        target: crate::protocol::native::ReplayTarget,
    },
}
impl From<Adapter> for UpstreamAdapter {
    fn from(adapter: Adapter) -> Self {
        Self::OpenAi(Box::new(adapter))
    }
}
impl UpstreamAdapter {
    pub fn openai(&self) -> Result<&Adapter, CodecError> {
        match self {
            Self::OpenAi(adapter) => Ok(adapter),
            Self::AnthropicMessages { .. }
            | Self::GoogleGenerateContent { .. }
            | Self::GoogleInteractions { .. } => Err(CodecError::ProfileMismatch),
        }
    }
    pub fn decode_response(&self, bytes: &[u8]) -> Result<DecodedResponse, CodecError> {
        match self {
            Self::OpenAi(adapter) => adapter.decode_response(bytes),
            Self::AnthropicMessages { profile, target } => {
                profile.decode_generation_response(bytes, NATIVE_SCOPE, target)
            }
            Self::GoogleGenerateContent { .. } | Self::GoogleInteractions { .. } => {
                Err(CodecError::Unsupported("Google intake requires SSE".into()))
            }
        }
    }
    pub(crate) fn is_native(&self) -> bool {
        !matches!(self, Self::OpenAi(_))
    }
    pub(crate) fn accepts_json(&self) -> bool {
        matches!(self, Self::OpenAi(_) | Self::AnthropicMessages { .. })
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
                Profile::Chat => GenerationDecoder::Chat(ChatSseDecoder::with_decoder(
                    status,
                    content_type,
                    limits,
                    adapter.event_decoder(),
                )?),
                Profile::Responses => {
                    GenerationDecoder::Responses(ResponsesSseDecoder::with_decoder(
                        status,
                        content_type,
                        limits,
                        adapter.event_decoder(),
                    )?)
                }
            },
            Self::AnthropicMessages { profile, target } => GenerationDecoder::Messages {
                decoder: anthropic::messages::GenerationSseDecoder::new(
                    status,
                    content_type,
                    native_limits,
                    NATIVE_SCOPE,
                    target.clone(),
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
            } => Ok(profile.decode_generation_message(decoder.finish()?, NATIVE_SCOPE, target)?),
            Self::Google(d) => d.finish(),
            Self::Interactions(d) => d.finish(),
        }
    }
}
