//! A builder is consumed at completion; its JSON value replaces, not duplicates, it.
use crate::task::generation::{GenerationError, StructuredValue, ToolArguments};
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StreamPartValue {
    Text(String),
    AudioFragments(crate::task::generation::AudioBuffer),
    Audio(crate::task::generation::GeneratedAudio),
    JsonFragments(String),
    Json(StructuredValue),
}
impl StreamPartValue {
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Self::Text(value) => Some(value),
            _ => None,
        }
    }
    pub fn fragments(&self) -> Option<&str> {
        match self {
            Self::Text(value) | Self::JsonFragments(value) => Some(value),
            _ => None,
        }
    }
    pub fn wire_text(&self) -> String {
        match self {
            Self::Text(value) | Self::JsonFragments(value) => value.clone(),
            Self::Json(value) => value.value().to_string(),
            Self::AudioFragments(_) | Self::Audio(_) => unreachable!("audio is not a text delta"),
        }
    }
    pub fn push(&mut self, fragment: &str) -> Result<(), GenerationError> {
        match self {
            Self::Text(value) | Self::JsonFragments(value) => value.push_str(fragment),
            _ => return Err(GenerationError::InvalidArguments),
        }
        Ok(())
    }
    pub fn arguments(&self) -> ToolArguments {
        match self {
            Self::Text(value) => value.clone().into(),
            Self::JsonFragments(value) => ToolArguments::StructuredPartial(value.clone()),
            Self::Json(value) => ToolArguments::Structured(value.clone()),
            Self::AudioFragments(_) | Self::Audio(_) => {
                unreachable!("audio is not a tool argument")
            }
        }
    }
    pub fn bytes(&self) -> Result<usize, GenerationError> {
        match self {
            Self::Text(value) | Self::JsonFragments(value) => Ok(value.len()),
            Self::Json(value) => value.bytes(),
            Self::AudioFragments(value) => Ok(value.bytes()),
            Self::Audio(value) => Ok(value.bytes()),
        }
    }
    pub fn finish(self) -> Result<Self, GenerationError> {
        match self {
            Self::JsonFragments(value) => {
                Ok(Self::Json(StructuredValue::from_bytes(value.as_bytes())?))
            }
            Self::AudioFragments(value) => Ok(Self::Audio(value.finish()?)),
            other => Ok(other),
        }
    }
}
