//! Pure codecs for explicitly admitted OpenAI operations.
mod accounting_shapes;
pub(crate) mod adapter_shapes;
pub mod chat;
mod chat_annotations;
mod chat_audio;
pub mod chat_envelope;
pub(crate) mod chat_logprobs;
mod chat_reasoning;
pub mod chat_sse;
pub(crate) mod citations;
mod common;
pub mod envelope;
pub mod events;
mod file;
mod function_tools;
mod image;
pub mod images;
mod inference_shapes;
pub(crate) mod input_audio;
pub(crate) mod json;
mod reasoning;
pub mod responses;
mod settings;
pub mod speech;
pub mod sse;
pub(crate) mod static_response;
mod terminal;
mod text;
#[cfg(test)]
mod tool_results_test;
pub mod transcription;

pub use crate::protocol::{CodecError, DecodedRequest, DecodedResponse, ResponseMetadata};
use crate::{
    protocol::fidelity::FidelityRecords,
    semantic::task::generation::{GenerationRequest, GenerationResponse},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Profile {
    Chat,
    Responses,
}
/// Only lowering can construct an encoding input; codecs cannot bypass representability.
pub struct RequestRepresentation<'a> {
    pub(crate) adaptation: crate::protocol::adaptation::Adaptation,
    pub(crate) semantic: std::borrow::Cow<'a, GenerationRequest>,
    pub(crate) fidelity: &'a FidelityRecords,
    pub(crate) profile: Profile,
    pub(crate) projection: Vec<crate::lowering::projection::ProjectionStage>,
}
impl RequestRepresentation<'_> {
    /// Final candidate-local value; a changed input or target needs fresh lowering.
    pub fn semantic(&self) -> &GenerationRequest {
        &self.semantic
    }
    pub fn projection(&self) -> &[crate::lowering::projection::ProjectionStage] {
        &self.projection
    }
    pub fn profile(&self) -> Profile {
        self.profile
    }
    pub fn requirements(&self) -> crate::semantic::task::generation::GenerationRequirements {
        crate::semantic::task::generation::GenerationRequirements::derive(&self.semantic)
    }
}
pub struct ResponseRepresentation<'a> {
    pub(crate) adaptation: crate::protocol::adaptation::Adaptation,
    pub(crate) semantic: std::borrow::Cow<'a, GenerationResponse>,
    pub(crate) fidelity: &'a FidelityRecords,
    pub(crate) metadata: std::borrow::Cow<'a, ResponseMetadata>,
    pub(crate) profile: Profile,
    pub(crate) projection: Vec<crate::lowering::projection::ProjectionStage>,
}
impl ResponseRepresentation<'_> {
    pub fn semantic(&self) -> &GenerationResponse {
        &self.semantic
    }
    pub fn metadata(&self) -> &ResponseMetadata {
        &self.metadata
    }
    pub fn projection(&self) -> &[crate::lowering::projection::ProjectionStage] {
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
        profile: Profile,
        contract: crate::lowering::generation::GenerationRepresentationContract,
    ) -> Result<ResponseRepresentation<'_>, crate::lowering::generation::RepresentationError> {
        use crate::lowering::{generation, projection::MAX_PROJECTION_STAGES};
        if self.projection.len() >= MAX_PROJECTION_STAGES {
            return Err(generation::RepresentationError::ProjectionLimit);
        }
        let mut next = generation::lower_response(
            &self.semantic,
            self.fidelity,
            &self.metadata,
            profile,
            contract,
        )?;
        if self.projection.len().saturating_add(next.projection.len()) > MAX_PROJECTION_STAGES {
            return Err(generation::RepresentationError::ProjectionLimit);
        }
        let mut stages = self.projection.clone();
        stages.append(&mut next.projection);
        next.projection = stages;
        Ok(next)
    }
}
