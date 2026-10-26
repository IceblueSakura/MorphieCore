//! Embedding envelopes and deployment bounds, separate from task values and source facts.
use crate::semantic::{context::ClientIdentityHints, task::embedding::*, value::Presence};

pub const MAX_INPUTS: usize = 128;
pub const MAX_INPUT_BYTES: usize = 256 << 10;
pub const MAX_DIMENSIONS: usize = 8192;
pub const MAX_VECTOR_VALUES: usize = 60_000;
pub const MAX_TOKEN_IDS: usize = 60_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Encoding {
    Float,
    Base64,
}
impl Encoding {
    pub fn name(self) -> &'static str {
        match self {
            Self::Float => "float",
            Self::Base64 => "base64",
        }
    }
}
#[derive(Clone, Eq, PartialEq)]
pub struct Request {
    pub model: String,
    pub task: EmbeddingRequest,
    pub encoding: Option<Encoding>,
    pub identity: ClientIdentityHints,
}
impl std::fmt::Debug for Request {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EmbeddingRequest")
            .field("model", &self.model)
            .field("task", &self.task)
            .field("encoding", &self.encoding)
            .field("identity", &"[redacted]")
            .finish()
    }
}
impl Request {
    pub fn new(model: impl Into<String>, task: EmbeddingRequest) -> Self {
        Self {
            model: model.into(),
            task,
            encoding: None,
            identity: ClientIdentityHints::default(),
        }
    }
    pub fn validate(&self) -> Result<(), EmbeddingError> {
        if !valid_model(&self.model)
            || self.identity.safety_identifier != Presence::Absent
            || self.identity.user == Presence::Null
            || self
                .identity
                .user
                .value()
                .is_some_and(|s| s.is_empty() || s.len() > 256)
        {
            return Err(EmbeddingError);
        }
        Ok(())
    }
}
pub(crate) fn valid_model(model: &str) -> bool {
    !model.is_empty()
        && model.len() <= 256
        && !model.chars().any(|c| c.is_control() || c.is_whitespace())
}
#[derive(Clone, Eq, PartialEq)]
pub struct TargetRequest {
    pub task: EmbeddingRequest,
    pub encoding: Encoding,
    pub user: Option<String>,
}
impl std::fmt::Debug for TargetRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EmbeddingTarget")
            .field("task", &self.task)
            .field("encoding", &self.encoding)
            .field("user", &self.user.as_ref().map(|_| "[redacted]"))
            .finish()
    }
}
/// Only upstream observations; bound identity and derived counters never enter this type.
#[derive(Clone, Eq, PartialEq)]
pub struct Response {
    pub model: Option<String>,
    pub task: EmbeddingResponse,
    pub id: Option<String>,
}
impl std::fmt::Debug for Response {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EmbeddingResponse")
            .field("model", &self.model)
            .field("task", &self.task)
            .field("id", &self.id.as_ref().map(|_| "[redacted]"))
            .finish()
    }
}
impl Response {
    pub fn validate(&self) -> Result<(), EmbeddingError> {
        if self.model.as_ref().is_some_and(|m| !valid_model(m))
            || self
                .id
                .as_ref()
                .is_some_and(|s| s.is_empty() || s.len() > 256 || s.chars().any(char::is_control))
        {
            return Err(EmbeddingError);
        }
        let vectors = self.task.vectors();
        if vectors.len() > MAX_INPUTS
            || vectors.iter().any(|v| v.values().len() > MAX_DIMENSIONS)
            || vectors
                .iter()
                .try_fold(0usize, |n, v| n.checked_add(v.values().len()))
                .is_none_or(|n| n > MAX_VECTOR_VALUES)
        {
            return Err(EmbeddingError);
        }
        Ok(())
    }
    pub fn model(&self) -> Option<&str> {
        self.model.as_deref()
    }
    pub fn vectors(&self) -> &[EmbeddingVector] {
        self.task.vectors()
    }
    pub fn validate_for(&self, request: &EmbeddingRequest) -> Result<(), EmbeddingError> {
        self.validate()?;
        self.task.validate_for(request)
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeliveryUsage {
    pub input_tokens: u64,
    pub total_tokens: u64,
}
/// A validated client view, never reused as an upstream report.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Delivery {
    pub(crate) model: String,
    pub(crate) vectors: Vec<EmbeddingVector>,
    pub(crate) usage: DeliveryUsage,
}
impl Delivery {
    pub fn model(&self) -> &str {
        &self.model
    }
    pub fn vectors(&self) -> &[EmbeddingVector] {
        &self.vectors
    }
    pub fn usage(&self) -> &DeliveryUsage {
        &self.usage
    }
}
