//! Selected native Messages types, not a second semantic model or an execution API.
//! `wire` holds the DTOs; `request`, `response`, `items` and `settings` map them to
//! and from the IR; `client` admits and delivers downstream values. Accounting and
//! terminal mappings are shared by those directions, not owned by a delivery front end.
use crate::protocol::failure::CarrierReason;
mod accounting;
pub(crate) mod carriers;
pub mod client;
pub mod sse;
pub use sse::{GenerationSseDecoder, MessageSseDecoder};
mod target;
pub use target::{Dialect, TargetContract};
mod items;
mod media;
mod request;
mod response;
pub(crate) mod service;
mod settings;
mod shape;
pub(crate) mod strict_schema;
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
    pub fn semantic_contract(self) -> crate::lowering::disposition::Dispositions {
        use crate::lowering::disposition::Dispositions;
        (Dispositions::empty())
            .with_native(
                crate::semantic::task::generation::Feature::Compaction,
                false,
            )
            .with_native(
                crate::semantic::task::generation::Feature::Instructions,
                true,
            )
            // Mid-conversation system messages need model-level binding evidence.
            .with_native(
                crate::semantic::task::generation::Feature::OrderedInstructions,
                false,
            )
            .with_native(
                crate::semantic::task::generation::Feature::Temperature,
                false,
            )
            .with_native(
                crate::semantic::task::generation::Feature::MaxOutputTokens,
                true,
            )
            .with_native(crate::semantic::task::generation::Feature::Tools, true)
            .with_native(
                crate::semantic::task::generation::Feature::CustomTools,
                false,
            )
            .with_native(
                crate::semantic::task::generation::Feature::StructuredArguments,
                true,
            )
            .with_native(
                crate::semantic::task::generation::Feature::StructuredToolResult,
                false,
            )
            .with_native(
                crate::semantic::task::generation::Feature::ToolExecutionReport,
                true,
            )
            .with_native(
                crate::semantic::task::generation::Feature::ToolResultError,
                true,
            )
            .with_native(
                crate::semantic::task::generation::Feature::ToolResultMedia,
                true,
            )
            .with_native(
                crate::semantic::task::generation::Feature::TextMetadata,
                false,
            )
            .with_native(crate::semantic::task::generation::Feature::TopP, false)
            .with_native(crate::semantic::task::generation::Feature::TopK, false)
            .with_native(
                crate::semantic::task::generation::Feature::StopSequences,
                false,
            )
            .with_native(
                crate::semantic::task::generation::Feature::PresencePenalty,
                false,
            )
            .with_native(
                crate::semantic::task::generation::Feature::FrequencyPenalty,
                false,
            )
            .with_native(crate::semantic::task::generation::Feature::Seed, false)
            .with_native(crate::semantic::task::generation::Feature::Logprobs, false)
            .with_native(crate::semantic::task::generation::Feature::Verbosity, false)
            .with_native(
                crate::semantic::task::generation::Feature::Truncation,
                false,
            )
            .with_native(
                crate::semantic::task::generation::Feature::StructuredOutput,
                true,
            )
            .with_native(crate::semantic::task::generation::Feature::Reasoning, true)
            .with_native(
                crate::semantic::task::generation::Feature::ResponseEffort,
                true,
            )
            .with_native(
                crate::semantic::task::generation::Feature::ReasoningBudget,
                true,
            )
            .with_native(crate::semantic::task::generation::Feature::ImageInput, true)
            .with_native(
                crate::semantic::task::generation::Feature::AudioInput,
                false,
            )
            .with_native(
                crate::semantic::task::generation::Feature::VideoInput,
                false,
            )
            .with_native(
                crate::semantic::task::generation::Feature::AudioOutput,
                false,
            )
            .with_native(
                crate::semantic::task::generation::Feature::AudioHistory,
                false,
            )
            .with_native(crate::semantic::task::generation::Feature::FileInput, true)
            .with_native(
                crate::semantic::task::generation::Feature::DocumentContext,
                false,
            )
            .with_native(
                crate::semantic::task::generation::Feature::ParallelTools,
                true,
            )
            .with_native(
                crate::semantic::task::generation::Feature::StrictTools,
                false,
            )
            .with_native(
                crate::semantic::task::generation::Feature::ToolExamples,
                false,
            )
            .with_native(
                crate::semantic::task::generation::Feature::ToolChanges,
                false,
            )
            .with_native(
                crate::semantic::task::generation::Feature::ConditionalToolStrictness,
                true,
            )
            .with_native(
                crate::semantic::task::generation::Feature::ExplicitCache,
                false,
            )
            .with_native(
                crate::semantic::task::generation::Feature::CachedContext,
                false,
            )
            .with_native(crate::semantic::task::generation::Feature::Safety, false)
            .with_native(
                crate::semantic::task::generation::Feature::AutomaticPrefixCache,
                false,
            )
            .with_native(
                crate::semantic::task::generation::Feature::EagerToolInput,
                true,
            )
            .with_native(
                crate::semantic::task::generation::Feature::ReplayMismatch,
                false,
            )
    }
    /// Codec support does not activate strict tools on an existing binding.
    pub fn representation_contract(self) -> crate::lowering::disposition::Dispositions {
        let mut contract = self.semantic_contract();
        contract.set_native(
            crate::semantic::task::generation::Feature::ReplayMismatch,
            true,
        );
        contract.set_native(
            crate::semantic::task::generation::Feature::MessageEffort,
            true,
        );
        contract.set_native(
            crate::semantic::task::generation::Feature::StrictTools,
            true,
        );
        contract.set_native(
            crate::semantic::task::generation::Feature::ToolExamples,
            true,
        );
        contract.set_native(
            crate::semantic::task::generation::Feature::DocumentContext,
            true,
        );
        contract.set_native(
            crate::semantic::task::generation::Feature::ExplicitCache,
            true,
        );
        contract.set_native(
            crate::semantic::task::generation::Feature::AutomaticPrefixCache,
            true,
        );
        contract.set_native(
            crate::semantic::task::generation::Feature::StopSequences,
            true,
        );
        contract.set_native(
            crate::semantic::task::generation::Feature::OrderedInstructions,
            true,
        );
        // Without the binding's carrier, changes collapse under a named rule.
        contract.set_native(
            crate::semantic::task::generation::Feature::ToolChanges,
            true,
        );
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
