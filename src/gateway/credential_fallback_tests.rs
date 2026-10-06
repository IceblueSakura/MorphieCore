//! One Router gate for credential attempts, local availability and publication boundaries.
use super::*;
use crate::{
    credential::{CredentialManager, CredentialPool, CredentialRef, Secret},
    provider::*,
    topology::*,
};
use axum::{
    Json, Router,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::post,
};
use serde_json::{Value, json};
use std::{sync::Mutex, time::Duration};

#[tokio::test]
async fn same_provider_fallback_is_ordered_bounded_and_never_crosses_publication_or_auth_errors() {
    for (status, stream, enabled, cap, expected_calls, expected_status) in [
        (503, false, true, 2, vec!["first", "second"], 200),
        (0, false, true, 2, vec!["second"], 200),
        (503, false, false, 2, vec!["first"], 502),
        (503, false, true, 1, vec!["first"], 502),
        (401, false, true, 2, vec!["first"], 502),
        (403, false, true, 2, vec!["first"], 502),
        (429, false, true, 2, vec!["first"], 429),
        (200, true, true, 2, vec!["first"], 200),
    ] {
        let dir = crate::credential::test_support::private_directory();
        let manager = CredentialManager::new(dir.path(), vec![]).unwrap();
        for alias in ["first", "second"] {
            manager
                .add_api_key(
                    "fixture",
                    alias,
                    Secret::new(format!("synthetic-{alias}")).unwrap(),
                )
                .unwrap();
        }
        if status == 0 {
            manager
                .set_api_key_enabled("fixture", "first", 1, false)
                .unwrap();
        }
        let config = CredentialPool {
            members: ["first", "second"]
                .into_iter()
                .map(|s| CredentialRef::ApiKey { alias: s.into() })
                .collect(),
            fallback: enabled,
            max_attempts: cap,
        };
        let mut credentials = Credentials::new();
        credentials
            .insert_pool(
                CredentialBindingId::new("fixture").unwrap(),
                manager.bind_pool("fixture", &config).unwrap(),
            )
            .unwrap();
        let calls = Arc::new(Mutex::new(Vec::new()));
        let observed = calls.clone();
        let upstream = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", upstream.local_addr().unwrap());
        let upstream = tokio::spawn(async move {
            axum::serve(upstream,Router::new().route("/responses",post(move |headers:HeaderMap,Json(body):Json<Value>| {
                let observed=observed.clone();
                async move {
                    let alias=if headers["authorization"]=="Bearer synthetic-first" {"first"} else {assert_eq!(headers["authorization"],"Bearer synthetic-second");"second"};
                    observed.lock().unwrap().push(alias);
                    assert_eq!(body["model"],"upstream-model");
                    assert_eq!(body["input"][0]["content"][0]["text"],"unchanged");
                    if alias=="first" && status!=200 {return StatusCode::from_u16(status).unwrap().into_response();}
                    if stream {
                        let initial=json!({"type":"response.created","sequence_number":1,"response":{"object":"response","id":"r","model":"upstream-model","created_at":1,"status":"in_progress","output":[]}});
                        return ([("content-type","text/event-stream")],format!("event: response.created\ndata: {initial}\n\n")).into_response();
                    }
                    Json(json!({"object":"response","id":"r","model":"upstream-model","created_at":1,"status":"completed","output":[{"type":"message","id":"msg","role":"assistant","status":"completed","content":[{"type":"output_text","text":"ok","annotations":[],"logprobs":[]}]}]})).into_response()
                }
            }))).await.unwrap()
        });
        let provider = ProviderDefinition {
            id: ProviderId::new("fixture").unwrap(),
            origin: TrustedOrigin::parse(&origin).unwrap(),
            responses: Some(EndpointPath::new("/responses").unwrap()),
            chat_completions: None,
            auth: AuthScheme::Bearer,
        };
        let endpoint = Endpoint {
            id: EndpointId::new("single").unwrap(),
            provider: provider.id.clone(),
            target: EndpointTarget {
                origin: provider.origin.clone(),
                path: provider.responses.clone().unwrap(),
            },
            task: TaskKind::Generation,
            protocol: ProtocolProfile::OpenAiResponses,
            upstream_model: "upstream-model".into(),
            canonical_model: ModelId::new("canonical").unwrap(),
            representation: GenerationRepresentationContract::full(),
            execution: ExecutionContract {
                streaming: true,
                retry_before_commit: false,
                request_body_limit: 4096,
                response_body_limit: 4096,
                timeout_ms: 2000,
                credential_kind: CredentialKind::ApiKey,
            },
            credential: CredentialBindingId::new("fixture").unwrap(),
        };
        let route = Route {
            id: RouteId::new("route").unwrap(),
            task: TaskKind::Generation,
            endpoints: vec![endpoint.id.clone()],
            policy: RoutePolicy::default(),
        };
        let model = PublicModel::new(
            ModelId::new("public").unwrap(),
            ModelId::new("canonical").unwrap(),
            TaskKind::Generation,
            route.id.clone(),
            GenerationSemanticContract::full(),
        );
        let topology = compile(
            vec![provider],
            vec![endpoint],
            vec![route],
            vec![model],
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
            vec![Entry {
                model: "public".into(),
                protocol: Profile::Responses,
                endpoint: EndpointId::new("single").unwrap(),
            }],
            credentials,
            SecretMaterial::new(tests::KEY).unwrap(),
            Limits::default(),
            None,
        )
        .unwrap();
        use tower::ServiceExt;
        let request = axum::http::Request::builder()
            .method("POST")
            .uri("/v1/responses")
            .header("authorization", format!("Bearer {}", tests::KEY))
            .header("content-type", "application/json")
            .body(axum::body::Body::from(
                serde_json::to_vec(&json!({"model":"public","input":"unchanged","stream":stream}))
                    .unwrap(),
            ))
            .unwrap();
        let response =
            tokio::time::timeout(Duration::from_secs(5), gateway.router().oneshot(request))
                .await
                .unwrap()
                .unwrap();
        assert_eq!(response.status().as_u16(), expected_status);
        let body = tokio::time::timeout(
            Duration::from_secs(5),
            axum::body::to_bytes(response.into_body(), 8192),
        )
        .await
        .unwrap();
        assert_eq!(body.is_err(), stream);
        assert_eq!(*calls.lock().unwrap(), expected_calls);
        gateway.shutdown();
        upstream.abort();
        let _ = upstream.await;
    }
}
