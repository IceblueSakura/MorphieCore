//! File transcription is an independent task, not a conversation or a file service.
use bytes::Bytes;
pub const MAX_INPUT_BYTES: usize = 1 << 20;
pub const MAX_TEXT_BYTES: usize = 1 << 20;
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[error("invalid or over-budget transcription value")]
pub struct RecognitionError;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InputFormat {
    Wav,
    Mp3,
}
#[derive(Clone, Eq, PartialEq)]
pub struct AudioInput {
    data: Bytes,
    format: InputFormat,
}
impl AudioInput {
    pub fn new(data: Bytes, format: InputFormat) -> Result<Self, RecognitionError> {
        if data.is_empty() || data.len() > MAX_INPUT_BYTES {
            return Err(RecognitionError);
        }
        Ok(Self { data, format })
    }
    pub fn data(&self) -> &Bytes {
        &self.data
    }
    pub fn format(&self) -> InputFormat {
        self.format
    }
}
impl std::fmt::Debug for AudioInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AudioInput")
            .field("data", &"[redacted]")
            .field("format", &self.format)
            .finish()
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecognitionRequest {
    pub audio: AudioInput,
    pub language: Option<String>,
}
impl RecognitionRequest {
    pub fn new(audio: AudioInput) -> Self {
        Self {
            audio,
            language: None,
        }
    }
    pub fn validate(&self) -> Result<(), RecognitionError> {
        if self
            .language
            .as_ref()
            .is_some_and(|s| s.len() != 2 || !s.bytes().all(|b| b.is_ascii_lowercase()))
        {
            return Err(RecognitionError);
        }
        Ok(())
    }
}
#[derive(Clone, Eq, PartialEq)]
pub struct RecognizedWord {
    pub text: String,
    pub punctuation: String,
    pub start_ms: u64,
    pub end_ms: u64,
}
#[derive(Clone, Eq, PartialEq)]
pub struct RecognizedSentence {
    pub id: u64,
    pub channel: u64,
    pub text: String,
    pub start_ms: u64,
    pub end_ms: u64,
    pub words: Vec<RecognizedWord>,
}
#[derive(Clone, Default, Eq, PartialEq)]
pub struct RecognitionReport {
    pub request_id: Option<String>,
    /// Actual processed duration, not a fabricated precise file-duration measurement.
    pub processed_seconds: Option<u64>,
    /// A final current-sentence report does not establish complete timing coverage.
    pub last_sentence: Option<RecognizedSentence>,
}
impl RecognitionReport {
    pub fn validate(&self, text: &str) -> Result<(), RecognitionError> {
        if self
            .request_id
            .as_ref()
            .is_some_and(|s| s.is_empty() || s.len() > 256 || s.chars().any(char::is_control))
        {
            return Err(RecognitionError);
        }
        if let Some(s) = &self.last_sentence {
            if s.id == 0
                || s.text.len() > MAX_TEXT_BYTES
                || !text.ends_with(&s.text)
                || s.start_ms > s.end_ms
                || s.words.len() > 65536
            {
                return Err(RecognitionError);
            }
            let mut words = String::new();
            for w in &s.words {
                if w.start_ms > w.end_ms
                    || w.start_ms < s.start_ms
                    || w.end_ms > s.end_ms
                    || words
                        .len()
                        .saturating_add(w.text.len())
                        .saturating_add(w.punctuation.len())
                        > MAX_TEXT_BYTES
                {
                    return Err(RecognitionError);
                }
                words.push_str(&w.text);
                words.push_str(&w.punctuation);
            }
            if !s.words.is_empty() && words != s.text {
                return Err(RecognitionError);
            }
        }
        Ok(())
    }
}
#[derive(Clone, Eq, PartialEq)]
pub struct RecognitionResult {
    text: String,
    report: RecognitionReport,
}
impl std::fmt::Debug for RecognitionResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("RecognitionResult([redacted])")
    }
}
impl RecognitionResult {
    pub fn new(text: String, report: RecognitionReport) -> Result<Self, RecognitionError> {
        if text.len() > MAX_TEXT_BYTES {
            return Err(RecognitionError);
        }
        report.validate(&text)?;
        Ok(Self { text, report })
    }
    pub fn text(&self) -> &str {
        &self.text
    }
    pub fn report(&self) -> &RecognitionReport {
        &self.report
    }
    pub fn set_text(&mut self, text: impl Into<String>) -> Result<(), RecognitionError> {
        let text = text.into();
        if text.len() > MAX_TEXT_BYTES {
            return Err(RecognitionError);
        }
        self.text = text;
        self.report = RecognitionReport::default();
        Ok(())
    }
}
