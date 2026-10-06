//! One Router smoke exercises independent targets, request preservation and visibility.
use super::*;
use crate::{
    lowering::generation::GenerationRepresentationContract as Representation,
    provider::{
        AuthScheme, CredentialBindingId, CredentialKind, EndpointPath, ProviderId, TrustedOrigin,
    },
    topology::*,
};
use axum::{Json, Router, http::StatusCode, response::IntoResponse, routing::post};
use serde_json::{Value, json};
use std::sync::Mutex;
#[tokio::test]
async fn ordered_route_only_falls_back_before_visible_delivery() {
    for (status, stream, policy, max_attempts, expected_calls, expected_status) in [
        (
            429,
            false,
            FallbackPolicy::BeforeCommit,
            2,
            vec!["a", "b"],
            200,
        ),
        (
            503,
            false,
            FallbackPolicy::BeforeCommit,
            2,
            vec!["a", "b"],
            200,
        ),
        (
            0,
            false,
            FallbackPolicy::BeforeCommit,
            2,
            vec!["a", "b"],
            200,
        ),
        (1, false, FallbackPolicy::BeforeCommit, 2, vec!["a"], 504),
        (302, false, FallbackPolicy::BeforeCommit, 2, vec!["a"], 502),
        (403, false, FallbackPolicy::BeforeCommit, 2, vec!["a"], 502),
        (429, false, FallbackPolicy::Disabled, 2, vec!["a"], 429),
        (429, false, FallbackPolicy::BeforeCommit, 1, vec!["a"], 429),
        (200, true, FallbackPolicy::BeforeCommit, 2, vec!["a"], 200),
    ] {
        let calls = Arc::new(Mutex::new(Vec::<&str>::new()));
        let upstream_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let upstream_addr = upstream_listener.local_addr().unwrap();
        let mut router = Router::new();
        for id in ["a", "b"] {
            let calls = calls.clone();
            router=router.route(&format!("/{id}"),post(move |headers:axum::http::HeaderMap,Json(body):Json<Value>| {
                let calls=calls.clone();
                async move {
                    assert_eq!(headers["authorization"],format!("Bearer synthetic-{id}-credential"));
                    assert_eq!(body["model"],format!("{id}-alias"));
                    assert_eq!(body["max_output_tokens"],32);
                    assert_eq!(body["prompt_cache_key"],"synthetic-affinity");
                    assert!(body.get("session_id").is_none());
                    assert_eq!(body["input"][0]["content"],json!([{"type":"input_text","text":"keep"},{"type":"input_image","image_url":"data:image/png;base64,AQ==","detail":"auto"}]));
                    calls.lock().unwrap().push(id);
                    if id=="a" && status<=1 { return std::future::pending::<axum::response::Response>().await; }
                    if id=="a" && status!=200 {return StatusCode::from_u16(status).unwrap().into_response();}
                    if stream {
                        let initial=json!({"type":"response.created","sequence_number":1,"response":{"object":"response","id":"a-r","model":"a-alias","created_at":1,"status":"in_progress","output":[]}});
                        return ([("content-type","text/event-stream")],format!("event: response.created\ndata: {initial}\n\n")).into_response();
                    }
                    Json(json!({"object":"response","id":"b-r","model":"b-alias","created_at":1,"status":"completed","output":[{"type":"message","id":"b-msg","role":"assistant","status":"completed","content":[{"type":"output_text","text":"ok","annotations":[],"logprobs":[]}]}]})).into_response()
                }
            }));
        }
        let upstream =
            tokio::spawn(async move { axum::serve(upstream_listener, router).await.unwrap() });
        let mut providers = vec![];
        let mut endpoints = vec![];
        let mut credentials = Credentials::new();
        for id in ["a", "b"] {
            let provider = ProviderDefinition {
                id: ProviderId::new(id).unwrap(),
                origin: TrustedOrigin::parse(&format!("http://{upstream_addr}")).unwrap(),
                chat_completions: Some(EndpointPath::new(&format!("/{id}")).unwrap()),
                responses: Some(EndpointPath::new(&format!("/{id}")).unwrap()),
                auth: AuthScheme::Bearer,
            };
            let credential = CredentialBindingId::new(&format!("{id}-key")).unwrap();
            credentials.insert(
                credential.clone(),
                Arc::new(SecretMaterial::new(&format!("synthetic-{id}-credential")).unwrap()),
            );
            endpoints.push(Endpoint {
                id: EndpointId::new(id).unwrap(),
                canonical_model: ModelId::new("canonical").unwrap(),
                provider: provider.id.clone(),
                target: EndpointTarget {
                    origin: provider.origin.clone(),
                    path: provider.responses.clone().unwrap(),
                },
                task: TaskKind::Generation,
                protocol: ProtocolProfile::OpenAiResponses,
                upstream_model: format!("{id}-alias"),
                representation: Adapter::new(
                    Profile::Responses,
                    crate::adapter::Dialect::MorphieCore,
                    None,
                )
                .contract(&Representation::full()),
                execution: ExecutionContract {
                    streaming: true,
                    retry_before_commit: false,
                    request_body_limit: 4096,
                    response_body_limit: 4096,
                    timeout_ms: if status == 0 && id == "a" { 50 } else { 2000 },
                    credential_kind: CredentialKind::ApiKey,
                },
                credential,
            });
            providers.push(provider);
        }
        let route = Route {
            id: RouteId::new("route").unwrap(),
            task: TaskKind::Generation,
            endpoints: vec![EndpointId::new("a").unwrap(), EndpointId::new("b").unwrap()],
            policy: RoutePolicy {
                fallback: policy,
                max_attempts,
                ..RoutePolicy::default()
            },
        };
        let public = PublicModel::new(
            ModelId::new("public").unwrap(),
            ModelId::new("canonical").unwrap(),
            TaskKind::Generation,
            route.id.clone(),
            GenerationSemanticContract::text_images(),
        );
        let topology = compile(
            providers,
            endpoints,
            vec![route],
            vec![public],
            vec![CanonicalModel {
                id: ModelId::new("canonical").unwrap(),
                task: TaskKind::Generation,
                contract: GenerationSemanticContract::full(),
            }],
        )
        .unwrap()
        .with_model_metadata([(
            ModelId::new("canonical").unwrap(),
            crate::topology::ModelMetadata::new(1, "Synthetic Developer").unwrap(),
        )])
        .unwrap();
        let gateway = Gateway::new(
            topology,
            ["b", "a"]
                .into_iter()
                .map(|id| Entry {
                    model: "public".into(),
                    protocol: Profile::Responses,
                    endpoint: EndpointId::new(id).unwrap(),
                })
                .collect(),
            credentials,
            SecretMaterial::new(super::tests::KEY).unwrap(),
            Limits {
                exchange_timeout: std::time::Duration::from_millis(if status == 1 {
                    50
                } else {
                    3000
                }),
                ..Limits::default()
            },
            None,
        )
        .unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let owned_gateway = gateway.clone();
        let serving = tokio::spawn(async move {
            owned_gateway
                .serve(listener, std::future::pending())
                .await
                .unwrap()
        });
        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        let response=tokio::time::timeout(std::time::Duration::from_secs(5),client.post(format!("http://{addr}/v1/responses")).bearer_auth(super::tests::KEY).json(&json!({"model":"public","prompt_cache_key":"synthetic-affinity","max_output_tokens":32,"stream":stream,"input":[{"role":"user","content":[{"type":"input_text","text":"keep"},{"type":"input_image","image_url":"data:image/png;base64,AQ==","detail":"auto"}]}]})).send()).await.unwrap().unwrap();
        assert_eq!(response.status().as_u16(), expected_status);
        if stream {
            assert!(
                tokio::time::timeout(std::time::Duration::from_secs(5), response.bytes())
                    .await
                    .unwrap()
                    .is_err()
            );
        } else if expected_status == 200 {
            let value: Value = response.json().await.unwrap();
            assert_eq!(value["model"], "public");
            assert_eq!(value["output"][0]["content"][0]["text"], "ok");
        }
        assert_eq!(*calls.lock().unwrap(), expected_calls);
        gateway.shutdown();
        serving.await.unwrap();
        upstream.abort();
        let _ = upstream.await;
    }
}
