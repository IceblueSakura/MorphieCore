use crate::{
    DecisionEntry, Gateway, Limits, TaskEntries,
    protocol::decision::Native,
    provider::{CredentialBindingId, CredentialKind, SecretMaterial, TrustedOrigin, catalog},
    topology::{ExecutionContract, ModelId, compile, decisions::DecisionRoute},
};
use axum::{
    Json, Router,
    body::{Body, to_bytes},
    extract::Request,
    http::{HeaderMap, StatusCode},
    routing::post,
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, sync::Arc};
use tower::ServiceExt;

#[tokio::test]
async fn configured_decision_discovery_separates_sdk_families() {
    use crate::startup::bootstrap::test_support::{Fixture, service};
    let bootstrap = Fixture::new(
        service(),
        json!({
            "models": {},
            "decisions": {
                "gpt-6-luna": {"account":"platform"},
                "jev-1.14.0": {"account":"jev"}
            }
        }),
        &[
            ("openai", "platform", "synthetic-openai-key"),
            ("typesafe", "jev", "synthetic-typesafe-key"),
        ],
    )
    .unwrap();
    let router = bootstrap.gateway.router();
    for (path, pointer, label) in [
        ("/v1/models", "/data/0/id", "gpt-6-luna"),
        ("/typesafe/v1/models", "/models/0/name", "jev-1.14.0"),
        ("/v1/models/gpt-6-luna", "/id", "gpt-6-luna"),
    ] {
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri(path)
                    .header("authorization", format!("Bearer {}", super::support::KEY))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let json: Value =
            serde_json::from_slice(&to_bytes(response.into_body(), 4096).await.unwrap()).unwrap();
        assert_eq!(json.pointer(pointer), Some(&json!(label)));
        if path == "/typesafe/v1/models" {
            assert_eq!(json["models"].as_array().unwrap().len(), 1);
            assert_eq!(json["models"][0]["release_date"], "2026-10-14");
            assert!(json.get("data").is_none());
        } else if path == "/v1/models" {
            assert_eq!(json["data"].as_array().unwrap().len(), 1);
            assert!(json.get("models").is_none());
        }
    }
    let unauthed = router
        .oneshot(
            Request::builder()
                .uri("/typesafe/v1/models")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(unauthed.status(), StatusCode::UNAUTHORIZED);
    let error: Value =
        serde_json::from_slice(&to_bytes(unauthed.into_body(), 4096).await.unwrap()).unwrap();
    assert!(error["detail"].is_string());
    assert!(error.get("error").is_none());
}

#[tokio::test]
async fn generation_label_does_not_enable_decisions() {
    use crate::startup::bootstrap::test_support::{Fixture, service};
    for (activation, status) in [
        (json!({}), StatusCode::NOT_FOUND),
        (
            json!({"gpt-6-luna":{"account":"platform"}}),
            StatusCode::BAD_REQUEST,
        ),
    ] {
        let boot = Fixture::new(
            service(),
            json!({
                "models":{"gpt-6-luna":{"candidates":[{"binding":"openai-luna-responses"}]}},
                "decisions":activation,
            }),
            &[("openai", "platform", "synthetic-key")],
        )
        .unwrap();
        let router = boot.gateway.router();
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/decisions")
                    .header("authorization", format!("Bearer {}", super::support::KEY))
                    .header("content-type", "application/json")
                    .body(Body::from("invalid payload"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), status);
        let response = router
            .oneshot(
                Request::builder()
                    .uri("/v1/models")
                    .header("authorization", format!("Bearer {}", super::support::KEY))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let list: Value =
            serde_json::from_slice(&to_bytes(response.into_body(), 4096).await.unwrap()).unwrap();
        assert_eq!(list["data"].as_array().unwrap().len(), 1);
        assert_eq!(list["data"][0]["id"], "gpt-6-luna");
    }
}

#[test]
fn decision_configuration_does_not_borrow_another_providers_account() {
    use crate::startup::bootstrap::test_support::{Fixture, service};
    let boot = Fixture::new(
        service(),
        json!({
            "models":{}, "decisions":{"gpt-6-luna":{"account":"jev"}}
        }),
        &[("typesafe", "jev", "synthetic-key")],
    );
    assert!(matches!(boot, Err(crate::StartupError::Credentials)));
}

#[tokio::test]
async fn explicit_decision_routes_send_only_their_bound_native_operation() {
    const UPSTREAM_KEY: &str = "synthetic-upstream-key";
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin =
        TrustedOrigin::parse(&format!("http://{}", listener.local_addr().unwrap())).unwrap();
    let upstream = Router::new()
        .route("/v1/decisions", post(|headers: HeaderMap, Json(body): Json<Value>| async move {
            assert_eq!(headers["authorization"], format!("Bearer {UPSTREAM_KEY}"));
            assert_eq!(body, json!({"model":"luna-wire","input":"red","questions":[
                {"name":"color","type":"predicate","instructions":"Is it red?"}
            ]}));
            Json(json!({"model":"luna-wire","answers":[{"name":"color","type":"predicate","probability":0.96}],
                "usage":{"input_tokens":8,"output_tokens":0,"total_tokens":8,
                    "input_tokens_details":{"cached_tokens":0,"cache_write_tokens":0},
                    "output_tokens_details":{"reasoning_tokens":0}}}))
        }))
        .route("/v1/systemone", post(|headers: HeaderMap, Json(body): Json<Value>| async move {
            assert_eq!(headers["authorization"], format!("Bearer {UPSTREAM_KEY}"));
            assert!(!headers.contains_key("content-encoding"));
            assert_eq!(body, json!({"model":"jev-wire","state":"blue","questions":{
                "color":{"type":"noul","instructions":"Is it blue?"}
            }}));
            Json(json!({"model":"jev-wire","answers":{"color":{"type":"noul","noul":0.98}},
                "usage":{"input_tokens":7,"output_tokens":0}}))
        }));
    let upstream = tokio::spawn(async move { axum::serve(listener, upstream).await.unwrap() });
    let key = CredentialBindingId::new("native-key").unwrap();
    let mut providers = Vec::new();
    let mut routes = Vec::new();
    for (id, wire, label, native) in [
        ("openai", "luna-wire", "luna", Native::OpenAi),
        ("typesafe", "jev-wire", "jev", Native::SystemOne),
    ] {
        let mut provider = catalog::get(id).unwrap().clone();
        provider.origin = origin.clone();
        routes.push(DecisionRoute {
            model: ModelId::new(label).unwrap(),
            upstream_model: wire.into(),
            provider: provider.id.clone(),
            credential: key.clone(),
            native,
            system_one_metadata: None,
            execution: ExecutionContract {
                streaming: false,
                request_body_limit: 4096,
                response_body_limit: 4096,
                timeout_ms: 2000,
                credential_kind: CredentialKind::ApiKey,
            },
        });
        providers.push(provider);
    }
    let topology = compile(providers, vec![], vec![], vec![], vec![])
        .unwrap()
        .with_decisions(routes)
        .unwrap();
    let gateway = Gateway::new(
        topology,
        TaskEntries {
            decisions: ["luna", "jev"]
                .map(|model| DecisionEntry {
                    model: model.into(),
                })
                .into(),
            ..TaskEntries::default()
        },
        BTreeMap::from([(key, Arc::new(SecretMaterial::new(UPSTREAM_KEY).unwrap()))]),
        BTreeMap::from([(
            "test".into(),
            SecretMaterial::new(super::support::KEY).unwrap(),
        )]),
        Limits::default(),
        None,
    )
    .unwrap();
    let request = |path, body: Value, auth| {
        let mut request = Request::builder()
            .method("POST")
            .uri(path)
            .header("content-type", "application/json");
        if auth {
            request = request.header("authorization", format!("Bearer {}", super::support::KEY));
        }
        request.body(Body::from(body.to_string())).unwrap()
    };
    let router = gateway.router();
    let openai = json!({"model":"luna","input":"red","questions":[
        {"name":"color","type":"predicate","instructions":"Is it red?"}
    ]});
    let systemone = json!({"model":"jev","state":"blue","questions":{
        "color":{"type":"noul","instructions":"Is it blue?"}
    }});
    for (path, body, model, pointer, probability) in [
        (
            "/v1/decisions",
            openai,
            "luna",
            "/answers/0/probability",
            0.96,
        ),
        (
            "/typesafe/v1/systemone",
            systemone.clone(),
            "jev",
            "/answers/color/noul",
            0.98,
        ),
    ] {
        let compressed = (path == "/typesafe/v1/systemone")
            .then(|| zstd::stream::encode_all(body.to_string().as_bytes(), 0).unwrap());
        let mut request = request(path, body, true);
        if let Some(compressed) = compressed {
            request
                .headers_mut()
                .insert("content-encoding", "zstd".parse().unwrap());
            *request.body_mut() = Body::from(compressed);
        }
        let response = router.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = to_bytes(response.into_body(), 4096).await.unwrap();
        let result: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(result["model"], model);
        assert_eq!(result.pointer(pointer), Some(&json!(probability)));
    }
    let mut wrong_model = systemone;
    wrong_model["model"] = json!("luna");
    for (body, auth, status) in [
        (json!(null), false, StatusCode::UNAUTHORIZED),
        (json!(null), true, StatusCode::UNPROCESSABLE_ENTITY),
        (wrong_model, true, StatusCode::NOT_FOUND),
    ] {
        let response = router
            .clone()
            .oneshot(request("/typesafe/v1/systemone", body, auth))
            .await
            .unwrap();
        assert_eq!(response.status(), status);
        let bytes = to_bytes(response.into_body(), 4096).await.unwrap();
        let result: Value = serde_json::from_slice(&bytes).unwrap();
        assert!(result.get("detail").is_some());
        assert!(result.get("error").is_none());
    }
    upstream.abort();
}
