//! Canonical identities and public aliases. Semantic vocabulary has no topology dependency.
use super::{ModelId, TaskKind};
use crate::lowering::generation::ReportedFactPolicy;
pub use crate::semantic::task::generation::GenerationSemanticContract;
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CanonicalModel {
    pub id: ModelId,
    pub task: TaskKind,
    pub contract: GenerationSemanticContract,
}
impl super::PublicModel {
    pub fn new(
        id: ModelId,
        canonical_model: ModelId,
        task: TaskKind,
        route: super::RouteId,
        contract: GenerationSemanticContract,
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
