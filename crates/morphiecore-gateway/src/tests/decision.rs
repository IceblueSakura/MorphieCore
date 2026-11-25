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
        let response = router
            .clone()
            .oneshot(request(path, body, true))
            .await
            .unwrap();
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
