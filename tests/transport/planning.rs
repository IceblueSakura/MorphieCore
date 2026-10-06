//! Candidate filtering must be value-sensitive and preserve trusted ordering.
use morphiecore::{
    adapter::{Adapter, Dialect},
    execution::ExecutionPlan,
    lowering::generation::GenerationRepresentationContract as Representation,
    protocol::openai::Profile,
    provider::{
        AuthScheme, CredentialBindingId, EndpointPath, ProviderDefinition, ProviderId,
        TrustedOrigin,
    },
    semantic::task::generation::ImageFormat,
    topology::*,
};
use serde_json::json;
fn provider(id: &str) -> ProviderDefinition {
    ProviderDefinition {
        id: ProviderId::new(id).unwrap(),
        origin: TrustedOrigin::parse("https://synthetic.invalid").unwrap(),
        chat_completions: None,
        responses: Some(EndpointPath::new("/responses").unwrap()),
        auth: AuthScheme::Bearer,
    }
}
fn topology(policy: CandidatePolicy) -> CompiledTopology {
    let model_id = ModelId::new("synthetic-canonical").unwrap();
    let mut endpoints = vec![];
    for (id, provider, formats) in [
        ("a", provider("first"), vec![ImageFormat::Png]),
        ("b", provider("second"), vec![ImageFormat::Bmp]),
    ] {
        let mut representation = Adapter::new(Profile::Responses, Dialect::MorphieCore, None)
            .contract(&Representation::full());
        representation.images.inline_formats = formats;
        endpoints.push(Endpoint {
            id: EndpointId::new(id).unwrap(),
            provider: provider.id.clone(),
            target: EndpointTarget {
                origin: provider.origin.clone(),
                path: provider.responses.clone().unwrap(),
            },
            canonical_model: model_id.clone(),
            task: TaskKind::Generation,
            protocol: ProtocolProfile::OpenAiResponses,
            upstream_model: format!("{id}-alias"),
            representation,
            execution: ExecutionContract {
                streaming: true,
                retry_before_commit: false,
                request_body_limit: 4096,
                response_body_limit: 4096,
                timeout_ms: 1000,
                credential_kind: morphiecore::provider::CredentialKind::ApiKey,
            },
            credential: CredentialBindingId::new(&format!("{id}-key")).unwrap(),
        });
    }
    let route = Route {
        id: RouteId::new("route").unwrap(),
        task: TaskKind::Generation,
        endpoints: vec![EndpointId::new("a").unwrap(), EndpointId::new("b").unwrap()],
        policy: RoutePolicy {
            candidates: policy,
            ..RoutePolicy::default()
        },
    };
    let public = PublicModel::new(
        ModelId::new("public").unwrap(),
        model_id.clone(),
        TaskKind::Generation,
        route.id.clone(),
        GenerationSemanticContract::text_images(),
    );
    compile(
        vec![provider("first"), provider("second")],
        endpoints,
        vec![route],
        vec![public],
        vec![CanonicalModel {
            id: model_id,
            task: TaskKind::Generation,
            contract: GenerationSemanticContract::full(),
        }],
    )
    .unwrap()
}
#[test]
fn forced_upstream_stream_is_rejected_before_attempt_preparation() {
    use morphiecore::execution::plan::{RejectionReason, representable};
    let compiled = topology(CandidatePolicy::SkipUnrepresentable);
    let mut endpoint = compiled
        .endpoint(&EndpointId::new("a").unwrap())
        .unwrap()
        .clone();
    endpoint.execution.streaming = false;
    let request = Adapter::new(Profile::Responses, Dialect::MorphieCore, None)
        .decode_request(br#"{"model":"public","input":"hello","stream":false}"#)
        .unwrap();
    assert!(representable(&endpoint, &request).is_ok());
    endpoint
        .representation
        .adaptation
        .rules
        .responses_forced_stream = true;
    assert_eq!(
        representable(&endpoint, &request),
        Err(RejectionReason::Streaming)
    );
    assert!(!request.delivery.streaming());
}

#[test]
fn full_requests_are_filtered_without_mutation_or_reordering() {
    let client = Adapter::new(Profile::Responses, Dialect::MorphieCore, None);
    let request=client.decode_request(&serde_json::to_vec(&json!({"model":"public","input":[{"role":"user","content":[{"type":"input_image","image_url":"data:image/bmp;base64,AQ=="}]}]})).unwrap()).unwrap();
    let before = request.clone();
    let plan =
        ExecutionPlan::for_request(&topology(CandidatePolicy::SkipUnrepresentable), &request)
            .unwrap();
    assert_eq!(
        plan.candidates
            .iter()
            .map(|c| c.endpoint_id.as_str())
            .collect::<Vec<_>>(),
        vec!["b"]
    );
    assert_eq!(request, before);
    assert_eq!(plan.rejections.len(), 1);
    assert_eq!(plan.rejections[0].endpoint_id.as_str(), "a");
    assert_eq!(
        plan.rejections[0].reason,
        morphiecore::execution::plan::RejectionReason::Image
    );
    let compiled = topology(CandidatePolicy::SkipUnrepresentable);
    let active = morphiecore::execution::plan::select_candidates(
        &request,
        CandidatePolicy::SkipUnrepresentable,
        compiled.route_endpoints(&RouteId::new("route").unwrap()),
    )
    .unwrap();
    assert_eq!(active.candidates, plan.candidates);
    assert_eq!(active.rejections, plan.rejections);
    assert!(ExecutionPlan::for_request(&topology(CandidatePolicy::RequireAll), &request).is_err());
    let both=client.decode_request(&serde_json::to_vec(&json!({"model":"public","input":[{"role":"user","content":[{"type":"input_image","image_url":"data:image/bmp;base64,AQ=="},{"type":"input_image","image_url":"data:image/png;base64,AQ=="}]}]})).unwrap()).unwrap();
    let error = ExecutionPlan::for_request(&topology(CandidatePolicy::SkipUnrepresentable), &both)
        .unwrap_err();
    let morphiecore::execution::PlanError::NoCandidate { rejections } = error else {
        panic!("expected all candidate rejections")
    };
    assert_eq!(rejections.len(), 2);
    assert!(
        rejections
            .iter()
            .all(|r| r.reason == morphiecore::execution::plan::RejectionReason::Image)
    );
    let text = client
        .decode_request(br#"{"model":"public","input":"x"}"#)
        .unwrap();
    let plan =
        ExecutionPlan::for_request(&topology(CandidatePolicy::SkipUnrepresentable), &text).unwrap();
    assert_eq!(
        plan.candidates
            .iter()
            .map(|c| c.endpoint_id.as_str())
            .collect::<Vec<_>>(),
        vec!["a", "b"]
    );
    let endpoint = compiled.endpoint(&EndpointId::new("a").unwrap()).unwrap();
    assert_eq!(
        morphiecore::execution::plan::select_candidates(
            &text,
            CandidatePolicy::SkipUnrepresentable,
            std::iter::repeat_n(endpoint, 65)
        ),
        Err(morphiecore::execution::PlanError::CandidateLimit)
    );
}
