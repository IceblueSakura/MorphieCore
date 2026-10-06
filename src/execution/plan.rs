//! Immutable execution plans derived from compiled topology.
//!
//! A plan is data, not a closure. Candidate order is the compiled route order;
//! deriving a plan can never expand, reorder or re-target it.

use crate::topology::{
    CandidatePolicy, CompiledTopology, Endpoint, EndpointId, ModelId, RoutePolicy, TaskKind,
};

/// Downstream interaction requirement, independent of upstream protocol.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Delivery {
    Json,
    Stream,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CandidatePlan {
    /// Position in the fixed input candidate list, not a credential locator.
    pub ordinal: usize,
    pub endpoint_id: EndpointId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExecutionPlan {
    pub model: ModelId,
    pub task: TaskKind,
    pub delivery: Delivery,
    /// Fixed route order. Candidates project independently from immutable input.
    pub candidates: Vec<CandidatePlan>,
    pub policy: RoutePolicy,
    pub rejections: Vec<CandidateRejection>,
}

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum PlanError {
    #[error("unknown public model label")]
    UnknownModel,
    #[error("task kind does not match the public model")]
    TaskMismatch,
    #[error("request does not satisfy public semantic admission")]
    InvalidRequest,
    #[error("no activated candidate can faithfully represent the request")]
    NoCandidate { rejections: Vec<CandidateRejection> },
    #[error("candidate rejected: {0:?}")]
    CandidateRejected(RejectionReason),
    #[error("candidate set exceeds planning budget")]
    CandidateLimit,
}

/// Closed reasons never retain request data, codec messages or credential locators.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RejectionReason {
    Streaming,
    BodyLimit,
    Semantic(crate::semantic::task::generation::GenerationFeature),
    Image,
    ReplayScope,
    Representation,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CandidateRejection {
    pub endpoint_id: EndpointId,
    pub reason: RejectionReason,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CandidateSelection {
    pub candidates: Vec<CandidatePlan>,
    pub rejections: Vec<CandidateRejection>,
}
pub const MAX_CANDIDATES: usize = crate::topology::route::MAX_ROUTE_CANDIDATES;
/// Pure preflight; use bounded counting, not a serialized copy of every candidate body.
pub fn representable(
    endpoint: &Endpoint,
    request: &crate::adapter::Request,
) -> Result<(), RejectionReason> {
    if (request.delivery.streaming()
        || endpoint
            .representation
            .adaptation
            .rules
            .responses_forced_stream)
        && !endpoint.execution.streaming
    {
        return Err(RejectionReason::Streaming);
    }
    let value = endpoint
        .adapter()
        .encode_request(request, &endpoint.upstream_model, &endpoint.representation)
        .map_err(|error| {
            use crate::{adapter::AdapterError, lowering::generation::RepresentationError};
            match error {
                AdapterError::Representation(RepresentationError::Admission(feature)) => {
                    RejectionReason::Semantic(feature)
                }
                AdapterError::Representation(RepresentationError::ImageInput) => {
                    RejectionReason::Image
                }
                AdapterError::Representation(RepresentationError::ReplayOrigin) => {
                    RejectionReason::ReplayScope
                }
                _ => RejectionReason::Representation,
            }
        })?;
    crate::semantic::value::json_size(&value, endpoint.execution.request_body_limit)
        .map_err(|_| RejectionReason::BodyLimit)?;
    Ok(())
}
/// Sole ordered selection mechanism for library plans and activated Gateway members.
pub fn select_candidates<'a>(
    request: &crate::adapter::Request,
    policy: CandidatePolicy,
    endpoints: impl IntoIterator<Item = &'a Endpoint>,
) -> Result<CandidateSelection, PlanError> {
    let mut selected = CandidateSelection {
        candidates: vec![],
        rejections: vec![],
    };
    for (index, endpoint) in endpoints.into_iter().enumerate() {
        if index >= MAX_CANDIDATES {
            return Err(PlanError::CandidateLimit);
        }
        match representable(endpoint, request) {
            Ok(()) => selected.candidates.push(CandidatePlan {
                ordinal: index,
                endpoint_id: endpoint.id.clone(),
            }),
            Err(reason) => {
                if policy == CandidatePolicy::RequireAll {
                    return Err(PlanError::CandidateRejected(reason));
                }
                selected.rejections.push(CandidateRejection {
                    endpoint_id: endpoint.id.clone(),
                    reason,
                });
            }
        }
    }
    if selected.candidates.is_empty() {
        return Err(PlanError::NoCandidate {
            rejections: selected.rejections,
        });
    }
    Ok(selected)
}
impl ExecutionPlan {
    pub fn for_request(
        topology: &CompiledTopology,
        request: &crate::adapter::Request,
    ) -> Result<Self, PlanError> {
        let public = topology
            .model(&request.model)
            .ok_or(PlanError::UnknownModel)?;
        super::admit(public, request).map_err(|_| PlanError::InvalidRequest)?;
        let mut plan = Self::derive(
            topology,
            &request.model,
            public.task,
            if request.delivery.streaming() {
                Delivery::Stream
            } else {
                Delivery::Json
            },
        )?;
        let selected = select_candidates(
            request,
            plan.policy.candidates,
            topology.route_endpoints(&public.route),
        )?;
        plan.candidates = selected.candidates;
        plan.rejections = selected.rejections;
        Ok(plan)
    }
    /// Derive the plan for one public model label. The label is the only model
    /// input a request supplies; targets, credentials and profiles stay fixed.
    pub fn derive(
        topology: &CompiledTopology,
        model: &str,
        task: TaskKind,
        delivery: Delivery,
    ) -> Result<Self, PlanError> {
        let model_contract = topology.model(model).ok_or(PlanError::UnknownModel)?;
        if model_contract.task != task {
            return Err(PlanError::TaskMismatch);
        }
        let route = topology
            .route(&model_contract.route)
            .ok_or(PlanError::UnknownModel)?;
        Ok(Self {
            model: model_contract.id.clone(),
            task,
            delivery,
            policy: route.policy.clone(),
            rejections: vec![],
            candidates: route
                .endpoints
                .iter()
                .enumerate()
                .map(|(ordinal, endpoint_id)| CandidatePlan {
                    ordinal,
                    endpoint_id: endpoint_id.clone(),
                })
                .collect(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::topology::catalog::default_topology;

    #[test]
    fn unknown_labels_never_plan() {
        let topology = default_topology().unwrap();
        assert_eq!(
            ExecutionPlan::derive(&topology, "gpt-42", TaskKind::Generation, Delivery::Json),
            Err(PlanError::UnknownModel)
        );
        // TaskMismatch becomes observable when a second task domain is admitted;
        // with a single TaskKind the compile-time checks already pin the binding.
    }
}
