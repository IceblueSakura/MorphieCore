//! Encoded audio bytes are not a transcript, remote reference or playback guarantee.
use super::ValueError;
use bytes::Bytes;

pub const MAX_AUDIO_BYTES: usize = 16 << 20;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum AudioEncoding {
    Mp3,
    Opus,
    Aac,
    Flac,
    Wav,
    /// Headerless signed 16-bit little-endian samples, 24 kHz, one channel.
    PcmS16Le24KhzMono,
}

#[derive(Clone, Eq, PartialEq)]
pub struct AudioArtifact {
    data: Bytes,
    encoding: Option<AudioEncoding>,
}
impl std::fmt::Debug for AudioArtifact {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AudioArtifact")
            .field("data", &"[redacted]")
            .field("encoding", &self.encoding)
            .finish()
    }
}
impl AudioArtifact {
    /// Encoding is a reported/profile-bound fact, never copied from request intent.
    pub fn new(data: Bytes, encoding: Option<AudioEncoding>) -> Result<Self, ValueError> {
        if data.is_empty() {
            return Err(ValueError::Empty { kind: "audio" });
        }
        if data.len() > MAX_AUDIO_BYTES {
            return Err(ValueError::TooLarge {
                kind: "audio",
                max_bytes: MAX_AUDIO_BYTES,
            });
        }
        Ok(Self { data, encoding })
    }
    pub fn data(&self) -> &Bytes {
        &self.data
    }
    pub fn into_data(self) -> Bytes {
        self.data
    }
    pub fn encoding(&self) -> Option<AudioEncoding> {
        self.encoding
    }
}
