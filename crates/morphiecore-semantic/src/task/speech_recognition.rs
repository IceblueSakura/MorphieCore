//! File transcription is an independent task, not a conversation or a file service.
use bytes::Bytes;
use serde_json::Number;
use std::{cmp::Ordering, collections::BTreeSet};

pub const MAX_INPUT_BYTES: usize = 4 << 20;
pub const MAX_TEXT_BYTES: usize = 1 << 20;
pub const MAX_PROMPT_CHARS: usize = 4096;
pub const MAX_PROMPT_BYTES: usize = MAX_PROMPT_CHARS * 4;
pub const MAX_LANGUAGES: usize = 32;
pub const MAX_KEYWORDS: usize = 100;
pub const MAX_KEYWORD_BYTES: usize = 256;
pub const MAX_KEYWORDS_TOTAL_BYTES: usize = 8 << 10;

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[error("invalid or over-budget transcription value")]
pub struct RecognitionError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InputFormat {
    Wav,
    Mp3,
    Flac,
    Mp4,
    Mpeg,
    Mpga,
    M4a,
    Ogg,
    Webm,
}
impl InputFormat {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Wav => "wav",
            Self::Mp3 => "mp3",
            Self::Flac => "flac",
            Self::Mp4 => "mp4",
            Self::Mpeg => "mpeg",
            Self::Mpga => "mpga",
            Self::M4a => "m4a",
            Self::Ogg => "ogg",
            Self::Webm => "webm",
        }
    }
    pub fn mime(self) -> &'static str {
        match self {
            Self::Wav => "audio/wav",
            Self::Mp3 | Self::Mpeg | Self::Mpga => "audio/mpeg",
            Self::Flac => "audio/flac",
            Self::Mp4 | Self::M4a => "audio/mp4",
            Self::Ogg => "audio/ogg",
            Self::Webm => "audio/webm",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum OutputFormat {
    #[default]
    Json,
    Text,
}

fn valid_non_negative_decimal(number: &Number) -> Result<(i32, Vec<u8>), RecognitionError> {
    let raw = number.as_str();
    if raw.len() > 128 || raw.starts_with('-') {
        return Err(RecognitionError);
    }
    let (mantissa, exponent) = raw.split_once(['e', 'E']).unwrap_or((raw, "0"));
    let exponent: i32 = exponent.parse().map_err(|_| RecognitionError)?;
    let point = mantissa.find('.').unwrap_or(mantissa.len());
    let digits: String = mantissa.chars().filter(|c| *c != '.').collect();
    let Some(first) = digits.find(|c| c != '0') else {
        return Ok((0, Vec::new()));
    };
    let order = (point as i32 - first as i32)
        .checked_add(exponent)
        .ok_or(RecognitionError)?;
    Ok((order, digits.as_bytes()[first..].to_vec()))
}

/// Exact decimal temperature in `[0, 1]`; never rounded through binary float.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Temperature(Number);
impl Temperature {
    pub fn new(number: Number) -> Result<Self, RecognitionError> {
        let (order, significant) = valid_non_negative_decimal(&number)?;
        if !significant.is_empty() {
            let cmp_one = (0..significant.len().max(1))
                .map(|i| {
                    significant.get(i).copied().unwrap_or(b'0').cmp(&if i == 0 {
                        b'1'
                    } else {
                        b'0'
                    })
                })
                .find(|v| *v != Ordering::Equal)
                .unwrap_or(Ordering::Equal);
            if order > 1 || (order == 1 && cmp_one == Ordering::Greater) {
                return Err(RecognitionError);
            }
        }
        Ok(Self(number))
    }
    pub fn number(&self) -> &Number {
        &self.0
    }
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

fn valid_iso_639_1(code: &str) -> bool {
    code.len() == 2 && code.bytes().all(|b| b.is_ascii_lowercase())
}

#[derive(Clone, Eq, PartialEq)]
pub struct RecognitionRequest {
    pub audio: AudioInput,
    pub language: Option<String>,
    pub languages: Vec<String>,
    pub keywords: Vec<String>,
    pub prompt: Option<String>,
    pub temperature: Option<Temperature>,
    pub response_format: OutputFormat,
}
impl std::fmt::Debug for RecognitionRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RecognitionRequest")
            .field("audio", &self.audio)
            .field("language", &self.language)
            .field("languages", &self.languages)
            .field("keywords", &self.keywords.len())
            .field("prompt", &self.prompt.as_ref().map(|_| "[redacted]"))
            .field("temperature", &self.temperature)
            .field("response_format", &self.response_format)
            .finish()
    }
}
impl RecognitionRequest {
    pub fn new(audio: AudioInput) -> Self {
        Self {
            audio,
            language: None,
            languages: Vec::new(),
            keywords: Vec::new(),
            prompt: None,
            temperature: None,
            response_format: OutputFormat::Json,
        }
    }
    pub fn validate(&self) -> Result<(), RecognitionError> {
        if self.language.is_some() && !self.languages.is_empty() {
            return Err(RecognitionError);
        }
        if self
            .language
            .as_deref()
            .is_some_and(|s| !valid_iso_639_1(s))
        {
            return Err(RecognitionError);
        }
        if self.languages.len() > MAX_LANGUAGES {
            return Err(RecognitionError);
        }
        let mut seen_languages = BTreeSet::new();
        for code in &self.languages {
            if !valid_iso_639_1(code) || !seen_languages.insert(code.as_str()) {
                return Err(RecognitionError);
            }
        }
        if self.keywords.len() > MAX_KEYWORDS {
            return Err(RecognitionError);
        }
        let mut keyword_bytes = 0usize;
        for keyword in &self.keywords {
            if keyword.is_empty()
                || keyword.len() > MAX_KEYWORD_BYTES
                || keyword.chars().any(char::is_control)
            {
                return Err(RecognitionError);
            }
            keyword_bytes = keyword_bytes
                .checked_add(keyword.len())
                .filter(|total| *total <= MAX_KEYWORDS_TOTAL_BYTES)
                .ok_or(RecognitionError)?;
        }
        if self
            .prompt
            .as_deref()
            .is_some_and(|p| p.len() > MAX_PROMPT_BYTES || p.chars().count() > MAX_PROMPT_CHARS)
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
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RecognitionUsage {
    /// Reported processed duration in seconds; never inferred from timestamps or file metadata.
    Duration(Number),
    Tokens {
        input_tokens: u64,
        output_tokens: u64,
        total_tokens: u64,
        audio_tokens: Option<u64>,
        text_tokens: Option<u64>,
    },
}
impl RecognitionUsage {
    pub fn duration_seconds(seconds: u64) -> Self {
        Self::Duration(Number::from(seconds))
    }
    pub fn duration(seconds: Number) -> Result<Self, RecognitionError> {
        valid_non_negative_decimal(&seconds)?;
        Ok(Self::Duration(seconds))
    }
    pub fn validate(&self) -> Result<(), RecognitionError> {
        match self {
            Self::Duration(seconds) => {
                valid_non_negative_decimal(seconds)?;
                Ok(())
            }
            Self::Tokens {
                audio_tokens,
                text_tokens,
                ..
            } => {
                if let (Some(audio), Some(text)) = (audio_tokens, text_tokens) {
                    audio.checked_add(*text).ok_or(RecognitionError)?;
                }
                Ok(())
            }
        }
    }
}

#[derive(Clone, Default, Eq, PartialEq)]
pub struct RecognitionReport {
    pub request_id: Option<String>,
    pub usage: Option<RecognitionUsage>,
    pub detected_languages: Option<Vec<String>>,
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
        if let Some(usage) = &self.usage {
            usage.validate()?;
        }
        if let Some(languages) = &self.detected_languages
            && (languages.len() > MAX_LANGUAGES
                || languages
                    .iter()
                    .any(|s| s.is_empty() || s.len() > 32 || s.chars().any(char::is_control)))
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
