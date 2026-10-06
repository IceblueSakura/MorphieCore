//! Fixed route ordering and the public model contract.

use crate::{
    lowering::generation::ReportedFactPolicy,
    topology::GenerationSemanticContract,
    topology::{EndpointId, ModelId, RouteId, TaskKind},
};

pub const MAX_ROUTE_CANDIDATES: usize = 64;
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, strum::EnumString, strum::IntoStaticStr)]
#[strum(serialize_all = "snake_case")]
pub enum CandidatePolicy {
    #[default]
    RequireAll,
    SkipUnrepresentable,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, strum::EnumString, strum::IntoStaticStr)]
#[strum(serialize_all = "snake_case")]
pub enum FallbackPolicy {
    #[default]
    Disabled,
    BeforeCommit,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RoutePolicy {
    pub candidates: CandidatePolicy,
    pub fallback: FallbackPolicy,
    pub max_attempts: usize,
}
impl Default for RoutePolicy {
    fn default() -> Self {
        Self {
            candidates: CandidatePolicy::RequireAll,
            fallback: FallbackPolicy::Disabled,
            max_attempts: 4,
        }
    }
}

/// Ordered candidate endpoints. The order is fixed at compile time; planning,
/// retry or fallback never expand or reorder it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Route {
    pub id: RouteId,
    pub task: TaskKind,
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
    pub task: TaskKind,
    pub route: RouteId,
    pub contract: GenerationSemanticContract,
    /// Public context admission is not a property of model intelligence.
    pub standard_context: bool,
    pub reported_facts: ReportedFactPolicy,
}
