//! Bounded incremental SSE framing. Network chunks are neither UTF-8 nor event boundaries.
//! CR, LF and CRLF are recognized without copying an entire untrusted chunk into a buffer.
use thiserror::Error;
#[derive(Debug, Error, Eq, PartialEq)]
pub enum SseDecodeError {
    #[error("SSE event exceeds the configured size limit")]
    EventTooLarge,
    #[error("SSE field is not valid UTF-8")]
    InvalidUtf8,
    #[error("SSE data ended without an event delimiter")]
    UnexpectedEof,
    #[error("SSE decoder is closed or rejected")]
    Closed,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SseEvent {
    event: Option<String>,
    data: String,
    id: Option<String>,
    retry_ms: Option<u64>,
}
impl SseEvent {
    pub fn event(&self) -> Option<&str> {
        self.event.as_deref()
    }
    pub fn data(&self) -> &str {
        &self.data
    }
    pub fn id(&self) -> Option<&str> {
        self.id.as_deref()
    }
    pub fn retry_ms(&self) -> Option<u64> {
        self.retry_ms
    }
}
pub struct SseDecoder {
    max_event_bytes: usize,
    line: Vec<u8>,
    current_bytes: usize,
    current: EventBuilder,
    skip_lf: bool,
    cr_completed_bytes: Option<usize>,
    first_line: bool,
    closed: bool,
}
impl SseDecoder {
    pub fn new(max_event_bytes: usize) -> Self {
        Self {
            max_event_bytes,
            line: vec![],
            current_bytes: 0,
            current: EventBuilder::default(),
            skip_lf: false,
            cr_completed_bytes: None,
            first_line: true,
            closed: false,
        }
    }
    /// Process a caller-owned chunk; only bounded unfinished line/event data is retained.
    pub fn push(&mut self, chunk: &[u8]) -> Result<Vec<SseEvent>, SseDecodeError> {
        let mut out = vec![];
        let mut offset = 0;
        while offset < chunk.len() {
            let (event, n) = self.push_until_event(&chunk[offset..])?;
            offset += n;
            if let Some(e) = event {
                out.push(e);
            }
        }
        Ok(out)
    }
    /// Consume at most one completed event, preserving the exact raw prefix for ingress.
    pub(crate) fn push_until_event(
        &mut self,
        chunk: &[u8],
    ) -> Result<(Option<SseEvent>, usize), SseDecodeError> {
        if self.closed {
            return Err(SseDecodeError::Closed);
        }
        let result = (|| {
            for (index, byte) in chunk.iter().copied().enumerate() {
                // The optional LF after CR adds no data and belongs to the same line ending.
                if self.skip_lf {
                    self.skip_lf = false;
                    if byte == b'\n' {
                        self.crlf_suffix()?;
                        continue;
                    }
                    self.cr_completed_bytes = None;
                }
                self.current_bytes = self
                    .current_bytes
                    .checked_add(1)
                    .ok_or(SseDecodeError::EventTooLarge)?;
                if self.current_bytes > self.max_event_bytes {
                    return Err(SseDecodeError::EventTooLarge);
                }
                if byte == b'\r' || byte == b'\n' {
                    self.skip_lf = byte == b'\r';
                    let raw_size = self.current_bytes;
                    let event = self.line_end()?;
                    if self.skip_lf && self.current_bytes == 0 {
                        self.cr_completed_bytes = Some(raw_size);
                    }
                    if let Some(event) = event {
                        let mut used = index + 1;
                        // Include an already available CRLF suffix in the raw event prefix.
                        if self.skip_lf && chunk.get(used) == Some(&b'\n') {
                            self.crlf_suffix()?;
                            used += 1;
                            self.skip_lf = false;
                        }
                        return Ok((Some(event), used));
                    }
                } else {
                    self.line.push(byte);
                }
            }
            Ok((None, chunk.len()))
        })();
        if result.is_err() {
            self.closed = true;
        }
        result
    }
    fn crlf_suffix(&mut self) -> Result<(), SseDecodeError> {
        let bytes = if let Some(n) = self.cr_completed_bytes.take() {
            n.checked_add(1).ok_or(SseDecodeError::EventTooLarge)?
        } else {
            self.current_bytes = self
                .current_bytes
                .checked_add(1)
                .ok_or(SseDecodeError::EventTooLarge)?;
            self.current_bytes
        };
        if bytes > self.max_event_bytes {
            return Err(SseDecodeError::EventTooLarge);
        }
        Ok(())
    }
    fn line_end(&mut self) -> Result<Option<SseEvent>, SseDecodeError> {
        let bytes = std::mem::take(&mut self.line);
        let mut line = std::str::from_utf8(&bytes).map_err(|_| SseDecodeError::InvalidUtf8)?;
        if self.first_line {
            self.first_line = false;
            line = line.strip_prefix('\u{feff}').unwrap_or(line);
        }
        if line.is_empty() {
            self.current_bytes = 0;
            return Ok(self.current.take_event());
        }
        if !line.starts_with(':') {
            self.current.apply_line(line);
        }
        Ok(None)
    }
    /// Legacy transport normalization permits a final fully parsed record at EOF.
    /// Typed Responses HTTP consumers use `finish_strict`, not this permissive policy.
    pub fn finish(&mut self) -> Result<Vec<SseEvent>, SseDecodeError> {
        self.finish_with_policy(false)
    }
    /// EOF never invents the blank line required to dispatch a business event.
    pub fn finish_strict(&mut self) -> Result<Vec<SseEvent>, SseDecodeError> {
        self.finish_with_policy(true)
    }
    fn finish_with_policy(&mut self, strict: bool) -> Result<Vec<SseEvent>, SseDecodeError> {
        if self.closed {
            return Err(SseDecodeError::Closed);
        }
        self.closed = true;
        if !self.line.is_empty() {
            self.line_end()?;
        }
        if strict {
            if !self.current.data_lines.is_empty() {
                return Err(SseDecodeError::UnexpectedEof);
            }
            return Ok(vec![]);
        }
        Ok(self.current.take_event().into_iter().collect())
    }
}
#[derive(Default)]
struct EventBuilder {
    event: Option<String>,
    data_lines: Vec<String>,
    id: Option<String>,
    retry_ms: Option<u64>,
    has_fields: bool,
}
impl EventBuilder {
    fn apply_line(&mut self, line: &str) {
        let (field, value) = line.split_once(':').unwrap_or((line, ""));
        let value = value.strip_prefix(' ').unwrap_or(value);
        match field {
            "event" => {
                self.event = Some(value.into());
                self.has_fields = true;
            }
            "data" => {
                self.data_lines.push(value.into());
                self.has_fields = true;
            }
            "id" if !value.contains('\0') => {
                self.id = Some(value.into());
                self.has_fields = true;
            }
            "retry" if value.bytes().all(|b| b.is_ascii_digit()) => {
                if let Ok(n) = value.parse() {
                    self.retry_ms = Some(n);
                    self.has_fields = true;
                }
            }
            _ => {}
        }
    }
    fn take_event(&mut self) -> Option<SseEvent> {
        if !self.has_fields {
            return None;
        }
        self.has_fields = false;
        Some(SseEvent {
            event: self.event.take(),
            data: std::mem::take(&mut self.data_lines).join("\n"),
            id: self.id.take(),
            retry_ms: self.retry_ms.take(),
        })
    }
}
#[cfg(test)]
mod tests {
    use super::SseDecoder;
    use proptest::prelude::*;
    proptest! {
        #![proptest_config(crate::test_properties::config())]
        #[test]
        fn property_sse_arbitrary_chunk_boundaries_preserve_independent_fields(
            data in prop::collection::vec(prop::sample::select(vec!['a', ' ', '你', 'é', '🦀']), 0..80),
            ending in prop::sample::select(vec!["\n", "\r", "\r\n"]),
            sizes in prop::collection::vec(1usize..24, 1..32)
        ) {
            let text: String = data.into_iter().collect();
            let wire = format!("\u{feff}event: delta{ending}id: fixture{ending}retry: 17{ending}data: {text}{ending}data: second{ending}{ending}data: done{ending}{ending}");
            let mut decoder = SseDecoder::new(4096);
            let mut events = Vec::new();
            let (mut offset, mut chunk) = (0, 0);
            while offset < wire.len() {
                let end = (offset + sizes[chunk % sizes.len()]).min(wire.len());
                while offset < end {
                    let (event, used) = decoder.push_until_event(&wire.as_bytes()[offset..end]).unwrap();
                    prop_assert!(used > 0 && used <= end - offset);
                    offset += used;
                    events.extend(event);
                }
                chunk += 1;
            }
            prop_assert!(decoder.finish_strict().unwrap().is_empty());
            prop_assert_eq!(events.len(), 2);
            prop_assert_eq!(events[0].event(), Some("delta"));
            prop_assert_eq!(events[0].id(), Some("fixture"));
            prop_assert_eq!(events[0].retry_ms(), Some(17));
            prop_assert_eq!(events[0].data(), format!("{text}\nsecond"));
            prop_assert_eq!(events[1].data(), "done");
        }
    }
    #[test]
    fn precommit_decode_returns_exactly_one_raw_event_prefix() {
        let payload = b": keepalive\n\nevent: first\ndata: one\n\nevent: second\ndata: two\n\n";
        let expected = b": keepalive\n\nevent: first\ndata: one\n\n";
        let mut d = SseDecoder::new(128);
        let (event, n) = d.push_until_event(payload).unwrap();
        assert_eq!(event.unwrap().event(), Some("first"));
        assert_eq!(&payload[..n], expected);
        assert_eq!(&payload[n..], b"event: second\ndata: two\n\n");
    }
}
