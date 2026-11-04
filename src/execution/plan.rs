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
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RejectionReason {
    Streaming,
    BodyLimit,
    Semantic(crate::semantic::task::generation::Feature),
    Image,
    ReplayScope,
    Representation,
    Conversion(Box<crate::protocol::ConversionFailure>),
}
impl RejectionReason {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Streaming => "admission.streaming",
            Self::BodyLimit => "admission.request_body_limit",
            Self::Semantic(feature) => super::failure::feature_code(feature),
            Self::Image => "admission.image",
            Self::ReplayScope => "admission.replay_scope",
            Self::Representation => "admission.client_representation",
            Self::Conversion(cause) => super::failure::conversion_code(cause),
        }
    }
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
    request: &crate::protocol::GenerationRequestEnvelope,
) -> Result<(), RejectionReason> {
    project_candidate(endpoint, request, None).map(drop)
}

/// One preflight conversion, including its body-free adaptation evidence.
pub(crate) fn project_candidate(
    endpoint: &Endpoint,
    request: &crate::protocol::GenerationRequestEnvelope,
    client: Option<&crate::adapter::ClientAdapter>,
) -> Result<crate::execution::attempt::RequestProjection, RejectionReason> {
    if (request.delivery.streaming() || endpoint.protocol.requires_streaming_intake())
        && !endpoint.execution.streaming
    {
        return Err(RejectionReason::Streaming);
    }
    let fallback = if let Some(crate::adapter::ClientAdapter::OpenAi { adapter, .. }) = client
        && adapter.protocol == crate::protocol::openai::GenerationApi::Chat
        && !adapter.adaptation.rules.readable_reasoning
        && endpoint.protocol == crate::topology::GenerationProtocol::OpenAiChat
    {
        endpoint
            .representation
            .openai()
            .map(|contract| crate::lowering::request::chat_history_fallback(request, contract))
            .transpose()
            .map_err(|_| RejectionReason::Representation)?
    } else {
        None
    };
    let input = fallback
        .as_ref()
        .map_or(request, |projection| projection.value.as_ref());
    let mut projection = endpoint
        .encode_request_with_evidence(input)
        .map_err(|error| {
            use crate::protocol::{ConversionFailure, conversion::generation::RepresentationError};
            match error {
                ConversionFailure {
                    cause:
                        crate::protocol::ConversionCause::Representation(
                            RepresentationError::Admission(feature),
                        ),
                    ..
                } => RejectionReason::Semantic(feature),
                ConversionFailure {
                    cause:
                        crate::protocol::ConversionCause::Representation(
                            RepresentationError::ImageInput,
                        ),
                    ..
                } => RejectionReason::Image,
                ConversionFailure {
                    cause:
                        crate::protocol::ConversionCause::Representation(
                            RepresentationError::ReplayOrigin,
                        ),
                    ..
                } => RejectionReason::ReplayScope,
                error => RejectionReason::Conversion(Box::new(error)),
            }
        })?;
    if let Some(fallback) = fallback {
        projection.evidence.extend(fallback.evidence);
    }
    crate::execution::attempt::RequestProjection::new(
        projection,
        endpoint.execution.request_body_limit,
    )
    .map_err(|_| RejectionReason::BodyLimit)
}
/// Sole ordered selection mechanism for library plans and activated Gateway members.
pub fn select_candidates<'a>(
    request: &crate::protocol::GenerationRequestEnvelope,
    policy: CandidatePolicy,
    endpoints: impl IntoIterator<Item = &'a Endpoint>,
) -> Result<CandidateSelection, PlanError> {
    let (candidates, rejections) =
        select_candidates_with(request, policy, endpoints, None, |ordinal, endpoint, _| {
            CandidatePlan {
                ordinal,
                endpoint_id: endpoint.id.clone(),
            }
        })?;
    Ok(CandidateSelection {
        candidates,
        rejections,
    })
}

/// Retention is a caller-local optimization; selection and rejection stay shared.
pub(crate) fn select_candidates_with<'a, T>(
    request: &crate::protocol::GenerationRequestEnvelope,
    policy: CandidatePolicy,
    endpoints: impl IntoIterator<Item = &'a Endpoint>,
    client: Option<&crate::adapter::ClientAdapter>,
    mut retain: impl FnMut(usize, &Endpoint, crate::execution::attempt::RequestProjection) -> T,
) -> Result<(Vec<T>, Vec<CandidateRejection>), PlanError> {
    let mut candidates = vec![];
    let mut rejections = vec![];
    for (index, endpoint) in endpoints.into_iter().enumerate() {
        if index >= MAX_CANDIDATES {
            return Err(PlanError::CandidateLimit);
        }
        match project_candidate(endpoint, request, client) {
            Ok(projection) => candidates.push(retain(index, endpoint, projection)),
            Err(reason) => {
                if policy == CandidatePolicy::RequireAll {
                    return Err(PlanError::CandidateRejected(reason));
                }
                rejections.push(CandidateRejection {
                    endpoint_id: endpoint.id.clone(),
                    reason,
                });
            }
        }
    }
    if candidates.is_empty() {
        return Err(PlanError::NoCandidate { rejections });
    }
    Ok((candidates, rejections))
}
impl ExecutionPlan {
    pub fn for_request(
        topology: &CompiledTopology,
        request: &crate::protocol::GenerationRequestEnvelope,
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::topology::catalog::default_topology;

    #[test]
    fn downstream_reasoning_requirements_fallback_or_delegate_without_reordering() {
        use crate::{
            adapter::ClientAdapter,
            protocol::openai::{Dialect, GenerationApi, GenerationCodec, TargetContract},
            semantic::task::generation::{ReasoningContext, ReplayMismatch},
        };
        let topology = default_topology().unwrap();
        let endpoint = topology
            .endpoint(&EndpointId::new("deepseek-chat").unwrap())
            .unwrap();
        let standard = GenerationCodec::new(GenerationApi::Chat, Dialect::Standard, None);
        let plain = ClientAdapter::openai(standard.clone(), TargetContract::full());
        let readable = ClientAdapter::openai(
            GenerationCodec::new(GenerationApi::Chat, Dialect::ChatReasoning, None),
            TargetContract::full(),
        );
        let mut unknown = endpoint.clone();
        unknown.id = EndpointId::new("unassessed-history").unwrap();
        unknown
            .representation
            .openai_mut()
            .unwrap()
            .reasoning_history = None;
        let tool_request = |effort| {
            standard
                .decode_client_request(
                    serde_json::json!({"model":"deepseek-flash","reasoning_effort":effort,
                    "messages":[{"role":"user","content":"go"}],
                    "tools":[{"type":"function","function":{"name":"read",
                        "parameters":{"type":"object","properties":{}}}}]})
                    .to_string()
                    .as_bytes(),
                )
                .unwrap()
        };
        let request = tool_request("low");
        let original = request.clone();
        assert!(
            !endpoint.encode_request(&request).unwrap()["tools"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        let projected = project_candidate(endpoint, &request, Some(&plain)).unwrap();
        assert_eq!(projected.value()["reasoning_effort"], "none");
        assert!(
            projected
                .evidence()
                .contains_rule("chat.fallback-thinking-off.v1")
        );
        for (context, mismatch) in [
            (
                crate::semantic::value::Presence::Value(ReasoningContext::CurrentTurn),
                None,
            ),
            (
                crate::semantic::value::Presence::Absent,
                Some(ReplayMismatch::Reject),
            ),
        ] {
            let mut constrained = request.clone();
            let mut settings = constrained.task.semantic.settings().clone();
            settings.reasoning.context = context;
            settings.reasoning.replay_mismatch = mismatch;
            constrained.task.semantic = constrained.task.semantic.with_settings(settings).unwrap();
            let before = constrained.clone();
            assert!(endpoint.encode_request(&constrained).is_err());
            assert!(project_candidate(endpoint, &constrained, Some(&plain)).is_err());
            assert_eq!(constrained, before);
        }
        let projected = project_candidate(endpoint, &request, Some(&readable)).unwrap();
        assert_eq!(projected.value()["reasoning_effort"], "low");
        assert!(
            !projected
                .evidence()
                .contains_rule("chat.fallback-thinking-off.v1")
        );
        let mut always_thinking = endpoint.clone();
        always_thinking
            .representation
            .openai_mut()
            .unwrap()
            .adaptation
            .rules
            .reject_disabled_reasoning = true;
        assert_eq!(
            project_candidate(&always_thinking, &request, Some(&plain)).map(drop),
            Err(RejectionReason::Representation)
        );
        let select = |policy, client: &ClientAdapter| {
            select_candidates_with(
                &request,
                policy,
                [endpoint, &unknown],
                Some(client),
                |ordinal, _, _| ordinal,
            )
        };
        let (selected, rejected) = select(CandidatePolicy::SkipUnrepresentable, &plain).unwrap();
        assert_eq!(selected, [0, 1]);
        assert!(rejected.is_empty());
        assert_eq!(
            select(CandidatePolicy::RequireAll, &plain).unwrap().0.len(),
            2
        );
        assert_eq!(
            select(CandidatePolicy::RequireAll, &readable)
                .unwrap()
                .0
                .len(),
            2
        );
        assert_eq!(request, original);
        let off = tool_request("none");
        assert_eq!(
            endpoint.encode_request(&off).unwrap()["reasoning_effort"],
            "none"
        );
        assert!(
            !project_candidate(endpoint, &off, Some(&plain))
                .unwrap()
                .evidence()
                .contains_rule("chat.fallback-thinking-off.v1")
        );
        let mut settings = request.task.semantic.settings().clone();
        settings.tools = None;
        let mut text = request.clone();
        text.task.semantic = text.task.semantic.with_settings(settings).unwrap();
        project_candidate(endpoint, &text, Some(&plain)).unwrap();
        let assessed = topology
            .endpoint(&EndpointId::new("dashscope-flash-chat").unwrap())
            .unwrap();
        project_candidate(assessed, &text, Some(&plain)).unwrap();
        let unassessed = topology
            .endpoint(&EndpointId::new("aliyun-tokenplan-cn-chat").unwrap())
            .unwrap();
        assert!(unassessed.encode_request(&text).is_ok());
        project_candidate(unassessed, &text, Some(&plain)).unwrap();
    }
}
