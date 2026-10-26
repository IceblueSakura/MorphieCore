//! Canonical identities and public aliases. Semantic vocabulary has no topology dependency.
use super::{ModelId, TaskKind};
use crate::protocol::conversion::generation::ReportedFactPolicy;
pub use crate::semantic::task::generation::GenerationFeatureSet;
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CanonicalModel {
    pub id: ModelId,
    pub task: TaskKind,
    pub contract: GenerationFeatureSet,
}
impl super::PublicModel {
    pub fn new(
        id: ModelId,
        canonical_model: ModelId,
        task: TaskKind,
        route: super::RouteId,
        contract: GenerationFeatureSet,
    ) -> Self {
        Self {
            id,
            canonical_model,
            task,
            route,
            contract,
            standard_context: true,
            reported_facts: ReportedFactPolicy::Faithful,
        }
    }
}
