//! Independent Models wire expectations over synthetic credentials, with no upstream I/O.
use crate::gateway::*;
use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use serde_json::{Value, json};
use tower::ServiceExt;

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
    let settings = json!({
        "feature":"response.reported.settings",
        "domain":"incidental_settings_without_safety",
        "treatment":"approved_adaptation",
        "rules":["chat.omit-incidental-settings.v1"]
    });
    let assessments = json!({
        "feature":"generation.safety","domain":"assessment_only_reports",
        "treatment":"approved_adaptation","rules":["generation.omit-safety-assessment.v1"]
    });
    let safety = json!({
        "feature":"generation.safety","domain":"blocking_reports_or_settings_echoes",
        "treatment":"rejected"
    });
    let natural = json!({
        "feature":"generation.completion_cause",
        "domain":"natural_or_matched_stop_without_tool_continuation",
        "treatment":"approved_adaptation","rules":["chat.coarsen-completion-cause.v1"]
    });
    let ambiguous = json!({
        "feature":"generation.completion_cause",
        "domain":"ambiguous_stop_without_tool_continuation","treatment":"faithful"
    });
    let phase = json!({"feature":"generation.message.phase",
        "domain":"assistant_commentary_or_final_answer","treatment":"approved_adaptation",
        "rules":["generation.omit-delivery-message-phase.v1"]});
    let phase_history = json!({"feature":"generation.message.phase",
        "domain":"returned_assistant_phase","treatment":"rejected"});
    let usage = json!({"feature":"response.delivery.usage",
        "domain":"reported_then_derived_then_estimated","treatment":"approved_adaptation",
        "rules":["generation.complete-delivery-usage.v1"]});
    assert_eq!(
        capabilities,
        json!({
            "object":"morphiecore.capabilities", "version":1,
            "scope":"gateway_contract", "coverage":"partial", "unlisted":"not_assessed",
            "provider_effectiveness":"not_asserted", "full_loop":"not_asserted",
            "data":[
                {"model":"fixture-model","protocol":"chat_completions",
                    "delivery_modes":["json","sse"],"request":[],
                    "delivery":[settings,assessments.clone(),safety.clone(),usage.clone(),natural,ambiguous,phase.clone()],
                    "history":[phase_history.clone()]},
                {"model":"fixture-model","protocol":"messages",
                    "delivery_modes":["json","sse"],"request":[],
                    "delivery":[
                        {"feature":"response.reported.settings",
                            "domain":"incidental_settings_without_safety",
                            "treatment":"approved_adaptation",
                            "rules":["messages.omit-incidental-settings.v1"]},
                        assessments,
                        safety,
                        usage,
                        {"feature":"generation.completion_cause",
                            "domain":"natural_or_matched_stop_without_tool_continuation",
                            "treatment":"faithful"},
                        {"feature":"generation.completion_cause",
                            "domain":"ambiguous_stop_without_tool_continuation",
                            "treatment":"rejected"},
                        {"feature":"generation.message.phase","domain":"assistant_commentary_or_final_answer",
                            "treatment":"approved_adaptation","rules":["generation.omit-delivery-message-phase.v1"]}
                    ],"history":[{"feature":"generation.message.phase","domain":"returned_assistant_phase",
                        "treatment":"rejected"}]}
            ]
        })
    );
}

#[tokio::test]
async fn activated_protocols_are_deduplicated_and_images_share_the_directory() {
    let gate = Gateway::new(
        crate::gateway::tests::topology::topology(),
        TaskEntries {
            generation: [Profile::Responses, Profile::Chat]
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
        responses["delivery"][0],
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
            CanonicalModel, Endpoint, EndpointId, EndpointTarget, ExecutionContract,
            GenerationSemanticContract, ModelId, ProtocolProfile, PublicModel, Route, RouteId,
            TaskKind, compile,
        },
    };
    let canonical = ModelId::new("synthetic-canonical").unwrap();
    let provider = ProviderDefinition {
        id: ProviderId::new("synthetic").unwrap(),
        origin: TrustedOrigin::parse("http://127.0.0.1:9").unwrap(),
        endpoints: [(
            crate::provider::ProviderOperation::ChatCompletions,
            crate::provider::ProviderEndpoint {
                path: EndpointPath::new("/chat/completions").unwrap(),
                auth: None,
            },
        )]
        .into(),
        credential_pool: None,
        auth: AuthScheme::Bearer,
    };
    let endpoint = Endpoint {
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
        protocol: ProtocolProfile::OpenAiChat,
        upstream_model: "private-synthetic-model".into(),
        representation: TargetCaps::full().into(),
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
                GenerationSemanticContract::full(),
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
            contract: GenerationSemanticContract::full(),
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
    entry.public.contract.safety = false;
    entry.public.contract.cached_context = false;
    entry.public.contract.normalization_default_tools = false;
    let crate::adapter::ClientAdapter::OpenAi { contract, .. } = &mut entry.client else {
        panic!("OpenAI entry")
    };
    contract.reported_facts = crate::lowering::generation::ReportedFactPolicy::StrictComplete;
    let entries = BTreeMap::from([(
        (family(Profile::Chat), "fixture-model".into()),
        Arc::new(entry),
    )]);
    let view = CapabilityView::bind(&entries, &BTreeMap::new(), 256 << 10).unwrap();
    let snapshot: Value = serde_json::from_slice(&view.0).unwrap();
    assert_eq!(
        snapshot["data"][0]["request"],
        json!([
            {"feature":"generation.safety","domain":"explicit_policy","treatment":"rejected"},
            {"feature":"generation.cached_context","domain":"existing_reference","treatment":"rejected"},
            {"feature":"generation.tools","domain":"normalization_default_strictness","treatment":"rejected"}
        ])
    );
    assert_eq!(
        snapshot["data"][0]["delivery"][0],
        json!({
            "feature":"response.reported.settings","domain":"incidental_settings_without_safety",
            "treatment":"rejected"
        })
    );
}
