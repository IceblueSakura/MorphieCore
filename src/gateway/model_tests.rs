//! Independent Models wire expectations over synthetic credentials, with no upstream I/O.
use super::*;
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
                .header("authorization", format!("Bearer {}", tests::KEY))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    assert_eq!(response.headers()["content-type"], "application/json");
    assert_eq!(response.headers()["cache-control"], "no-store");
    let bytes = to_bytes(response.into_body(), 32 << 10).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap())
}

#[tokio::test]
async fn models_list_and_retrieve_use_the_same_standard_public_object() {
    let gate = tests::gateway(Limits::default());
    // Official release date, not OpenRouter's distinct directory timestamp.
    let expected = json!({
        "id":"deepseek-flash", "object":"model",
        "created":1788998400_u64, "owned_by":"DeepSeek"
    });
    let (status, list) = request(&gate, "GET", "/v1/models").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list, json!({"object":"list","data":[expected.clone()]}));
    let (status, model) = request(&gate, "GET", "/v1/models/deepseek-flash").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(model, expected);
}

#[tokio::test]
async fn activated_protocols_are_deduplicated_and_images_share_the_directory() {
    let gate = Gateway::new_with_images(
        crate::topology::catalog::default_topology().unwrap(),
        [Profile::Responses, Profile::Chat]
            .into_iter()
            .map(|protocol| Entry {
                model: "deepseek-flash".into(),
                protocol,
                endpoint: crate::topology::EndpointId::new("deepseek-chat").unwrap(),
            })
            .collect(),
        vec![ImageEntry {
            model: "gpt-image-2.5-flare".into(),
        }],
        BTreeMap::from([
            (
                crate::provider::CredentialBindingId::new("deepseek-api-key").unwrap(),
                Arc::new(SecretMaterial::new("synthetic-deepseek-credential").unwrap()),
            ),
            (
                crate::provider::CredentialBindingId::new("openrouter-api-key").unwrap(),
                Arc::new(SecretMaterial::new("synthetic-router-credential").unwrap()),
            ),
        ]),
        SecretMaterial::new(tests::KEY).unwrap(),
        Limits::default(),
        None,
    )
    .unwrap();
    let (status, list) = request(&gate, "GET", "/v1/models").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        list,
        json!({"object":"list","data":[
            {"id":"deepseek-flash","object":"model","created":1788998400_u64,"owned_by":"DeepSeek"},
            {"id":"gpt-image-2.5-flare","object":"model","created":1788825600_u64,"owned_by":"OpenAI"}
        ]})
    );
    let (status, image) = request(&gate, "GET", "/v1/models/gpt-image-2.5-flare").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(image, list["data"][1]);
    for method in ["GET", "DELETE"] {
        for label in ["unknown", "gpt-6-luna", "OpenAI", "openai%2Fgpt-6-luna"] {
            let (status, error) = request(&gate, method, &format!("/v1/models/{label}")).await;
            assert_eq!(status, StatusCode::NOT_FOUND);
            assert_eq!(error["error"]["code"], "model_not_found");
            assert!(!error.to_string().contains(label));
        }
    }
    let (status, error) = request(&gate, "DELETE", "/v1/models/deepseek-flash").await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(error["error"]["code"], "model_deletion_forbidden");
    assert_eq!(request(&gate, "GET", "/v1/models").await.1, list);
}

#[tokio::test]
async fn discovery_authentication_and_path_failures_never_poll_or_reflect_input() {
    let gate = tests::gateway(Limits::default());
    for (method, path) in [
        ("GET", "/v1/models"),
        ("GET", "/v1/models/deepseek-flash"),
        ("DELETE", "/v1/models/deepseek-flash"),
        ("POST", "/v1/models"),
    ] {
        for auth in [0, 1, 2, 3] {
            let mut request = Request::builder().method(method).uri(path);
            if auth == 1 {
                request = request.header("authorization", "Bearer synthetic-wrong-token");
            }
            if auth >= 2 {
                request = request.header("authorization", format!("Bearer {}", tests::KEY));
            }
            if auth == 2 {
                request = request.header("authorization", format!("Bearer {}", tests::KEY));
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
    let gate = tests::gateway(Limits::default());
    for (method, path) in [
        ("POST", "/v1/models"),
        ("DELETE", "/v1/models"),
        ("PUT", "/v1/models/deepseek-flash"),
        ("POST", "/v1/models/deepseek-flash"),
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
                .header("authorization", format!("Bearer {}", tests::KEY))
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
        ("GET", "/v1/models/deepseek-flash"),
        ("DELETE", "/v1/models/deepseek-flash"),
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
        chat_completions: Some(EndpointPath::new("/chat/completions").unwrap()),
        responses: None,
        auth: AuthScheme::Bearer,
    };
    let endpoint = Endpoint {
        id: EndpointId::new("synthetic-chat").unwrap(),
        canonical_model: canonical.clone(),
        provider: provider.id.clone(),
        target: EndpointTarget {
            origin: provider.origin.clone(),
            path: provider.chat_completions.clone().unwrap(),
        },
        task: TaskKind::Generation,
        protocol: ProtocolProfile::OpenAiChat,
        upstream_model: "private-synthetic-model".into(),
        representation: GenerationRepresentationContract::full(),
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
    use super::models::ModelView;
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
}

#[test]
fn startup_requires_publication_facts_only_for_activated_models() {
    let build = |metadata, cap| {
        Gateway::new(
            aliases(2, metadata),
            vec![Entry {
                model: "alias-00".into(),
                protocol: Profile::Chat,
                endpoint: crate::topology::EndpointId::new("synthetic-chat").unwrap(),
            }],
            BTreeMap::from([(
                crate::provider::CredentialBindingId::new("synthetic-key").unwrap(),
                Arc::new(SecretMaterial::new("synthetic-upstream-credential").unwrap()),
            )]),
            SecretMaterial::new(tests::KEY).unwrap(),
            Limits {
                response_bytes: cap,
                ..Limits::default()
            },
            None,
        )
    };
    assert!(matches!(build(false, 32 << 10), Err(StartupError::Binding)));
    assert!(matches!(build(true, 1), Err(StartupError::Limits)));
    let gate = build(true, 32 << 10).unwrap();
    assert_eq!(gate.state.models.models.len(), 1);
    assert!(gate.state.models.models.contains_key("alias-00"));
}
