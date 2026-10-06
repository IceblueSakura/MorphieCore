//! Bounded byte accumulation. Each wire codec decodes its own transport fragments.
use super::{
    AudioReference, GeneratedAudio, GenerationError, MAX_AUDIO_DECODED_BYTES, MAX_TEXT_BYTES,
};
use crate::semantic::value::Text;
use base64::{Engine, engine::general_purpose::STANDARD};
#[derive(Clone, Default, Eq, PartialEq)]
pub struct AudioUpdate {
    pub id: Option<Text>,
    pub data: Option<Vec<u8>>,
    pub transcript: Option<Text>,
    pub expires_at: Option<u64>,
}
impl std::fmt::Debug for AudioUpdate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("AudioUpdate([redacted])")
    }
}
#[derive(Clone, Default, Eq, PartialEq)]
pub struct AudioBuffer {
    id: Option<Text>,
    data: Vec<u8>,
    transcript: Option<String>,
    expires_at: Option<u64>,
}
impl std::fmt::Debug for AudioBuffer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("AudioBuffer([redacted])")
    }
}
impl AudioBuffer {
    pub fn bytes(&self) -> usize {
        self.data.len().div_ceil(3) * 4
            + self.transcript.as_ref().map_or(0, String::len)
            + self.id.as_ref().map_or(0, |v| v.as_str().len())
            + usize::from(self.expires_at.is_some()) * 8
    }
    pub fn push(&mut self, update: &AudioUpdate) -> Result<(), GenerationError> {
        if self.expires_at.is_some()
            || (update.id.is_none()
                && update.data.is_none()
                && update.transcript.is_none()
                && update.expires_at.is_none())
        {
            return Err(GenerationError::InvalidResource);
        }
        if let Some(id) = &update.id {
            AudioReference::new(id.clone(), None)?;
            if self.id.as_ref().is_some_and(|old| old != id) {
                return Err(GenerationError::InvalidResource);
            }
            self.id = Some(id.clone());
        }
        let size = self
            .bytes()
            .saturating_add(update.data.as_ref().map_or(0, |v| v.len().div_ceil(3) * 4))
            .saturating_add(update.transcript.as_ref().map_or(0, |v| v.as_str().len()));
        if size > MAX_TEXT_BYTES
            || self
                .data
                .len()
                .saturating_add(update.data.as_ref().map_or(0, Vec::len))
                > MAX_AUDIO_DECODED_BYTES
        {
            return Err(GenerationError::Limit);
        }
        if let Some(data) = &update.data {
            self.data.extend_from_slice(data);
        }
        if let Some(transcript) = &update.transcript {
            self.transcript
                .get_or_insert_with(String::new)
                .push_str(transcript.as_str());
        }
        self.expires_at = update.expires_at;
        if self.bytes() > MAX_TEXT_BYTES {
            return Err(GenerationError::Limit);
        }
        Ok(())
    }
    pub fn finish(self) -> Result<GeneratedAudio, GenerationError> {
        let id = self.id.ok_or(GenerationError::InvalidResource)?;
        let transcript = self.transcript.ok_or(GenerationError::InvalidResource)?;
        GeneratedAudio::new(
            AudioReference::new(id, self.expires_at)?,
            Text::new(STANDARD.encode(&self.data), "audio", MAX_TEXT_BYTES)
                .map_err(|_| GenerationError::Limit)?,
            Text::allowing_empty(transcript, "transcript", MAX_TEXT_BYTES)
                .map_err(|_| GenerationError::Limit)?,
        )
    }
}
