//! Independent Models wire expectations over synthetic credentials, with no upstream I/O.
use crate::gateway::*;
use crate::protocol::ClientProtocol;
use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use serde_json::{Value, json};
use tower::ServiceExt;

mod text;

#[test]
fn admitted_omitted_control_is_not_published_as_rejected_input() {
    let gate = crate::gateway::tests::support::gateway(Limits::default());
    let topology = crate::gateway::tests::topology::topology();
    let mut entry = gate.state.entries[&(ClientProtocol::Chat, "fixture-model".into())]
        .as_ref()
        .clone();
    let mut candidate = entry.candidates[0].as_ref().clone();
    candidate.endpoint = topology
        .endpoint(&crate::topology::EndpointId::new("fixture-responses").unwrap())
        .unwrap()
        .clone();
    candidate
        .endpoint
        .representation
        .openai_mut()
        .unwrap()
        .adaptation
        .rules
        .responses_siwc = true;
    entry.candidates = vec![Arc::new(candidate)];
    let entries = [(
        (ClientProtocol::Chat, "fixture-model".into()),
        Arc::new(entry),
    )]
    .into();
    let view =
        ingress::capabilities::CapabilityView::bind(&entries, &Default::default(), 256 << 10)
            .unwrap();
    let capabilities: Value = serde_json::from_slice(&view.0).unwrap();
    let cases = capabilities["data"][0]["request"].as_array().unwrap();
    let maximum = cases
        .iter()
        .find(|case| case["feature"] == "max_output_tokens")
        .unwrap();
    assert_eq!(maximum["treatment"], "approved_adaptation");
    assert_eq!(
        maximum["rules"],
        json!(["generation.omit-max-output-tokens.v1"])
    );
}

async fn request(gate: &Gateway, method: &str, path: &str) -> (StatusCode, Value) {
    let response = gate
        .router()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(path)
                .header(
                    "authorization",
                    format!("Bearer {}", crate::gateway::tests::support::KEY),
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    assert!(!response.headers().contains_key("x-morphiecore-adaptation"));
    assert_eq!(response.headers()["content-type"], "application/json");
    assert_eq!(response.headers()["cache-control"], "no-store");
    let bytes = to_bytes(response.into_body(), 32 << 10).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap())
}

#[tokio::test]
async fn models_list_and_retrieve_use_the_same_standard_public_object() {
    let gate = crate::gateway::tests::support::gateway(Limits::default());
    // Independent publication facts, unrelated to product catalog metadata.
    let expected = json!({
        "id":"fixture-model", "object":"model",
        "created":7, "owned_by":"Synthetic Developer"
    });
    let (status, list) = request(&gate, "GET", "/v1/models").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list, json!({"object":"list","data":[expected.clone()]}));
    let (status, model) = request(&gate, "GET", "/v1/models/fixture-model").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(model, expected);
    let (status, capabilities) = request(&gate, "GET", "/v1/morphiecore/capabilities").await;
    assert_eq!(status, StatusCode::OK);
    let (_, descriptor) = request(&gate, "GET", "/v1/morphiecore/models/fixture-model").await;
    assert_eq!(capabilities["provider_effectiveness"], "not_asserted");
    assert_eq!(capabilities["full_loop"], "not_asserted");
    let interfaces = descriptor["data"]["interfaces"].as_array().unwrap();
    let entries = capabilities["data"].as_array().unwrap();
    assert_eq!(entries.len(), interfaces.len());
    for entry in entries {
        let interface = interfaces
            .iter()
            .find(|interface| interface["protocol"] == entry["protocol"])
            .unwrap();
        assert_eq!(
            entry["agent_defaults"],
            interface["generation"]["agent_defaults"]
        );
        let request_cases = entry["request"].as_array().unwrap();
        for (feature, assessment) in interface["generation"]["features"].as_object().unwrap() {
            let case = request_cases
                .iter()
                .find(|case| case["feature"] == *feature)
                .unwrap();
            if assessment["state"] != "unavailable" {
                assert_eq!(case["treatment"], assessment["state"]);
            }
        }
        let delivery = entry["delivery"].as_array().unwrap();
        assert!(
            delivery
                .iter()
                .any(|case| case["feature"] == "generation.safety"
                    && case["domain"] == "blocking_reports_or_settings_echoes"
                    && case["treatment"] == "rejected")
        );
        assert!(
            delivery
                .iter()
                .any(|case| case["feature"] == "response.delivery.usage"
                    && case["rules"] == json!(["generation.complete-delivery-usage.v1"]))
        );
        let expected = if entry["protocol"] == "chat_completions" {
            "chat.omit-reported-settings.v1"
        } else {
            "messages.omit-incidental-settings.v1"
        };
        assert!(
            delivery
                .iter()
                .any(|case| case["feature"] == "reported_settings"
                    && case["treatment"] == "approved_adaptation"
                    && case["rules"] == json!([expected]))
        );
        for mode in ["json", "sse"] {
            assert!(
                delivery
                    .iter()
                    .any(|case| case["feature"] == "message_phase"
                        && case["domain"] == mode
                        && case["rules"] == json!(["generation.omit-delivery-message-phase.v1"]))
            );
        }
    }
}

#[tokio::test]
async fn activated_protocols_are_deduplicated_and_images_share_the_directory() {
    let gate = Gateway::new(
        crate::gateway::tests::topology::topology(),
        TaskEntries {
            generation: crate::protocol::ClientProtocol::ALL
                .into_iter()
                .map(|protocol| Entry {
                    chat_profile: Default::default(),
                    model: "fixture-model".into(),
                    protocol,
                    endpoint: crate::topology::EndpointId::new("fixture-chat").unwrap(),
                })
                .collect(),
            images: vec![ImageEntry {
                model: "fixture-image".into(),
            }],
            ..Default::default()
        },
        BTreeMap::from([(
            crate::provider::CredentialBindingId::new("fixture-key").unwrap(),
            Arc::new(SecretMaterial::new("synthetic-upstream-credential").unwrap()),
        )]),
        std::collections::BTreeMap::from([(
            "fixture-agent".into(),
            SecretMaterial::new(crate::gateway::tests::support::KEY).unwrap(),
        )]),
        Limits::default(),
        None,
    )
    .unwrap();
    let (status, list) = request(&gate, "GET", "/v1/models").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        list,
        json!({"object":"list","data":[
            {"id":"fixture-image","object":"model","created":8,"owned_by":"Synthetic Image Developer"},
            {"id":"fixture-model","object":"model","created":7,"owned_by":"Synthetic Developer"}
        ]})
    );
    let (status, image) = request(&gate, "GET", "/v1/models/fixture-image").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(image, list["data"][0]);
    let (_, capabilities) = request(&gate, "GET", "/v1/morphiecore/capabilities").await;
    assert_eq!(capabilities["data"].as_array().unwrap().len(), 3);
    assert!(
        capabilities["data"]
            .as_array()
            .unwrap()
            .iter()
            .all(|entry| entry["model"] == "fixture-model")
    );
    let responses = &capabilities["data"][1];
    assert_eq!(responses["protocol"], "responses");
    assert_eq!(
        responses["delivery"]
            .as_array()
            .unwrap()
            .iter()
            .find(|case| case["feature"] == "response.reported.settings")
            .unwrap()
            .clone(),
        json!({
            "feature":"response.reported.settings",
            "domain":"independent_reasoning_controls_without_carrier",
            "treatment":"approved_adaptation",
            "rules":["responses.omit-incidental-reasoning-controls.v1"]
        })
    );
    assert_eq!(
        responses["history"],
        json!([{
            "feature":"generation.delivered_projection",
            "domain":"completed_stored_response_within_ttl","treatment":"faithful"
        },{
            "feature":"generation.message.phase",
            "domain":"returned_native_assistant_phase","treatment":"faithful"
        }])
    );
    assert!(responses["delivery"].as_array().unwrap().contains(&json!({
        "feature":"generation.message.phase","domain":"assistant_commentary_or_final_answer",
        "treatment":"faithful"
    })));
    for domain in [
        "natural_or_matched_stop_without_tool_continuation",
        "ambiguous_stop_without_tool_continuation",
    ] {
        assert!(responses["delivery"].as_array().unwrap().contains(&json!({
            "feature":"generation.completion_cause","domain":domain,
            "treatment":"approved_adaptation",
            "rules":["responses.omit-completion-cause.v1"]
        })));
    }
    for private in [
        "private-fixture",
        "fixture-chat",
        "fixture-key",
        "synthetic-upstream-credential",
        "127.0.0.1",
    ] {
        assert!(!capabilities.to_string().contains(private));
    }
    for method in ["GET", "DELETE"] {
        for label in [
            "unknown",
            "limited-model",
            "fixture",
            "fixture%2Flimited-model",
        ] {
            let (status, error) = request(&gate, method, &format!("/v1/models/{label}")).await;
            assert_eq!(status, StatusCode::NOT_FOUND);
            assert_eq!(error["error"]["code"], "model_not_found");
            assert!(!error.to_string().contains(label));
        }
    }
    let (status, error) = request(&gate, "DELETE", "/v1/models/fixture-model").await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(error["error"]["code"], "model_deletion_forbidden");
    assert_eq!(request(&gate, "GET", "/v1/models").await.1, list);
}

#[tokio::test]
async fn discovery_authentication_and_path_failures_never_poll_or_reflect_input() {
    let gate = crate::gateway::tests::support::gateway(Limits::default());
    for (method, path) in [
        ("GET", "/v1/models"),
        ("GET", "/v1/models/fixture-model"),
        ("DELETE", "/v1/models/fixture-model"),
        ("POST", "/v1/models"),
        ("GET", "/v1/morphiecore/capabilities"),
        ("POST", "/v1/morphiecore/capabilities"),
        ("GET", "/v1/morphiecore/models"),
        ("GET", "/v1/morphiecore/models/fixture-model"),
        ("POST", "/v1/morphiecore/models"),
    ] {
        for auth in [0, 1, 2, 3] {
            let mut request = Request::builder().method(method).uri(path);
            if auth == 1 {
                request = request.header("authorization", "Bearer synthetic-wrong-token");
            }
            if auth >= 2 {
                request = request.header(
                    "authorization",
                    format!("Bearer {}", crate::gateway::tests::support::KEY),
                );
            }
            if auth == 2 {
                request = request.header(
                    "authorization",
                    format!("Bearer {}", crate::gateway::tests::support::KEY),
                );
            }
            let body = futures_util::stream::poll_fn(|_| {
                panic!("Models must never poll a request body");
                #[allow(unreachable_code)]
                std::task::Poll::Ready(None::<Result<bytes::Bytes, std::io::Error>>)
            });
            let response = gate
                .router()
                .oneshot(request.body(Body::from_stream(body)).unwrap())
                .await
                .unwrap();
            assert_eq!(
                response.status(),
                if auth == 3 {
                    if method == "DELETE" {
                        StatusCode::FORBIDDEN
                    } else if method == "POST" {
                        StatusCode::METHOD_NOT_ALLOWED
                    } else {
                        StatusCode::OK
                    }
                } else {
                    StatusCode::UNAUTHORIZED
                }
            );
            if auth != 3 {
                assert_eq!(response.headers()["www-authenticate"], "Bearer");
            }
        }
    }
    for method in ["GET", "DELETE"] {
        let (status, error) = request(&gate, method, "/v1/models/%FF").await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(error["error"]["code"], "invalid_request");
        assert!(!error.to_string().contains("%FF"));
    }
}

#[tokio::test]
async fn discovery_obeys_methods_and_shutdown_without_mutating_the_directory() {
    let gate = crate::gateway::tests::support::gateway(Limits::default());
    for (method, path) in [
        ("POST", "/v1/models"),
        ("DELETE", "/v1/models"),
        ("PUT", "/v1/models/fixture-model"),
        ("POST", "/v1/models/fixture-model"),
        ("POST", "/v1/morphiecore/capabilities"),
        ("DELETE", "/v1/morphiecore/capabilities"),
        ("POST", "/v1/morphiecore/models"),
        ("DELETE", "/v1/morphiecore/models/fixture-model"),
    ] {
        assert_eq!(
            request(&gate, method, path).await.0,
            StatusCode::METHOD_NOT_ALLOWED
        );
    }
    let head = gate
        .router()
        .oneshot(
            Request::builder()
                .method("HEAD")
                .uri("/v1/models")
                .header(
                    "authorization",
                    format!("Bearer {}", crate::gateway::tests::support::KEY),
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(head.status(), StatusCode::OK);
    assert_eq!(head.headers()["cache-control"], "no-store");
    assert!(
        to_bytes(head.into_body(), 32 << 10)
            .await
            .unwrap()
            .is_empty()
    );
    let head = gate
        .router()
        .oneshot(
            Request::builder()
                .method("HEAD")
                .uri("/v1/morphiecore/capabilities")
                .header("x-api-key", crate::gateway::tests::support::KEY)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(head.status(), StatusCode::OK);
    assert_eq!(head.headers()["cache-control"], "no-store");
    assert!(
        to_bytes(head.into_body(), 32 << 10)
            .await
            .unwrap()
            .is_empty()
    );
    gate.shutdown();
    for (method, path) in [
        ("GET", "/v1/models"),
        ("GET", "/v1/models/fixture-model"),
        ("DELETE", "/v1/models/fixture-model"),
        ("GET", "/v1/morphiecore/capabilities"),
        ("GET", "/v1/morphiecore/models"),
        ("GET", "/v1/morphiecore/models/fixture-model"),
    ] {
        let (status, error) = request(&gate, method, path).await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(error["error"]["code"], "shutting_down");
    }
}

fn aliases(count: usize, metadata: bool) -> crate::topology::CompiledTopology {
    use crate::{
        provider::{
            AuthScheme, CredentialBindingId, CredentialKind, EndpointPath, ProviderDefinition,
            ProviderId, TrustedOrigin,
        },
        topology::{
            CanonicalModel, Dispositions, Endpoint, EndpointId, EndpointTarget, ExecutionContract,
            GenerationProtocol, ModelId, PublicModel, Route, RouteId, TaskKind, compile,
        },
    };
    let canonical = ModelId::new("synthetic-canonical").unwrap();
    let provider = ProviderDefinition {
        id: ProviderId::new("synthetic").unwrap(),
        origin: TrustedOrigin::parse("http://127.0.0.1:9").unwrap(),
        endpoints: [(
            crate::provider::ProviderOperation::ChatCompletions,
            crate::provider::ProviderEndpoint {
                suffix: EndpointPath::new("/chat/completions").unwrap(),
                path: EndpointPath::new("/chat/completions").unwrap(),
                auth: None,
            },
        )]
        .into(),
        auth: AuthScheme::Bearer,
    };
    let endpoint = Endpoint {
        bound_replay_scope: None,
        id: EndpointId::new("synthetic-chat").unwrap(),
        canonical_model: canonical.clone(),
        provider: provider.id.clone(),
        target: EndpointTarget {
            origin: provider.origin.clone(),
            path: provider
                .entry(crate::provider::ProviderOperation::ChatCompletions)
                .unwrap()
                .path
                .clone(),
        },
        task: TaskKind::Generation,
        protocol: GenerationProtocol::OpenAiChat,
        upstream_model: "private-synthetic-model".into(),
        representation: TargetContract::full().into(),
        execution: ExecutionContract {
            streaming: true,
            retry_before_commit: false,
            request_body_limit: 4096,
            response_body_limit: 4096,
            timeout_ms: 1000,
            credential_kind: CredentialKind::ApiKey,
        },
        credential: CredentialBindingId::new("synthetic-key").unwrap(),
    };
    let route = Route {
        id: RouteId::new("synthetic-route").unwrap(),
        task: TaskKind::Generation,
        endpoints: vec![endpoint.id.clone()],
        policy: crate::topology::RoutePolicy::default(),
    };
    let models = (0..count)
        .map(|i| {
            PublicModel::new(
                ModelId::new(&format!("alias-{i:02}")).unwrap(),
                canonical.clone(),
                TaskKind::Generation,
                route.id.clone(),
                Dispositions::full(),
            )
        })
        .collect();
    let topology = compile(
        vec![provider],
        vec![endpoint],
        vec![route],
        models,
        vec![CanonicalModel {
            id: canonical.clone(),
            task: TaskKind::Generation,
            contract: Dispositions::full(),
        }],
    )
    .unwrap();
    if metadata {
        topology
            .with_model_metadata([(
                canonical,
                crate::topology::ModelMetadata::new(9, "Synthetic Developer").unwrap(),
            )])
            .unwrap()
    } else {
        topology
    }
}

#[test]
fn local_directory_encodes_empty_and_alias_views_with_inclusive_resource_limits() {
    use crate::gateway::ingress::models::ModelView;
    let topology = aliases(65, true);
    let empty = ModelView::bind(&topology, [], 27).unwrap();
    assert_eq!(
        serde_json::from_slice::<Value>(&empty.list).unwrap(),
        json!({"object":"list","data":[]})
    );
    assert!(matches!(
        ModelView::bind(&topology, [], 26),
        Err(StartupError::Limits)
    ));
    let labels: Vec<_> = (0..65).map(|i| format!("alias-{i:02}")).collect();
    let view =
        ModelView::bind(&topology, labels[..64].iter().map(String::as_str), 32 << 10).unwrap();
    let list: Value = serde_json::from_slice(&view.list).unwrap();
    assert_eq!(list["data"].as_array().unwrap().len(), 64);
    assert_eq!(
        list["data"][0],
        json!({"id":"alias-00","object":"model","created":9,"owned_by":"Synthetic Developer"})
    );
    assert!(
        ModelView::bind(
            &topology,
            labels[..64].iter().map(String::as_str),
            view.list.len()
        )
        .is_ok()
    );
    assert!(matches!(
        ModelView::bind(
            &topology,
            labels[..64].iter().map(String::as_str),
            view.list.len() - 1
        ),
        Err(StartupError::Limits)
    ));
    assert!(matches!(
        ModelView::bind(&topology, labels.iter().map(String::as_str), 32 << 10),
        Err(StartupError::Binding)
    ));
    assert!(matches!(
        ModelView::bind(&topology, ["unknown"], 32 << 10),
        Err(StartupError::Binding)
    ));
    assert!(matches!(
        ModelView::bind(&aliases(1, false), ["alias-00"], 32 << 10),
        Err(StartupError::Binding)
    ));

    use crate::gateway::ingress::capabilities::CapabilityView;
    let empty = CapabilityView::bind(&BTreeMap::new(), &BTreeMap::new(), 256 << 10).unwrap();
    let snapshot: Value = serde_json::from_slice(&empty.0).unwrap();
    assert_eq!(snapshot["data"], json!([]));
    let gate = crate::gateway::tests::support::gateway(Limits::default());
    let view =
        CapabilityView::bind(&gate.state.entries, &gate.state.embeddings, 256 << 10).unwrap();
    assert!(
        CapabilityView::bind(&gate.state.entries, &gate.state.embeddings, view.0.len()).is_ok()
    );
    assert!(matches!(
        CapabilityView::bind(
            &gate.state.entries,
            &gate.state.embeddings,
            view.0.len() - 1
        ),
        Err(StartupError::Limits)
    ));
    let mut entry = (*gate.state.entries.values().next().unwrap().as_ref()).clone();
    entry
        .public
        .contract
        .set_native(crate::semantic::task::generation::Feature::Safety, false);
    entry.public.contract.set_native(
        crate::semantic::task::generation::Feature::CachedContext,
        false,
    );
    entry.public.contract.set_native(
        crate::semantic::task::generation::Feature::ConditionalToolStrictness,
        false,
    );
    let entries = BTreeMap::from([(
        (
            crate::protocol::ClientProtocol::Chat,
            "fixture-model".into(),
        ),
        Arc::new(entry),
    )]);
    let view = CapabilityView::bind(&entries, &BTreeMap::new(), 256 << 10).unwrap();
    let snapshot: Value = serde_json::from_slice(&view.0).unwrap();
    assert_eq!(
        snapshot["data"][0]["request"]
            .as_array()
            .unwrap()
            .iter()
            .find(|case| case["feature"] == "function_tools")
            .unwrap()["treatment"],
        "faithful",
    );
    assert_eq!(
        snapshot["data"][0]["delivery"]
            .as_array()
            .unwrap()
            .iter()
            .find(|case| case["feature"] == "reported_settings" && case["domain"] == "json")
            .unwrap()
            .clone(),
        json!({
            "feature":"reported_settings","domain":"json",
            "treatment":"approved_adaptation","rules":["chat.omit-reported-settings.v1"]
        })
    );
}
