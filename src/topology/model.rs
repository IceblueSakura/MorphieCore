//! Canonical identities and public aliases. Semantic vocabulary has no topology dependency.
use super::ModelId;
pub use crate::lowering::disposition::Dispositions;
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CanonicalModel {
    pub id: ModelId,
    pub contract: Dispositions,
}
impl super::PublicModel {
    pub fn new(
        id: ModelId,
        canonical_model: ModelId,
        route: super::RouteId,
        contract: Dispositions,
    ) -> Self {
        Self {
            id,
            canonical_model,
            route,
            contract,
        }
    }
}
