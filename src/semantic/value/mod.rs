//! Shared semantic leaf values.
mod audio;
pub use audio::{AudioArtifact, AudioEncoding, MAX_AUDIO_BYTES};
/// Measure JSON without allocating an unbounded serialized copy.
pub fn json_size(value: &impl serde::Serialize, max: usize) -> Result<usize, ValueError> {
    struct Counter {
        bytes: usize,
        max: usize,
    }
    impl std::io::Write for Counter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.bytes = self.bytes.saturating_add(bytes.len());
            if self.bytes > self.max {
                return Err(std::io::Error::other("JSON limit"));
            }
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut count = Counter { bytes: 0, max };
    serde_json::to_writer(&mut count, value).map_err(|_| ValueError::TooLarge {
        kind: "JSON",
        max_bytes: max,
    })?;
    Ok(count.bytes)
}
mod json;
pub use json::{JsonError, JsonLimits, parse_json};
pub(crate) use json::{parse_image_response_json, parse_json_view};
mod image;
pub use image::ImageFormat;
mod presence;
mod replay;
mod timestamp;
pub use presence::Presence;
pub use replay::ReplayOrigin;
pub(crate) use timestamp::valid_timestamp;
#[derive(Clone, Debug, Eq, thiserror::Error, PartialEq)]
pub enum ValueError {
    #[error("{kind} must not be empty")]
    Empty { kind: &'static str },
    #[error("{kind} exceeds the {max_bytes}-byte limit")]
    TooLarge {
        kind: &'static str,
        max_bytes: usize,
    },
}
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct Text(String);
impl Text {
    pub fn new(
        value: impl Into<String>,
        kind: &'static str,
        max_bytes: usize,
    ) -> Result<Self, ValueError> {
        let value = value.into();
        if value.is_empty() {
            return Err(ValueError::Empty { kind });
        }
        if value.len() > max_bytes {
            return Err(ValueError::TooLarge { kind, max_bytes });
        }
        Ok(Self(value))
    }
    /// Bounded text whose empty value is meaningful (message content and tool output).
    pub fn allowing_empty(
        value: impl Into<String>,
        kind: &'static str,
        max_bytes: usize,
    ) -> Result<Self, ValueError> {
        let value = value.into();
        if value.len() > max_bytes {
            return Err(ValueError::TooLarge { kind, max_bytes });
        }
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
