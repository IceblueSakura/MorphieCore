//! Embedding envelopes keep model reports and client identity outside task content.
pub use crate::lowering::embeddings::{Contract, DimensionSupport};
use crate::semantic::{context::ClientIdentityHints, task::embedding::*, value::Presence};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Encoding {
    Float,
}
/// Trusted response-report rules never expand the public standard request profile.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Profile {
    Standard,
    DashScope,
    OpenRouter,
}
impl Profile {
    pub fn encode_request(
        self,
        request: &Request,
        model: &str,
    ) -> Result<serde_json::Value, crate::protocol::CodecError> {
        // Resolve the selected profile's float default in a typed target copy, never after encoding.
        let mut target = request.clone();
        if target.encoding == Presence::Absent {
            target.encoding = Presence::Value(Encoding::Float);
        }
        crate::protocol::openai::embeddings::encode_request(&target, model)
    }
    pub fn decode_response(self, bytes: &[u8]) -> Result<Response, crate::protocol::CodecError> {
        crate::protocol::openai::embeddings::decode_response_with(bytes, self)
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct Request {
    pub model: String,
    pub task: EmbeddingRequest,
    pub encoding: Presence<Encoding>,
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
            encoding: Presence::Absent,
            identity: ClientIdentityHints::default(),
        }
    }
}
#[derive(Clone, Eq, PartialEq)]
pub struct Response {
    pub model: String,
    pub task: EmbeddingResponse,
    pub id: Presence<String>,
}
impl std::fmt::Debug for Response {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EmbeddingResponse")
            .field("model", &self.model)
            .field("task", &self.task)
            .field("id", &"[redacted]")
            .finish()
    }
}
impl Response {
    pub fn validate(&self) -> Result<(), EmbeddingError> {
        self.task.validate()?;
        if self.model.is_empty()
            || self.model.len() > 256
            || self
                .model
                .chars()
                .any(|c| c.is_control() || c.is_whitespace())
            || self.id == Presence::Null
            || self
                .id
                .value()
                .is_some_and(|s| s.is_empty() || s.len() > 256 || s.chars().any(char::is_control))
        {
            return Err(EmbeddingError);
        }
        Ok(())
    }
    pub fn model(&self) -> &str {
        &self.model
    }
    pub fn vectors(&self) -> &[EmbeddingVector] {
        self.task.vectors()
    }
    pub fn validate_for(&self, request: &EmbeddingRequest) -> Result<(), EmbeddingError> {
        self.validate()?;
        self.task.validate_for(request)
    }
}
