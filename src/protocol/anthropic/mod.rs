//! Selected native wire types, not a second semantic model or an execution API.
mod accounting;
mod adapter;
mod carriers;
pub mod client;
pub mod client_events;
pub use adapter::{Dialect, TargetCaps};
pub use carriers::Carriers;
mod request;
mod response;
mod semantic;
mod semantic_intake;
mod semantic_request;
mod service;
mod shape;
mod sse;
mod strict_schema;
pub use sse::MessageSseDecoder;
mod events;
mod media;
use crate::protocol::CodecError;
pub use events::GenerationSseDecoder;
pub use media::{Media, ResultBlock, ResultContent, Source};
pub use request::*;
pub use response::*;
pub use semantic::NativeProjection;
pub use semantic_request::RequestRule;
use serde_json::Value;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Profile {
    AdaptiveTextTools,
}
impl Profile {
    /// Default binding admission, kept separate from codec carrier availability.
    pub fn semantic_contract(
        self,
    ) -> crate::semantic::task::generation::GenerationSemanticContract {
        use crate::semantic::task::generation::GenerationSemanticContract;
        GenerationSemanticContract {
            compaction: false,
            instructions: true,
            // Mid-conversation system messages need model-level binding evidence.
            ordered_instructions: false,
            // Accepted, then dropped by `RequestRule::OmitSampling`.
            temperature: true,
            max_output_tokens: true,
            tools: true,
            custom_tools: false,
            structured_arguments: true,
            structured_tool_results: false,
            tool_execution_reports: true,
            tool_result_errors: true,
            tool_result_media: true,
            text_metadata: false,
            top_p: true,
            top_k: false,
            stop_sequences: false,
            presence_penalty: false,
            frequency_penalty: false,
            seed: false,
            logprobs: false,
            verbosity: false,
            truncation: false,
            structured_output: true,
            reasoning: true,
            response_effort: true,
            image_input: true,
            audio_input: false,
            video_input: false,
            audio_output: false,
            audio_history: false,
            file_input: true,
            parallel_tool_calls: true,
            strict_tools: false,
            tool_changes: false,
            normalization_default_tools: false,
            explicit_cache: false,
            cached_context: false,
            safety: false,
            automatic_prefix_cache: false,
        }
    }
    /// Codec support does not activate strict tools on an existing binding.
    pub fn representation_contract(
        self,
    ) -> crate::semantic::task::generation::GenerationSemanticContract {
        let mut contract = self.semantic_contract();
        contract.strict_tools = true;
        contract.explicit_cache = true;
        contract.automatic_prefix_cache = true;
        contract.stop_sequences = true;
        contract.ordered_instructions = true;
        // Without the binding's carrier, changes collapse under a named rule.
        contract.tool_changes = true;
        contract
    }
    pub fn decode_request(self, bytes: &[u8]) -> Result<Request, CodecError> {
        Request::read(&shape::parse(bytes)?)
    }
    pub fn encode_request(self, request: &Request) -> Result<Value, CodecError> {
        request.validate()?;
        if !strict_schema::admits_request(request) {
            return Err(CodecError::Unsupported(
                "Anthropic strict tool schema".into(),
            ));
        }
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
