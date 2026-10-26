//! Standalone embedding codec dispatch.
use crate::protocol::embedding::*;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UpstreamAdapter {
    Standard,
    DashScope,
    OpenRouter,
    Google,
}
impl UpstreamAdapter {
    pub fn encode_request(
        self,
        request: &TargetRequest,
        model: &str,
    ) -> Result<serde_json::Value, crate::protocol::CodecError> {
        match self {
            Self::Google => crate::protocol::google::embeddings::encode_request(request, model),
            _ => crate::protocol::openai::embeddings::encode_request(request, model),
        }
    }
    pub fn decode_response(self, bytes: &[u8]) -> Result<Response, crate::protocol::CodecError> {
        self.decode_response_as(bytes, Encoding::Float)
    }
    pub fn decode_response_as(
        self,
        bytes: &[u8],
        encoding: Encoding,
    ) -> Result<Response, crate::protocol::CodecError> {
        match self {
            Self::Google => crate::protocol::google::embeddings::decode_response(bytes),
            Self::Standard | Self::DashScope | Self::OpenRouter => {
                use crate::protocol::openai::embeddings::{Dialect, decode_response_with};
                let dialect = match self {
                    Self::Standard => Dialect::Standard,
                    Self::DashScope => Dialect::DashScope,
                    Self::OpenRouter => Dialect::OpenRouter,
                    Self::Google => unreachable!("handled native API"),
                };
                decode_response_with(bytes, dialect, encoding)
            }
        }
    }
}
