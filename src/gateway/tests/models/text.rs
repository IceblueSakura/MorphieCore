//! Text-only discovery expectations, independent of the compiled summary implementation.
use super::*;
use crate::{gateway::ingress::discovery::TextModelView, protocol::ClientProtocol};
use bytes::Bytes;

#[tokio::test]
async fn text_directory_is_interface_scoped_and_leaves_standard_discovery_unchanged() {
    let gate = crate::gateway::tests::support::gateway(Limits::default());
    let (_, standard) = request(&gate, "GET", "/v1/models").await;
    let (status, list) = request(&gate, "GET", "/v1/morphiecore/models").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list["object"], "morphiecore.model_list");
    assert_eq!(list["scope"], "generation_text");
    assert_eq!(list["coverage"], "partial");
    assert_eq!(list["unlisted"], "not_assessed");
    assert_eq!(list["provider_effectiveness"], "not_asserted");
    assert_eq!(list["full_loop"], "not_asserted");
    assert_eq!(list["data"].as_array().unwrap().len(), 1);
    let descriptor = &list["data"][0];
    assert_eq!(descriptor["id"], "fixture-model");
    assert_eq!(descriptor["name"], "fixture-model");
    assert_eq!(descriptor["owned_by"], "Synthetic Developer");
    let interfaces = descriptor["interfaces"].as_array().unwrap();
    assert_eq!(interfaces.len(), 2);
    assert_eq!(interfaces[0]["protocol"], "chat_completions");
    assert_eq!(interfaces[1]["protocol"], "messages");
    for interface in interfaces {
        assert_eq!(interface["task"], "generation");
        assert_eq!(interface["delivery_modes"], json!(["json", "sse"]));
        let generation = &interface["generation"];
        assert_eq!(generation["scope"], "route_common");
        assert_eq!(generation["composition"], "not_asserted");
        assert_eq!(generation["input"]["text"], json!({"state":"faithful"}));
        assert_eq!(generation["output"], json!({"text":{"state":"faithful"}}));
        assert!(generation["input"]["image"].is_array());
        assert_eq!(
            generation["token_limits"],
            json!({
                "purpose":"planning","context_window_tokens":null,"max_input_tokens":null,
                "max_output_tokens":null,"output_token_accounting":"not_assessed"
            })
        );
        for feature in [
            "function_tools",
            "strict_tool_schema",
            "parallel_tool_calls",
            "max_output_tokens",
        ] {
            assert_eq!(
                generation["features"][feature],
                json!({"state":"faithful"}),
                "{interface}"
            );
        }
    }
    assert_eq!(
        interfaces[0]["generation"]["features"]["json_object_output"],
        json!({"state":"faithful"})
    );
    assert_eq!(
        interfaces[1]["generation"]["features"]["json_object_output"],
        json!({"state":"unavailable"})
    );
    assert_eq!(
        interfaces[0]["generation"]["reasoning"]["effort"]["reasoning"],
        json!({"state":"faithful","values":["minimal","low","medium","high","xhigh","max"]})
    );
    assert_eq!(
        interfaces[0]["generation"]["reasoning"]["effort"]["response"],
        json!({"state":"unavailable"})
    );
    let (status, detail) = request(&gate, "GET", "/v1/morphiecore/models/fixture-model").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(detail["object"], "morphiecore.model");
    assert_eq!(&detail["data"], descriptor);
    for private in [
        "private-fixture",
        "fixture-key",
        "fixture-chat",
        "fixture-agent",
        "synthetic-upstream-credential",
        "127.0.0.1",
        "version",
        "catalog_revision",
        "created",
        "has_more",
        "modalities",
    ] {
        assert!(!list.to_string().contains(private), "{private}");
    }
    for label in [
        "limited-model",
        "fixture-image",
        "unknown",
        "fixture%2Flimited-model",
    ] {
        let (status, error) =
            request(&gate, "GET", &format!("/v1/morphiecore/models/{label}")).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(error["error"]["code"], "model_not_found");
        assert!(!error.to_string().contains(label));
    }
    let (status, error) = request(&gate, "GET", "/v1/morphiecore/models/%FF").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(error["error"]["code"], "invalid_request");
    assert_eq!(request(&gate, "GET", "/v1/models").await.1, standard);
    for path in [
        "/v1/morphiecore/models",
        "/v1/morphiecore/models/fixture-model",
    ] {
        let head = gate
            .router()
            .oneshot(
                Request::builder()
                    .method("HEAD")
                    .uri(path)
                    .header("x-api-key", crate::gateway::tests::support::KEY)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(head.status(), StatusCode::OK);
        assert_eq!(head.headers()["cache-control"], "no-store");
        assert!(
            to_bytes(head.into_body(), 256 << 10)
                .await
                .unwrap()
                .is_empty()
        );
    }
}

fn interface(entry: BoundEntry) -> Value {
    let entries = BTreeMap::from([(
        (entry.client.protocol(), entry.public.id.as_str().to_owned()),
        Arc::new(entry),
    )]);
    let view = TextModelView::bind(
        &crate::gateway::tests::topology::topology(),
        &entries,
        256 << 10,
    )
    .unwrap();
    let list: Value = serde_json::from_slice(&view.list).unwrap();
    list["data"][0]["interfaces"][0].clone()
}
fn descriptor(entry: BoundEntry) -> Value {
    interface(entry)["generation"].clone()
}

#[test]
fn text_contract_intersects_source_efforts_and_comparable_limits_without_inventing_defaults() {
    use crate::protocol::reasoning::ReasoningHistory;
    use crate::semantic::task::generation::EffortLevel::{High, Low};
    let gate = crate::gateway::tests::support::gateway(Limits::default());
    let mut entry = gate.state.entries[&(ClientProtocol::Chat, "fixture-model".into())]
        .as_ref()
        .clone();
    let mut first = entry.candidates[0].as_ref().clone();
    let mut second = first.clone();
    first
        .endpoint
        .representation
        .openai_mut()
        .unwrap()
        .reasoning_history = Some(ReasoningHistory::Accepted);
    second
        .endpoint
        .representation
        .openai_mut()
        .unwrap()
        .reasoning_history = Some(ReasoningHistory::Accepted);
    let limits = first.endpoint.representation.model_limits_mut();
    limits.allowed_effort = Some(vec![Low, High].into());
    limits.context_window_tokens = Some(200_000);
    limits.maximum_input_tokens = Some(150_000);
    limits.maximum_output_tokens = Some(8192);
    let limits = second.endpoint.representation.model_limits_mut();
    limits.allowed_effort = Some(vec![High].into());
    limits.context_window_tokens = Some(100_000);
    limits.maximum_input_tokens = Some(90_000);
    limits.maximum_output_tokens = Some(4096);
    second.endpoint.execution.streaming = false;
    entry.candidates = vec![Arc::new(first), Arc::new(second)];
    assert_eq!(interface(entry.clone())["delivery_modes"], json!(["json"]));
    let value = descriptor(entry.clone());
    assert_eq!(value["reasoning"]["history"], "accepted");
    assert_eq!(value["token_limits"]["max_output_tokens"], 4096);
    assert_eq!(value["token_limits"]["context_window_tokens"], 100_000);
    assert_eq!(value["token_limits"]["max_input_tokens"], 90_000);
    assert_eq!(
        value["reasoning"]["effort"]["reasoning"],
        json!({"state":"faithful","values":["high"]})
    );
    let mut second = entry.candidates[1].as_ref().clone();
    second
        .endpoint
        .representation
        .openai_mut()
        .unwrap()
        .reasoning_history = Some(ReasoningHistory::RequiredWithTools);
    second
        .endpoint
        .representation
        .model_limits_mut()
        .allowed_effort = Some(vec![crate::semantic::task::generation::EffortLevel::Medium].into());
    second
        .endpoint
        .representation
        .model_limits_mut()
        .maximum_output_tokens = None;
    second
        .endpoint
        .representation
        .model_limits_mut()
        .context_window_tokens = None;
    entry.candidates[1] = Arc::new(second);
    let value = descriptor(entry.clone());
    assert_eq!(value["reasoning"]["history"], "non_uniform");
    let mut unassessed = entry.candidates[1].as_ref().clone();
    unassessed
        .endpoint
        .representation
        .openai_mut()
        .unwrap()
        .reasoning_history = None;
    entry.candidates.push(Arc::new(unassessed));
    assert_eq!(
        descriptor(entry.clone())["reasoning"]["history"],
        "not_assessed"
    );
    assert_eq!(value["token_limits"]["max_output_tokens"], Value::Null);
    assert_eq!(value["token_limits"]["context_window_tokens"], Value::Null);
    assert_eq!(value["token_limits"]["max_input_tokens"], 90_000);
    assert_eq!(
        value["reasoning"]["effort"]["reasoning"],
        json!({"state":"non_uniform"})
    );
    entry.candidates.truncate(1);
    let mut first = entry.candidates[0].as_ref().clone();
    first
        .endpoint
        .representation
        .model_limits_mut()
        .maximum_output_tokens = Some(u64::MAX);
    entry.candidates[0] = Arc::new(first);
    let mut mixed = entry.clone();
    let mut safe = mixed.candidates[0].as_ref().clone();
    safe.endpoint
        .representation
        .model_limits_mut()
        .maximum_output_tokens = Some(4096);
    mixed.candidates.push(Arc::new(safe));
    assert_eq!(
        descriptor(mixed)["token_limits"]["max_output_tokens"],
        Value::Null
    );
    assert_eq!(
        descriptor(entry)["token_limits"]["max_output_tokens"],
        Value::Null
    );
    let mut entry = gate.state.entries[&(ClientProtocol::Chat, "fixture-model".into())]
        .as_ref()
        .clone();
    let mut candidate = entry.candidates[0].as_ref().clone();
    let caps = candidate.endpoint.representation.openai_mut().unwrap();
    caps.model_limits.allowed_effort = Some(vec![High].into());
    caps.adaptation.effort_mapping = Some(&crate::protocol::reasoning::ReasoningEffortMapping {
        canonical_model: "fixture-canonical",
        rule: "example.effort.v1",
        scope: crate::semantic::task::generation::EffortScope::Reasoning,
        native_levels: &[High],
        encode: &[
            (crate::semantic::task::generation::EffortLevel::Medium, High),
            (High, High),
        ],
        decode: &[(High, High)],
    });
    entry.candidates = vec![Arc::new(candidate)];
    assert_eq!(
        descriptor(entry)["reasoning"]["effort"]["reasoning"],
        json!({
            "state":"approved_adaptation","values":["medium","high"],"rules":["example.effort.v1"]
        })
    );
}

#[test]
fn text_contract_uses_actual_dialect_checks_and_does_not_advertise_omitted_controls() {
    use crate::protocol::openai::{Dialect, GenerationApi, GenerationCodec};
    let gate = crate::gateway::tests::support::gateway(Limits::default());
    let mut entry = gate.state.entries[&(ClientProtocol::Chat, "fixture-model".into())]
        .as_ref()
        .clone();
    let mut candidate = entry.candidates[0].as_ref().clone();
    let caps = candidate.endpoint.representation.openai_mut().unwrap();
    caps.adaptation = GenerationCodec::new(GenerationApi::Chat, Dialect::Bailian, None).adaptation;
    candidate.endpoint.upstream_model = "qwen3.8-flash".into();
    entry.candidates = vec![Arc::new(candidate)];
    let value = descriptor(entry.clone());
    assert_eq!(
        value["features"]["strict_tool_schema"],
        json!({"state":"unavailable"})
    );
    assert_eq!(
        value["features"]["function_tools"],
        json!({"state":"faithful"})
    );
    let mut candidate = entry.candidates[0].as_ref().clone();
    candidate
        .endpoint
        .representation
        .openai_mut()
        .unwrap()
        .adaptation = GenerationCodec::new(GenerationApi::Chat, Dialect::DeepSeek, None).adaptation;
    entry.candidates = vec![Arc::new(candidate)];
    let value = descriptor(entry.clone());
    assert_eq!(
        value["features"]["parallel_tool_calls"],
        json!({"state":"faithful"})
    );
    // A plain request blocked by the client-reasoning boundary is not evidence
    // that unrelated function declarations are unsupported.
    let mut candidate = entry.candidates[0].as_ref().clone();
    candidate
        .endpoint
        .representation
        .openai_mut()
        .unwrap()
        .chat_thinking_output = true;
    entry.candidates = vec![Arc::new(candidate)];
    assert_eq!(
        descriptor(entry)["features"]["function_tools"],
        json!({"state":"not_assessed"})
    );
}

#[test]
fn text_reasoning_uses_source_scopes_and_never_advertises_noop_activation_or_default_caps() {
    let gate = crate::gateway::tests::support::gateway(Limits::default());
    let mut entry = gate.state.entries[&(ClientProtocol::Messages, "fixture-model".into())]
        .as_ref()
        .clone();
    let mut candidate = entry.candidates[0].as_ref().clone();
    candidate.endpoint = crate::topology::catalog::GOOGLE_BINDINGS[0].endpoint();
    entry.candidates = vec![Arc::new(candidate.clone())];
    let value = descriptor(entry.clone());
    assert_eq!(value["token_limits"]["max_output_tokens"], 65536);
    assert_eq!(value["token_limits"]["max_input_tokens"], 1_048_576);
    assert_eq!(value["token_limits"]["context_window_tokens"], Value::Null);
    assert_eq!(
        value["reasoning"]["effort"]["response"],
        json!({
            "state":"approved_adaptation","values":["low","medium","high"],
            "rules":["google.gemini-3.8-flash.response-effort-to-thinking.v1"]
        })
    );
    assert_eq!(
        value["features"]["json_schema_output"],
        json!({"state":"unavailable"})
    );
    assert!(
        !value["reasoning"]["activation"]["values"]
            .as_array()
            .is_some_and(|values| values.contains(&json!("adaptive")))
    );
    candidate.endpoint = crate::topology::catalog::MESSAGES_BINDINGS[0].endpoint();
    entry.candidates = vec![Arc::new(candidate)];
    let value = descriptor(entry);
    assert_eq!(value["token_limits"]["max_output_tokens"], Value::Null);
    assert_eq!(
        value["reasoning"]["activation"],
        json!({
            "state":"faithful","values":["disabled","adaptive"]
        })
    );
}

#[test]
fn text_directory_bounds_cover_both_envelopes_and_empty_generation() {
    // Image domains remain inside the same complete, bounded Generation directory.
    let gate = crate::gateway::tests::support::gateway(Limits::default());
    let topology = crate::gateway::tests::topology::topology();
    let entries = BTreeMap::new();
    let empty = TextModelView::bind(&topology, &entries, 256 << 10).unwrap();
    assert_eq!(
        serde_json::from_slice::<Value>(&empty.list).unwrap()["data"],
        json!([])
    );
    assert!(empty.models.is_empty());
    assert!(TextModelView::bind(&topology, &entries, empty.list.len()).is_ok());
    assert!(matches!(
        TextModelView::bind(&topology, &entries, empty.list.len() - 1),
        Err(StartupError::Limits)
    ));
    let view = TextModelView::bind(&topology, &gate.state.entries, 256 << 10).unwrap();
    let limit = view
        .list
        .len()
        .max(view.models.values().map(Bytes::len).max().unwrap());
    assert!(TextModelView::bind(&topology, &gate.state.entries, limit).is_ok());
    assert!(matches!(
        TextModelView::bind(&topology, &gate.state.entries, limit - 1),
        Err(StartupError::Limits)
    ));
    let entry = gate.state.entries.values().next().unwrap().clone();
    let many = (0..193)
        .map(|i| ((ClientProtocol::Chat, format!("label-{i}")), entry.clone()))
        .collect();
    assert!(matches!(
        TextModelView::bind(&topology, &many, 256 << 10),
        Err(StartupError::Binding)
    ));
}

#[test]
fn image_domains_preserve_placement_detail_and_candidate_intersection() {
    use crate::protocol::openai::{Dialect, GenerationApi, GenerationCodec, TargetContract};
    let gate = crate::gateway::tests::support::gateway(Limits::default());
    let mut entry = gate.state.entries[&(ClientProtocol::Chat, "fixture-model".into())]
        .as_ref()
        .clone();
    entry.client = crate::adapter::ClientAdapter::openai(
        GenerationCodec::new(GenerationApi::Responses, Dialect::MorphieCore, None),
        TargetContract::full(),
    );
    // A native Responses candidate carries both user and function-result images.
    let mut responses = entry.candidates[0].as_ref().clone();
    responses.endpoint = crate::gateway::tests::topology::topology()
        .endpoint(&crate::topology::EndpointId::new("fixture-responses").unwrap())
        .unwrap()
        .clone();
    let responses = Arc::new(responses);
    entry.candidates = vec![responses.clone()];
    let value = descriptor(entry.clone());
    let images = value["input"]["image"].as_array().unwrap();
    for placement in ["user", "tool_result"] {
        let domain = images
            .iter()
            .find(|d| {
                d["placement"] == placement && d["source"] == "inline" && d["detail"] == "auto"
            })
            .unwrap_or_else(|| panic!("missing {placement} inline/auto domain: {images:?}"));
        assert_eq!(domain["state"], "faithful");
        assert_eq!(
            domain["mime_types"],
            json!(["image/png", "image/jpeg", "image/webp", "image/gif"])
        );
    }
    let mut restricted = entry.candidates[0].as_ref().clone();
    restricted
        .endpoint
        .representation
        .openai_mut()
        .unwrap()
        .images
        .details
        .clear();
    entry.candidates.push(Arc::new(restricted));
    let value = descriptor(entry.clone());
    let images = value["input"]["image"].as_array().unwrap();
    assert!(!images.is_empty());
    assert!(images.iter().all(|d| d["detail"] == "omitted"));

    // A Chat alternative removes tool-result image support, not ordinary user images.
    entry.candidates = vec![
        responses,
        gate.state.entries[&(ClientProtocol::Chat, "fixture-model".into())].candidates[0].clone(),
    ];
    let value = descriptor(entry.clone());
    let images = value["input"]["image"].as_array().unwrap();
    assert!(!images.is_empty());
    assert!(images.iter().all(|d| d["placement"] == "user"));
    entry.public.contract.image_input = false;
    let value = descriptor(entry);
    assert_eq!(value["input"]["image"], json!([]));
    assert_eq!(value["input"]["text"], json!({"state":"faithful"}));
    assert_eq!(value["output"], json!({"text":{"state":"faithful"}}));
}

#[tokio::test]
async fn an_image_only_gateway_keeps_standard_discovery_and_has_no_text_models() {
    let gate = Gateway::new(
        crate::gateway::tests::topology::topology(),
        TaskEntries {
            images: vec![ImageEntry {
                model: "fixture-image".into(),
            }],
            ..Default::default()
        },
        BTreeMap::from([(
            crate::provider::CredentialBindingId::new("fixture-key").unwrap(),
            Arc::new(SecretMaterial::new("synthetic-upstream-credential").unwrap()),
        )]),
        BTreeMap::from([(
            "fixture-agent".into(),
            SecretMaterial::new(crate::gateway::tests::support::KEY).unwrap(),
        )]),
        Limits::default(),
        None,
    )
    .unwrap();
    let (_, list) = request(&gate, "GET", "/v1/models").await;
    assert_eq!(
        list["data"],
        json!([{
            "id":"fixture-image","object":"model","created":8,"owned_by":"Synthetic Image Developer"
        }])
    );
    let (status, list) = request(&gate, "GET", "/v1/morphiecore/models").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list["data"], json!([]));
    assert_eq!(
        request(&gate, "GET", "/v1/morphiecore/models/fixture-image")
            .await
            .0,
        StatusCode::NOT_FOUND
    );
}
