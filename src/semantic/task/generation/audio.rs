//! Generated audio is an artifact, not ordinary text or a fetchable URL.
//! Format is request intent unless independently reported; expiry concerns the remote reference.
use super::{GenerationError, MAX_TEXT_BYTES};
use crate::semantic::value::Text;
use base64::{Engine, engine::general_purpose::STANDARD};
use sha2::{Digest, Sha256};
pub const MAX_AUDIO_DECODED_BYTES: usize = 768 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq, strum::EnumString, strum::IntoStaticStr)]
#[strum(serialize_all = "snake_case")]
pub enum AudioFormat {
    Wav,
    Aac,
    Mp3,
    Flac,
    Opus,
    Pcm16,
}
impl AudioFormat {
    pub fn label(self) -> &'static str {
        self.into()
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AudioOutputOptions {
    pub format: AudioFormat,
    pub voice: Text,
}
impl AudioOutputOptions {
    pub fn validate(&self) -> Result<(), GenerationError> {
        if self.voice.as_str().is_empty() || self.voice.as_str().len() > 128 {
            return Err(GenerationError::InvalidControl);
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, strum::EnumString, strum::IntoStaticStr)]
#[strum(serialize_all = "snake_case")]
pub enum OutputModality {
    Text,
    Audio,
}
impl OutputModality {
    pub fn label(self) -> &'static str {
        self.into()
    }
}

/// A remote generated-audio reference. Provenance belongs to the boundary sidecar.
/// An absent expiry is unreported, not unlimited retention. This type performs no I/O.
#[derive(Clone, Eq, PartialEq)]
pub struct AudioReference {
    id: Text,
    expires_at: Option<u64>,
}
impl std::fmt::Debug for AudioReference {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AudioReference")
            .field("id", &"[redacted]")
            .field("expires_at", &self.expires_at)
            .finish()
    }
}
impl AudioReference {
    pub fn new(id: Text, expires_at: Option<u64>) -> Result<Self, GenerationError> {
        if id.as_str().is_empty() || id.as_str().len() > 256 {
            return Err(GenerationError::InvalidResource);
        }
        Ok(Self { id, expires_at })
    }
    pub fn id(&self) -> &str {
        self.id.as_str()
    }
    pub fn expires_at(&self) -> Option<u64> {
        self.expires_at
    }
    pub fn bytes(&self) -> usize {
        self.id.as_str().len() + 8
    }
    pub fn check_at(&self, now: u64) -> Result<(), GenerationError> {
        if self.expires_at.is_some_and(|end| now >= end) {
            return Err(GenerationError::InvalidResource);
        }
        Ok(())
    }
    pub fn fingerprint(&self) -> [u8; 32] {
        let mut hash = Sha256::new();
        hash.update(self.id.as_str().as_bytes());
        hash.update([u8::from(self.expires_at.is_some())]);
        hash.update(self.expires_at.unwrap_or_default().to_le_bytes());
        hash.finalize().into()
    }
}
#[derive(Clone, Eq, PartialEq)]
pub struct GeneratedAudio {
    reference: AudioReference,
    data_base64: Text,
    transcript: Text,
}
impl std::fmt::Debug for GeneratedAudio {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GeneratedAudio")
            .field("reference", &self.reference)
            .field("content", &"[redacted]")
            .finish()
    }
}
impl GeneratedAudio {
    pub fn new(
        reference: AudioReference,
        data_base64: Text,
        transcript: Text,
    ) -> Result<Self, GenerationError> {
        let value = Self {
            reference,
            data_base64,
            transcript,
        };
        value.validate()?;
        Ok(value)
    }
    pub fn reference(&self) -> &AudioReference {
        &self.reference
    }
    pub fn data_base64(&self) -> &str {
        self.data_base64.as_str()
    }
    pub fn transcript(&self) -> &str {
        self.transcript.as_str()
    }
    pub fn bytes(&self) -> usize {
        self.reference.bytes() + self.data_base64.as_str().len() + self.transcript.as_str().len()
    }
    pub fn validate(&self) -> Result<(), GenerationError> {
        if self.bytes() > MAX_TEXT_BYTES {
            return Err(GenerationError::Limit);
        }
        if self.reference.expires_at.is_none() || self.data_base64.as_str().is_empty() {
            return Err(GenerationError::InvalidResource);
        }
        let bytes = STANDARD
            .decode(self.data_base64.as_str())
            .map_err(|_| GenerationError::InvalidResource)?;
        if bytes.len() > MAX_AUDIO_DECODED_BYTES {
            return Err(GenerationError::Limit);
        }
        Ok(())
    }
    pub fn fingerprint(&self) -> [u8; 32] {
        let mut hash = Sha256::new();
        hash.update(self.reference.fingerprint());
        for value in [self.data_base64(), self.transcript()] {
            hash.update((value.len() as u64).to_le_bytes());
            hash.update(value.as_bytes());
        }
        hash.finalize().into()
    }
}
