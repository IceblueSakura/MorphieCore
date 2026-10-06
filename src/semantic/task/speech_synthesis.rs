//! Independent text-to-speech values, with no conversation or runtime target.
use crate::semantic::value::AudioEncoding;
use serde_json::Number;
use std::cmp::Ordering;
mod result;
pub use result::{AudioReference, SpeechReport, SpeechResult, SpeechSentence, SpeechWord};

pub const MAX_INPUT_CHARS: usize = 4096;
pub const MAX_INPUT_BYTES: usize = MAX_INPUT_CHARS * 4;
pub const MAX_VOICE_BYTES: usize = 128;

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[error("invalid or over-budget speech value")]
pub struct SpeechError;

/// Exact decimal multiplier; range checks cannot round values into admission.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Speed(Number);
impl Speed {
    pub fn new(number: Number) -> Result<Self, SpeechError> {
        let raw = number.as_str();
        if raw.len() > 128 || raw.starts_with('-') {
            return Err(SpeechError);
        }
        let (mantissa, exponent) = raw.split_once(['e', 'E']).unwrap_or((raw, "0"));
        let exponent: i32 = exponent.parse().map_err(|_| SpeechError)?;
        let point = mantissa.find('.').unwrap_or(mantissa.len());
        let digits: String = mantissa.chars().filter(|c| *c != '.').collect();
        let first = digits.find(|c| c != '0').ok_or(SpeechError)?;
        let order = (point as i32 - first as i32)
            .checked_add(exponent)
            .ok_or(SpeechError)?;
        let significant = &digits.as_bytes()[first..];
        let compare = |bound: &[u8]| {
            (0..significant.len().max(bound.len()))
                .map(|i| {
                    significant
                        .get(i)
                        .copied()
                        .unwrap_or(b'0')
                        .cmp(&bound.get(i).copied().unwrap_or(b'0'))
                })
                .find(|v| *v != Ordering::Equal)
                .unwrap_or(Ordering::Equal)
        };
        if !(order == 0 && compare(b"25") != Ordering::Less
            || order == 1 && compare(b"4") != Ordering::Greater)
        {
            return Err(SpeechError);
        }
        Ok(Self(number))
    }
    pub fn number(&self) -> &Number {
        &self.0
    }
}

pub fn valid_voice(voice: &str) -> bool {
    !voice.is_empty()
        && voice.len() <= MAX_VOICE_BYTES
        && !voice.chars().any(|c| c.is_control() || c.is_whitespace())
}
fn text_in_bounds(value: &str) -> bool {
    value.len() <= MAX_INPUT_BYTES && value.chars().count() <= MAX_INPUT_CHARS
}

#[derive(Clone, Eq, PartialEq)]
pub struct SpeechRequest {
    input: String,
    voice: String,
    pub instructions: Option<String>,
    pub speed: Option<Speed>,
    pub format: Option<AudioEncoding>,
}
impl std::fmt::Debug for SpeechRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SpeechRequest")
            .field("input", &"[redacted]")
            .field("voice", &"[redacted]")
            .field(
                "instructions",
                &self.instructions.as_ref().map(|_| "[redacted]"),
            )
            .field("speed", &self.speed)
            .field("format", &self.format)
            .finish()
    }
}
impl SpeechRequest {
    pub fn new(input: impl Into<String>, voice: impl Into<String>) -> Result<Self, SpeechError> {
        let value = Self {
            input: input.into(),
            voice: voice.into(),
            instructions: None,
            speed: None,
            format: None,
        };
        value.validate()?;
        Ok(value)
    }
    pub fn input(&self) -> &str {
        &self.input
    }
    pub fn voice(&self) -> &str {
        &self.voice
    }
    pub fn set_input(&mut self, input: impl Into<String>) -> Result<(), SpeechError> {
        let input = input.into();
        if input.is_empty() || !text_in_bounds(&input) {
            return Err(SpeechError);
        }
        self.input = input;
        Ok(())
    }
    pub fn set_voice(&mut self, voice: impl Into<String>) -> Result<(), SpeechError> {
        let voice = voice.into();
        if !valid_voice(&voice) {
            return Err(SpeechError);
        }
        self.voice = voice;
        Ok(())
    }
    pub fn requested_format(&self) -> AudioEncoding {
        self.format.unwrap_or(AudioEncoding::Mp3)
    }
    pub fn validate(&self) -> Result<(), SpeechError> {
        if self.input.is_empty()
            || !text_in_bounds(&self.input)
            || !valid_voice(&self.voice)
            || self
                .instructions
                .as_ref()
                .is_some_and(|v| !text_in_bounds(v))
        {
            return Err(SpeechError);
        }
        Ok(())
    }
}
