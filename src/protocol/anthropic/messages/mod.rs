//! Selected native Messages types, not a second semantic model or an execution API.
//! `wire` holds the DTOs; `request`, `response`, `items` and `settings` map them to
//! and from the IR; `client` admits and delivers downstream values. Accounting and
//! terminal mappings are shared by those directions, not owned by a delivery front end.
use crate::protocol::failure::CarrierReason;
mod accounting;
mod carriers;
pub mod client;
pub mod sse;
pub use sse::{GenerationSseDecoder, MessageSseDecoder};
mod target;
pub use carriers::Carriers;
pub use target::{Dialect, SamplingPolicy, TargetCaps};
mod items;
mod media;
mod request;
mod response;
mod service;
mod settings;
mod shape;
mod strict_schema;
mod terminal;
pub mod wire;
use crate::protocol::CodecError;
pub use media::{Media, ResultBlock, ResultContent, Source};
pub use request::RequestRule;
use serde_json::Value;
pub use wire::*;

/// Selected stable HTTP contract, shared by ingress and trusted upstream preparation.
pub const API_VERSION: &str = "2026-10-21";

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
            temperature: false,
            max_output_tokens: true,
            tools: true,
            custom_tools: false,
            structured_arguments: true,
            structured_tool_results: false,
            tool_execution_reports: true,
            tool_result_errors: true,
            tool_result_media: true,
            text_metadata: false,
            top_p: false,
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
            document_context: false,
            parallel_tool_calls: true,
            strict_tools: false,
            tool_examples: false,
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
        contract.tool_examples = true;
        contract.document_context = true;
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
            return Err(CodecError::Unsupported(CarrierReason::Schema));
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
        if message
            .content
            .iter()
            .any(|b| matches!(b, Block::PartialToolUse { .. }))
        {
            return Err(CodecError::Unsupported(CarrierReason::Unavailable));
        }
        let wire = message.wire();
        shape::bounded(&wire)?;
        Ok(wire)
    }
    /// Validates one event's shape only; ordering/finality belongs to the stream decoder.
    pub fn decode_event(self, name: &str, bytes: &[u8]) -> Result<Event, CodecError> {
        let value = shape::parse(bytes)?;
        if value.get("type").and_then(Value::as_str) != Some(name) {
            return Err(CodecError::invalid());
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
