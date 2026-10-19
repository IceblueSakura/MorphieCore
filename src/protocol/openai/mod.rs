//! Pure codecs for explicitly admitted OpenAI operations.
pub mod adapter;
pub use adapter::{Adapter, Dialect};
pub(crate) mod bailian;
pub mod chat;
pub(crate) mod content;
pub mod dialect;
pub mod events;
pub mod lowering;
pub mod responses;
mod target;
pub use target::TargetCaps;
mod common;
pub(crate) use common::DecodeDirection;
pub use common::IgnoredFields;
pub mod embeddings;
mod function_tools;
pub mod images;
mod reasoning;
mod service;
mod settings;
pub mod speech;
pub(crate) mod static_response;
pub mod stream;
mod strict_schema;
mod terminal;
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
    pub(crate) adaptation: crate::protocol::openai::dialect::adaptation::Adaptation,
    pub(crate) semantic: std::borrow::Cow<'a, GenerationRequest>,
    pub(crate) fidelity: &'a FidelityRecords,
    pub(crate) profile: Profile,
    pub(crate) projection: Vec<crate::protocol::openai::lowering::projection::ProjectionStage>,
}
impl RequestRepresentation<'_> {
    /// Final candidate-local value; a changed input or target needs fresh lowering.
    pub fn semantic(&self) -> &GenerationRequest {
        &self.semantic
    }
    pub fn projection(&self) -> &[crate::protocol::openai::lowering::projection::ProjectionStage] {
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
    pub(crate) adaptation: crate::protocol::openai::dialect::adaptation::Adaptation,
    pub(crate) semantic: std::borrow::Cow<'a, GenerationResponse>,
    pub(crate) fidelity: &'a FidelityRecords,
    pub(crate) metadata: std::borrow::Cow<'a, ResponseMetadata>,
    pub(crate) profile: Profile,
    pub(crate) projection: Vec<crate::protocol::openai::lowering::projection::ProjectionStage>,
    /// Owned replay manifest emitted by this client encoder, never a later body patch.
    pub(crate) attachment: Option<crate::protocol::attachments::Carrier>,
}
impl ResponseRepresentation<'_> {
    pub fn semantic(&self) -> &GenerationResponse {
        &self.semantic
    }
    pub fn metadata(&self) -> &ResponseMetadata {
        &self.metadata
    }
    pub fn projection(&self) -> &[crate::protocol::openai::lowering::projection::ProjectionStage] {
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
        contract: crate::protocol::openai::TargetCaps,
    ) -> Result<ResponseRepresentation<'_>, crate::lowering::generation::RepresentationError> {
        use crate::protocol::openai::lowering::{
            self as generation, projection::MAX_PROJECTION_STAGES,
        };
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
