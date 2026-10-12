//! Shared bounded SSE admission and codec errors; no socket I/O or wire grammar.
use crate::{
    protocol::CodecError, semantic::task::generation::MAX_TOTAL_BYTES,
    transport::sse::SseDecodeError,
};
#[derive(Clone, Copy, Debug)]
pub struct SseLimits {
    pub max_event_bytes: usize,
    pub max_wire_bytes: usize,
    pub max_events: usize,
    /// Aggregate decoded UTF-8 padding bytes; independent of wire and semantic budgets.
    /// Zero forbids nonempty padding without disabling unpadded streams.
    pub max_obfuscation_bytes: usize,
}
impl Default for SseLimits {
    fn default() -> Self {
        Self {
            max_event_bytes: MAX_TOTAL_BYTES + 256,
            max_wire_bytes: 64 << 20,
            max_events: 1_000_000,
            max_obfuscation_bytes: 4 << 20,
        }
    }
}
impl SseLimits {
    pub(crate) fn validate(self) -> Result<(), SseError> {
        if self.max_event_bytes == 0 || self.max_wire_bytes == 0 || self.max_events == 0 {
            return Err(SseError::Limit);
        }
        Ok(())
    }
}
pub(crate) fn validate_http(status: u16, content_type: &str) -> Result<(), SseError> {
    let media: mime::Mime = content_type.parse().map_err(|_| SseError::Http)?;
    if status != 200
        || media.type_() != mime::TEXT
        || media.subtype() != "event-stream"
        || media
            .params()
            .any(|(k, v)| k == mime::CHARSET && v != mime::UTF_8)
    {
        return Err(SseError::Http);
    }
    Ok(())
}
#[derive(Debug, thiserror::Error)]
pub enum SseError {
    #[error("invalid SSE HTTP status or content type")]
    Http,
    #[error("SSE budget exceeded")]
    Limit,
    #[error("SSE event and JSON type disagree")]
    EventType,
    #[error("SSE decoder is closed or rejected")]
    Closed,
    #[error(transparent)]
    Framing(#[from] SseDecodeError),
    #[error(transparent)]
    Codec(#[from] CodecError),
}
