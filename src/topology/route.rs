//! Fixed route ordering and the public model contract.

use crate::{
    topology::Dispositions,
    topology::{EndpointId, ModelId, RouteId},
};

pub const MAX_ROUTE_CANDIDATES: usize = 64;
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, strum::EnumString, strum::IntoStaticStr)]
#[strum(serialize_all = "snake_case")]
pub enum CandidatePolicy {
    #[default]
    RequireAll,
    SkipUnrepresentable,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RoutePolicy {
    pub candidates: CandidatePolicy,
    /// Model-local exchange deadline, still bounded by the service ceiling.
    pub total_timeout_ms: Option<u64>,
}
impl RoutePolicy {
    /// Skip unrepresentable alternatives during request-boundary selection.
    /// Dispatch never advances to another target.
    pub fn for_candidates(count: usize) -> Self {
        if count > 1 {
            Self {
                candidates: CandidatePolicy::SkipUnrepresentable,
                ..Self::default()
            }
        } else {
            Self::default()
        }
    }
}
impl Default for RoutePolicy {
    fn default() -> Self {
        Self {
            candidates: CandidatePolicy::RequireAll,
            total_timeout_ms: None,
        }
    }
}

/// Ordered candidate endpoints. The order is fixed at compile time; planning,
/// retry or fallback never expand or reorder it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Route {
    pub id: RouteId,
    pub endpoints: Vec<EndpointId>,
    pub policy: RoutePolicy,
}

/// A public model is an intentional downstream contract for one task, compiled
/// from trusted catalog facts and required to be satisfiable by its route.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublicModel {
    /// Client-visible label without a Provider prefix; upstream names stay on Endpoints.
    pub id: ModelId,
    /// Semantic identity shared by every member of the fixed Route.
    pub canonical_model: ModelId,
    pub route: RouteId,
    pub contract: Dispositions,
}
