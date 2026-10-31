//! Canonical identities and public aliases. Semantic vocabulary has no topology dependency.
use super::{ModelId, TaskKind};
pub use crate::lowering::disposition::Dispositions;
use crate::protocol::conversion::generation::ReportedFactPolicy;
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CanonicalModel {
    pub id: ModelId,
    pub task: TaskKind,
    pub contract: Dispositions,
}
impl super::PublicModel {
    pub fn new(
        id: ModelId,
        canonical_model: ModelId,
        task: TaskKind,
        route: super::RouteId,
        contract: Dispositions,
    ) -> Self {
        Self {
            id,
            canonical_model,
            task,
            route,
            contract,
            reported_facts: ReportedFactPolicy::Faithful,
        }
    }
}
