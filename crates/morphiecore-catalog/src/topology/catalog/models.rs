//! Explicit canonical declarations. Upstream aliases and public routes never create models.
use crate::{
    lowering::disposition::Dispositions,
    topology::{CanonicalModel, ModelId},
};
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelDefinition {
    pub custom_tools: bool,
    pub video: bool,
    pub id: &'static str,
    pub images: bool,
    pub files: bool,
    pub strict_tools: bool,
    pub released_at: u64,
    pub developer: &'static str,
}
// Official publication dates normalized to UTC midnight, not router listing timestamps.
pub fn contract(id: &str) -> Dispositions {
    if let Some(binding) = super::messages_bindings().iter().find(|b| b.model == id) {
        // Shared public admission is narrower than the model's native capabilities.
        // Do not copy Messages-only cache support into OpenAI-compatible bindings.
        return binding.public_model().contract;
    }
    let definition = super::data::models()
        .iter()
        .find(|m| m.id == id)
        .expect("explicit canonical reference");
    (Dispositions::text_images())
        .with_native(
            crate::semantic::task::generation::Feature::CustomTools,
            definition.custom_tools,
        )
        .with_native(
            crate::semantic::task::generation::Feature::ImageInput,
            definition.images,
        )
        .with_native(
            crate::semantic::task::generation::Feature::FileInput,
            definition.files,
        )
        // Sources: MiniCPM-V docs/api.md and the Zhipu Chat reference.
        // Media policy: docs/architecture/adaptation-and-dialects.md#media.
        .with_native(
            crate::semantic::task::generation::Feature::VideoInput,
            definition.video,
        )
        .with_native(
            crate::semantic::task::generation::Feature::StrictTools,
            definition.strict_tools,
        )
        // Admission still needs the media kind's input. Only Responses carries tool
        // media; standard Chat has no carrier, so Chat targets reject it.
        .with_native(
            crate::semantic::task::generation::Feature::ToolResultMedia,
            true,
        )
}
pub fn canonical_models() -> Vec<CanonicalModel> {
    super::data::models()
        .iter()
        .map(|m| CanonicalModel {
            id: ModelId::new(m.id).expect("static id"),
            contract: contract(m.id),
        })
        .collect()
}
