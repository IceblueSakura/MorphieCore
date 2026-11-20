//! Shared bounded SSE admission and codec limits.
//!
//! Enforces bounded memory and event consumption across upstream and downstream
//! SSE event streams without socket I/O or protocol wire grammar dependencies.
use crate::{
    CodecError,
    semantic::task::generation::{EventError, MAX_TOTAL_BYTES},
    sse::{SseDecodeError, SseDecoder, SseEvent},
};

/// Last complete source frame, not necessarily the frame causing a framing/EOF error.
/// Event names come from a protocol-owned static vocabulary, never raw input.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SourceEvent {
    pub ordinal: usize,
    pub kind: &'static str,
    pub sequence: Option<u64>,
}

/// Hard resource consumption bounds for Server-Sent Events (SSE) streams.
#[derive(Clone, Copy, Debug)]
pub struct SseLimits {
    /// Maximum byte size of an individual SSE event data payload.
    pub max_event_bytes: usize,
    /// Maximum cumulative wire byte limit allowed for an entire stream session.
    pub max_wire_bytes: usize,
    /// Maximum total event count permitted before terminating the stream.
    pub max_events: usize,
    /// Aggregate decoded UTF-8 padding bytes; independent of wire and semantic budgets.
    /// Zero forbids non-empty padding without disabling unpadded streams.
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
    /// Validates that limits are non-zero.
    pub fn validate(self) -> Result<(), SseError> {
        if self.max_event_bytes == 0 || self.max_wire_bytes == 0 || self.max_events == 0 {
            return Err(SseError::Limit);
        }
        Ok(())
    }
}

/// Bounded framing and lifecycle for a typed upstream decoder. Protocols
/// decide when an observed frame is chargeable and what proves completion.
pub(crate) struct SseIntake {
    framing: SseDecoder,
    limits: SseLimits,
    wire_bytes: usize,
    frames: usize,
    closed: bool,
    rejected: bool,
}

impl SseIntake {
    pub(crate) fn new(limits: SseLimits) -> Self {
        Self {
            framing: SseDecoder::new(limits.max_event_bytes),
            limits,
            wire_bytes: 0,
            frames: 0,
            closed: false,
            rejected: false,
        }
    }

    pub(crate) fn open(&self) -> Result<(), SseError> {
        if self.closed || self.rejected {
            return Err(SseError::Closed);
        }
        Ok(())
    }

    pub(crate) fn completed(&self) -> bool {
        self.closed && !self.rejected
    }

    pub(crate) fn reject(&mut self) {
        self.rejected = true;
    }

    pub(crate) fn pull(&mut self, chunk: &[u8]) -> Result<(Option<SseEvent>, usize), SseError> {
        let remaining = self.limits.max_wire_bytes.saturating_sub(self.wire_bytes);
        let available = chunk.len().min(remaining.saturating_add(1));
        let (frame, used) = self.framing.push_until_event(&chunk[..available])?;
        self.wire_bytes = self.wire_bytes.checked_add(used).ok_or(SseError::Limit)?;
        if self.wire_bytes > self.limits.max_wire_bytes {
            return Err(SseError::Limit);
        }
        Ok((frame, used))
    }

    pub(crate) fn next_ordinal(&self) -> usize {
        self.frames.saturating_add(1)
    }

    pub(crate) fn charge_frame(&mut self) -> Result<(), SseError> {
        self.frames = self.frames.checked_add(1).ok_or(SseError::Limit)?;
        if self.frames > self.limits.max_events {
            return Err(SseError::Limit);
        }
        Ok(())
    }

    pub(crate) fn finish_framing(&mut self) -> Result<(), SseError> {
        self.closed = true;
        self.framing.finish_strict()?;
        Ok(())
    }
}

/// The wire adapter, not a trailer or the framer, supplies the terminal fact.
pub(crate) fn require_terminal(observed: bool) -> Result<(), SseError> {
    if !observed {
        return Err(CodecError::Event(EventError::EofBeforeTerminal).into());
    }
    Ok(())
}

/// Validates HTTP status and Content-Type for incoming SSE streams.
///
/// Requires HTTP 200 and `text/event-stream; charset=utf-8`.
pub fn validate_http(status: u16, content_type: &str) -> Result<(), SseError> {
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

/// SSE transport and admission error classifications.
#[derive(Clone, Debug, thiserror::Error)]
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
