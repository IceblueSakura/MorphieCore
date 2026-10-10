//! Selected native wire types, not a second semantic model or an execution API.
mod request;
mod response;
mod semantic;
mod semantic_intake;
mod semantic_request;
mod shape;
mod target;
use crate::protocol::CodecError;
pub use request::*;
pub use response::*;
pub use semantic::NativeProjection;
pub(crate) use semantic_request::prefix_digest;
use serde_json::Value;
pub use target::ReplayTarget;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Profile {
    AdaptiveTextTools,
}
impl Profile {
    /// The implemented semantic slice, not a Provider/model capability inventory.
    pub fn semantic_contract(
        self,
    ) -> crate::semantic::task::generation::GenerationSemanticContract {
        use crate::semantic::task::generation::GenerationSemanticContract;
        GenerationSemanticContract {
            instructions: true,
            temperature: false,
            max_output_tokens: true,
            tools: true,
            custom_tools: false,
            structured_arguments: true,
            structured_tool_results: false,
            tool_execution_reports: true,
            tool_result_errors: true,
            tool_result_images: false,
            text_metadata: false,
            top_p: false,
            logprobs: false,
            verbosity: false,
            truncation: false,
            structured_output: false,
            reasoning: true,
            image_input: false,
            audio_input: false,
            audio_output: false,
            audio_history: false,
            file_input: false,
            parallel_tool_calls: true,
            strict_tools: false,
        }
    }
    pub fn decode_request(self, bytes: &[u8]) -> Result<Request, CodecError> {
        Request::read(&shape::parse(bytes)?)
    }
    pub fn encode_request(self, request: &Request) -> Result<Value, CodecError> {
        request.validate()?;
        let wire = request.wire();
        shape::bounded(&wire)?;
        Ok(wire)
    }
    pub fn decode_response(self, bytes: &[u8]) -> Result<Message, CodecError> {
        Message::read(&shape::parse(bytes)?, false)
    }
    pub fn encode_response(self, message: &Message) -> Result<Value, CodecError> {
        message.validate(false)?;
        let wire = message.wire();
        shape::bounded(&wire)?;
        Ok(wire)
    }
    /// Validates one event's shape only; ordering/finality belongs to the stream decoder.
    pub fn decode_event(self, name: &str, bytes: &[u8]) -> Result<Event, CodecError> {
        let value = shape::parse(bytes)?;
        if value.get("type").and_then(Value::as_str) != Some(name) {
            return Err(CodecError::Invalid("Anthropic event type"));
        }
        Event::read(&value)
    }
    pub fn encode_event(self, event: &Event) -> Result<Value, CodecError> {
        event.validate()?;
        let wire = event.wire();
        shape::bounded(&wire)?;
        Ok(wire)
    }
}
