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
    representable_for_client(endpoint, request, None)
}
fn representable_for_client(
    endpoint: &Endpoint,
    request: &crate::adapter::Request,
    client: Option<&crate::adapter::ClientAdapter>,
) -> Result<(), RejectionReason> {
    if (request.delivery.streaming() || endpoint.representation.forced_stream())
        && !endpoint.execution.streaming
    {
        return Err(RejectionReason::Streaming);
    }
    let value = endpoint.encode_request(request).map_err(|error| {
        use crate::{adapter::ConversionFailure, lowering::generation::RepresentationError};
        match error {
            ConversionFailure { cause: crate::adapter::ConversionCause::Representation(RepresentationError::Admission(feature)), .. } => {
                RejectionReason::Semantic(feature)
            }
            ConversionFailure { cause: crate::adapter::ConversionCause::Representation(RepresentationError::ImageInput), .. } => RejectionReason::Image,
            ConversionFailure { cause: crate::adapter::ConversionCause::Representation(RepresentationError::ReplayOrigin), .. } => {
                RejectionReason::ReplayScope
            }
            _ => RejectionReason::Representation,
        }
    })?;
    if let Some(crate::adapter::ClientAdapter::OpenAi { adapter, .. }) = client
        && adapter.protocol == crate::protocol::openai::Profile::Chat
        && !adapter.adaptation.rules.readable_reasoning
        && endpoint.protocol == crate::topology::ProtocolProfile::OpenAiChat
        && let crate::lowering::target::TargetCaps::OpenAi(caps) = &endpoint.representation
        && caps.chat_thinking_output
        && value.get("enable_thinking") != Some(&serde_json::Value::Bool(false))
    {
        return Err(RejectionReason::Representation);
    }
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
    select_candidates_for_client(request, policy, endpoints, None)
}
pub(crate) fn select_candidates_for_client<'a>(
    request: &crate::adapter::Request,
    policy: CandidatePolicy,
    endpoints: impl IntoIterator<Item = &'a Endpoint>,
    client: Option<&crate::adapter::ClientAdapter>,
) -> Result<CandidateSelection, PlanError> {
    let mut selected = CandidateSelection {
        candidates: vec![],
        rejections: vec![],
    };
    for (index, endpoint) in endpoints.into_iter().enumerate() {
        if index >= MAX_CANDIDATES {
            return Err(PlanError::CandidateLimit);
        }
        match representable_for_client(endpoint, request, client) {
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
        let route = topology
            .route(&public.route)
            .ok_or(PlanError::UnknownModel)?;
        let selected = select_candidates(
            request,
            route.policy.candidates,
            topology.route_endpoints(&public.route),
        )?;
        Ok(Self {
            model: public.id.clone(),
            task: public.task,
            delivery: if request.delivery.streaming() {
                Delivery::Stream
            } else {
                Delivery::Json
            },
            policy: route.policy.clone(),
            candidates: selected.candidates,
            rejections: selected.rejections,
        })
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
    fn downstream_reasoning_requirements_filter_without_disabling_or_reordering() {
        use crate::adapter::ClientAdapter;
        use crate::protocol::openai::{Adapter, Dialect, Profile, TargetCaps};
        let topology = default_topology().unwrap();
        let endpoint = topology
            .endpoint(&EndpointId::new("dashscope-flash-chat").unwrap())
            .unwrap();
        let standard = Adapter::new(Profile::Chat, Dialect::Standard, None);
        let plain = ClientAdapter::openai(standard.clone(), TargetCaps::full());
        let readable = ClientAdapter::openai(
            Adapter::new(Profile::Chat, Dialect::ChatReasoning, None),
            TargetCaps::full(),
        );
        let mut unknown = endpoint.clone();
        unknown.id = EndpointId::new("unspecified-output").unwrap();
        unknown
            .representation
            .openai_mut()
            .unwrap()
            .chat_thinking_output = false;
        let request = standard
            .decode_client_request(
                br#"{"model":"qwen3.8-flash",
            "messages":[{"role":"user","content":"go"}],"reasoning_effort":"low"}"#,
            )
            .unwrap();
        let original = request.clone();
        let selected = select_candidates_for_client(
            &request,
            CandidatePolicy::SkipUnrepresentable,
            [endpoint, &unknown],
            Some(&plain),
        )
        .unwrap();
        assert_eq!(selected.candidates[0].ordinal, 1);
        assert_eq!(selected.rejections[0].endpoint_id, endpoint.id);
        assert!(
            select_candidates_for_client(
                &request,
                CandidatePolicy::RequireAll,
                [endpoint, &unknown],
                Some(&plain)
            )
            .is_err()
        );
        assert_eq!(
            select_candidates_for_client(
                &request,
                CandidatePolicy::RequireAll,
                [endpoint, &unknown],
                Some(&readable)
            )
            .unwrap()
            .candidates
            .len(),
            2
        );
        assert_eq!(request, original);
        let off = standard
            .decode_client_request(
                br#"{"model":"qwen3.8-flash",
            "messages":[{"role":"user","content":"go"}],"reasoning_effort":"none"}"#,
            )
            .unwrap();
        let wire = endpoint.encode_request(&off).unwrap();
        assert_eq!(wire["enable_thinking"], false);
        select_candidates_for_client(&off, CandidatePolicy::RequireAll, [endpoint], Some(&plain))
            .unwrap();
    }

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
