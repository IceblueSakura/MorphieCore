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
impl ModelDefinition {
    pub(super) fn contract(&self) -> Dispositions {
        use crate::semantic::task::generation::Feature::*;
        Dispositions::text_images()
            .with_native(CustomTools, self.custom_tools)
            .with_native(ImageInput, self.images)
            .with_native(FileInput, self.files)
            // Sources: MiniCPM-V docs/api.md and the Zhipu Chat reference.
            .with_native(VideoInput, self.video)
            .with_native(StrictTools, self.strict_tools)
            // Admission still needs the media kind's input and target carrier.
            .with_native(ToolResultMedia, true)
    }
}
pub fn canonical_models() -> Vec<CanonicalModel> {
    super::data::models()
        .iter()
        .map(|m| CanonicalModel {
            id: ModelId::new(m.id).expect("static id"),
            contract: m.contract(),
        })
        .collect()
}
