//! Pure codecs for explicitly admitted OpenAI operations.
//! `chat`/`responses` own one whole request and response each; `accounting` and
//! `context` hold what both profiles report identically. `events` owns payload
//! state/grammar; SSE framing stays with each protocol.
pub mod codec;
pub use codec::GenerationCodec;
pub use dialect::Dialect;
pub mod cache_affinity;
pub mod chat;
pub mod content;
pub mod dialect;
pub mod events;
pub mod lowering;
pub mod responses;
mod target;
pub use target::TargetContract;
pub mod accounting;
mod shape;
pub use context::validate_identity;
pub use shape::DecodeDirection;
pub use shape::IgnoredFields;
pub mod context;
pub mod embeddings;
mod function_tools;
pub mod images;
pub mod obfuscation;
mod reasoning;
pub mod service;
mod settings;
pub mod speech;
mod strict_schema;
pub mod terminal;
#[cfg(test)]
mod tool_results_test;
pub mod transcription;

pub use crate::{CodecError, DecodedRequest, DecodedResponse, ResponseMetadata};
use crate::{
    fidelity::FidelityRecords,
    semantic::task::generation::{GenerationRequest, GenerationResponse},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GenerationApi {
    Chat,
    Responses,
}
/// Only lowering can construct an encoding input; codecs cannot bypass representability.
pub struct RequestRepresentation<'a> {
    pub adaptation: crate::openai::dialect::adaptation::Adaptation,
    pub semantic: std::borrow::Cow<'a, GenerationRequest>,
    pub fidelity: &'a FidelityRecords,
    pub profile: GenerationApi,
    pub projection: Vec<crate::conversion::generation::Effect>,
}
impl RequestRepresentation<'_> {
    /// Final candidate-local value; a changed input or target needs fresh lowering.
    pub fn semantic(&self) -> &GenerationRequest {
        &self.semantic
    }
    pub fn projection(&self) -> &[crate::conversion::generation::Effect] {
        &self.projection
    }
    pub fn profile(&self) -> GenerationApi {
        self.profile
    }
    pub fn features(
        &self,
    ) -> std::collections::BTreeMap<crate::semantic::task::generation::Feature, usize> {
        self.semantic.features()
    }
}
pub struct ResponseRepresentation<'a> {
    pub adaptation: crate::openai::dialect::adaptation::Adaptation,
    pub semantic: std::borrow::Cow<'a, GenerationResponse>,
    pub fidelity: &'a FidelityRecords,
    pub metadata: std::borrow::Cow<'a, ResponseMetadata>,
    pub profile: GenerationApi,
    pub projection: Vec<crate::conversion::generation::Effect>,
    pub projection_steps: usize,
}
impl ResponseRepresentation<'_> {
    pub fn semantic(&self) -> &GenerationResponse {
        &self.semantic
    }
    pub fn metadata(&self) -> &ResponseMetadata {
        &self.metadata
    }
    pub fn projection(&self) -> &[crate::conversion::generation::Effect] {
        &self.projection
    }
    pub fn requirements(
        &self,
    ) -> crate::semantic::task::generation::GenerationResponseRequirements {
        crate::semantic::task::generation::GenerationResponseRequirements::derive(&self.semantic)
    }
    /// Revalidate the final value at a new fixed target. Never return to a hidden
    /// original to restore information omitted by an earlier stage.
    pub fn reproject(
        &self,
        profile: GenerationApi,
        contract: crate::openai::TargetContract,
    ) -> Result<ResponseRepresentation<'_>, crate::ConversionFailure> {
        use crate::openai::lowering::{self as generation, projection::MAX_PROJECTION_STAGES};
        if self.projection_steps >= MAX_PROJECTION_STAGES {
            return Err(generation::RepresentationError::ProjectionLimit.into());
        }
        let mut next = generation::lower_response(
            &self.semantic,
            self.fidelity,
            &self.metadata,
            profile,
            contract,
        )?;
        if self.projection_steps.saturating_add(next.projection_steps) > MAX_PROJECTION_STAGES {
            return Err(generation::RepresentationError::ProjectionLimit.into());
        }
        next.projection_steps += self.projection_steps;
        let mut stages = self.projection.clone();
        stages.append(&mut next.projection);
        next.projection = stages;
        Ok(next)
    }
}
