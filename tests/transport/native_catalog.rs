//! Native registration, OpenAI activation and Provider acceptance are distinct.
use morphiecore::{
    adapter::Request,
    execution::{admit, plan::representable, prepare},
    protocol::{
        anthropic::{Profile, ReplayTarget},
        openai::Profile as ClientProfile,
    },
    provider::SecretMaterial,
    semantic::{task::generation::LocalScope, value::Presence},
    topology::catalog,
};
#[test]
fn explicit_native_library_binding_is_not_openai_activation_or_a_default_model() {
    let binding = &catalog::MESSAGES_BINDINGS[0];
    let topology = binding.topology().unwrap();
    assert!(
        catalog::default_topology()
            .unwrap()
            .model(binding.model)
            .is_none()
    );
    let endpoint = binding.endpoint();
    let provider = binding.provider();
    let target = ReplayTarget::new(provider.id.as_str(), binding.upstream).unwrap();
    let source=Profile::AdaptiveTextTools.decode_generation_request(
        br#"{"model":"claude-haiku-5-5","max_tokens":32,"messages":[{"role":"user","content":"synthetic"}]}"#,
        LocalScope::new(1),&target).unwrap();
    let mut request = Request::from_generation(source, binding.model).unwrap();
    request.conversation = Some(
        morphiecore::semantic::context::ConversationContext::conversation("synthetic").unwrap(),
    );
    for stream in [false, true] {
        request.delivery.stream = Presence::Value(stream);
        admit(topology.model(binding.model).unwrap(), &request).unwrap();
        representable(&endpoint, &request).unwrap();
        let original = request.clone();
        let prepared = prepare(
            &endpoint,
            &provider,
            &SecretMaterial::new("synthetic-key").unwrap(),
            &request,
        )
        .unwrap();
        assert_eq!(prepared.path, "/zen/go/v1/messages");
        assert_eq!(prepared.auth_header.0, "x-api-key");
        assert!(
            prepared
                .safe_headers
                .contains(&("anthropic-version".into(), "2023-06-01".into()))
        );
        assert!(
            prepared
                .safe_headers
                .iter()
                .any(|(n, v)| n == "user-agent" && v.starts_with("MorphieCore/"))
        );
        assert!(
            prepared
                .safe_headers
                .iter()
                .any(|(n, v)| n == "x-opencode-session" && v.len() == 64)
        );
        let wire: serde_json::Value = serde_json::from_slice(&prepared.body).unwrap();
        assert_eq!(wire["model"], binding.upstream);
        assert_eq!(wire["stream"], stream);
        assert!(wire.get("thinking").is_none());
        assert!(wire.get("session_id").is_none());
        assert_eq!(original, request);
    }
    let mut controlled = request.clone();
    let mut settings = controlled.task.semantic.settings().clone();
    settings.reasoning =
        morphiecore::semantic::task::generation::ReasoningRequest::present(None, None);
    settings.reasoning.effort =
        Presence::Value(morphiecore::semantic::task::generation::ReasoningEffort::Low);
    controlled.task.semantic = controlled.task.semantic.with_settings(settings).unwrap();
    assert!(representable(&endpoint, &controlled).is_err());
    assert!(
        prepare(
            &endpoint,
            &provider,
            &SecretMaterial::new("synthetic-key").unwrap(),
            &controlled
        )
        .is_err()
    );
    let error = morphiecore::gateway::Gateway::new(
        topology,
        vec![morphiecore::gateway::Entry {
            model: binding.model.into(),
            protocol: ClientProfile::Responses,
            endpoint: endpoint.id,
        }],
        std::collections::BTreeMap::new(),
        SecretMaterial::new("synthetic-ingress-token-000001").unwrap(),
        morphiecore::gateway::Limits::default(),
        None,
    );
    assert!(error.is_err());
}
#[test]
fn native_unknown_field_diagnostics_are_closed_without_accepting_null_or_leaking_keys() {
    let p = Profile::AdaptiveTextTools;
    let base = serde_json::json!({"id":"msg","type":"message","role":"assistant","model":"synthetic-model",
        "content":[{"type":"text","text":"ok"}],"stop_reason":"end_turn",
        "usage":{"input_tokens":1,"output_tokens":1}});
    for (key, category) in [
        ("stop_details", "Anthropic stop_details"),
        ("private-business-key", "Anthropic field"),
    ] {
        let mut wire = base.clone();
        wire[key] = serde_json::Value::Null;
        let error = p.decode_response(wire.to_string().as_bytes()).unwrap_err();
        assert_eq!(
            error,
            morphiecore::protocol::CodecError::Unsupported(category.into())
        );
        assert!(!error.to_string().contains("private-business-key"));
    }
}
