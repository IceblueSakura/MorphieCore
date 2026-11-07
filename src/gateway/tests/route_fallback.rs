//! One Router smoke exercises independent targets, request preservation and visibility.
use crate::lowering::disposition::Dispositions;
use crate::{
    gateway::*,
    protocol::openai::TargetContract as Representation,
    provider::{
        AuthScheme, CredentialBindingId, CredentialKind, EndpointPath, ProviderId, TrustedOrigin,
    },
    topology::*,
};
use axum::{Json, Router, http::StatusCode, response::IntoResponse, routing::post};
use serde_json::{Value, json};
use std::sync::Mutex;
#[tokio::test]
async fn selected_target_never_changes_after_dispatch() {
    for (status, stream, expected_calls, expected_status) in [
        (429, false, vec!["a"], 429),
        (503, false, vec!["a", "a"], 200),
        (402, false, vec!["a"], 429),
        (0, false, vec!["a"], 504),
        (1, false, vec!["a"], 504),
        (2, false, vec!["a"], 502),
        (302, false, vec!["a"], 502),
        (403, false, vec!["a"], 502),
        (200, true, vec!["a"], 502),
        (201, true, vec!["a"], 200),
        (202, true, vec!["a"], 502),
    ] {
        let calls = Arc::new(Mutex::new(Vec::<&str>::new()));
        let cut_stream = Arc::new(tokio::sync::Notify::new());
        let upstream_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let upstream_addr = upstream_listener.local_addr().unwrap();
        let mut router = Router::new();
        for id in ["a", "b"] {
            let calls = calls.clone();
            let cut_stream = cut_stream.clone();
            router=router.route(&format!("/{id}"),post(move |headers:axum::http::HeaderMap,Json(body):Json<Value>| {
                let calls=calls.clone();
                let cut_stream=cut_stream.clone();
                async move {
                    assert_eq!(headers["authorization"],format!("Bearer synthetic-{id}-credential"));
                    assert_eq!(body["model"],format!("{id}-alias"));
                    assert_eq!(body["max_output_tokens"],32);
                    assert_eq!(body["prompt_cache_key"],"synthetic-affinity");
                    assert_eq!(body["metadata"],json!({"ticket":"original"}));
                    assert!(body.get("session_id").is_none());
                    assert!(!headers.contains_key("x-session-affinity"));
                    assert_eq!(body["input"][0]["content"],json!([{"type":"input_text","text":"keep"},{"type":"input_image","image_url":"data:image/png;base64,AQ==","detail":"auto"}]));
                    let first = {
                        let mut calls = calls.lock().unwrap();
                        calls.push(id);
                        calls.len() == 1
                    };
                    if id == "a" && status == 503 && !first {
                        return ([("content-type","text/event-stream")], super::responses_wire::response_stream(json!({"object":"response","id":"retry-r","model":"a-alias","created_at":1,"status":"completed","output":[{"type":"message","id":"retry-msg","role":"assistant","status":"completed","content":[{"type":"output_text","text":"ok","annotations":[],"logprobs":[]}]}]}))).into_response();
                    }
                    if id=="a" && status<=1 { return std::future::pending::<axum::response::Response>().await; }
                    if id=="a" && status==2 { return ([("content-type","text/html")],"<html>").into_response(); }
                    // A zero hint supplies no future reset; fallback cooldown applies.
                    if id=="a" && status==429 {return (StatusCode::TOO_MANY_REQUESTS,[("retry-after","0")]).into_response();}
                    if id=="a" && status!=200 && status!=201 && status!=202 {return StatusCode::from_u16(status).unwrap().into_response();}
                    if stream {
                        let mut initial=json!({"type":"response.created","sequence_number":1,"response":{"object":"response","id":"a-r","model":"a-alias","created_at":1,"status":"in_progress","output":[]}});
                        if status==202 {
                            if id=="a" {
                                initial["response"]["metadata"]=json!({"failed_candidate":"not-delivered"});
                            } else {
                                initial["response"]["id"]=json!("b-r");
                                initial["response"]["model"]=json!("b-alias");
                                let mut terminal=initial.clone();
                                terminal["type"]=json!("response.completed");
                                terminal["sequence_number"]=json!(2);
                                terminal["response"]["status"]=json!("completed");
                                return ([("content-type","text/event-stream")],
                                    format!("data: {initial}\n\ndata: {terminal}\n\n")).into_response();
                            }
                        }
                        if status==201 {
                            let frames = [
                                initial,
                                json!({"type":"response.output_item.added","sequence_number":2,"output_index":0,"item":{"id":"msg","type":"message","role":"assistant","status":"in_progress","content":[]}}),
                                json!({"type":"response.content_part.added","sequence_number":3,"output_index":0,"item_id":"msg","content_index":0,"part":{"type":"output_text","text":"","annotations":[]}}),
                                json!({"type":"response.output_text.delta","sequence_number":4,"output_index":0,"item_id":"msg","content_index":0,"delta":"visible","logprobs":[]}),
                            ].iter().map(|v|format!("data: {v}\n\n")).collect::<String>();
                            use futures_util::StreamExt;
                            let frames = futures_util::stream::once(async move { Ok::<_,std::convert::Infallible>(bytes::Bytes::from(frames)) })
                                .chain(futures_util::stream::once(async move { cut_stream.notified().await; Ok(bytes::Bytes::new()) }));
                            return ([("content-type","text/event-stream")],axum::body::Body::from_stream(frames)).into_response();
                        }
                        return ([("content-type","text/event-stream")],format!("event: response.created\ndata: {initial}\n\n")).into_response();
                    }
                    ([("content-type","text/event-stream")], super::responses_wire::response_stream(json!({"object":"response","id":"b-r","model":"b-alias","created_at":1,"status":"completed","output":[{"type":"message","id":"b-msg","role":"assistant","status":"completed","content":[{"type":"output_text","text":"ok","annotations":[],"logprobs":[]}]}]}))).into_response()
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
                endpoints: [
                    (
                        crate::provider::ProviderOperation::ChatCompletions,
                        crate::provider::ProviderEndpoint {
                            suffix: EndpointPath::new(&format!("/{id}")).unwrap(),
                            path: EndpointPath::new(&format!("/{id}")).unwrap(),
                            auth: None,
                        },
                    ),
                    (
                        crate::provider::ProviderOperation::Responses,
                        crate::provider::ProviderEndpoint {
                            suffix: EndpointPath::new(&format!("/{id}")).unwrap(),
                            path: EndpointPath::new(&format!("/{id}")).unwrap(),
                            auth: None,
                        },
                    ),
                ]
                .into(),
                auth: AuthScheme::Bearer,
            };
            let credential = CredentialBindingId::new(&format!("{id}-key")).unwrap();
            credentials.insert(
                credential.clone(),
                Arc::new(SecretMaterial::new(&format!("synthetic-{id}-credential")).unwrap()),
            );
            endpoints.push(Endpoint {
                bound_replay_scope: None,
                id: EndpointId::new(id).unwrap(),
                canonical_model: ModelId::new("canonical").unwrap(),
                provider: provider.id.clone(),
                target: EndpointTarget {
                    origin: provider.origin.clone(),
                    path: provider
                        .entry(crate::provider::ProviderOperation::Responses)
                        .unwrap()
                        .path
                        .clone(),
                },
                task: TaskKind::Generation,
                protocol: GenerationProtocol::OpenAiResponses,
                upstream_model: format!("{id}-alias"),
                representation: GenerationCodec::new(
                    GenerationApi::Responses,
                    crate::protocol::openai::Dialect::MorphieCore,
                    None,
                )
                .contract(&Representation::full())
                .into(),
                execution: ExecutionContract {
                    streaming: true,
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
                total_timeout_ms: (status == 1).then_some(50),
                ..RoutePolicy::default()
            },
        };
        let public = PublicModel::new(
            ModelId::new("public").unwrap(),
            ModelId::new("canonical").unwrap(),
            TaskKind::Generation,
            route.id.clone(),
            Dispositions::text_images(),
        );
        let topology = compile(
            providers,
            endpoints,
            vec![route],
            vec![public],
            vec![CanonicalModel {
                id: ModelId::new("canonical").unwrap(),
                task: TaskKind::Generation,
                contract: Dispositions::full(),
            }],
        )
        .unwrap()
        .with_model_metadata([(
            ModelId::new("canonical").unwrap(),
            crate::topology::ModelMetadata::new(1, "Synthetic Developer").unwrap(),
        )])
        .unwrap();
        let diagnostics = crate::test_files::private_directory();
        let diagnostic_path = diagnostics.path().join("events.jsonl");
        let mut gateway = Gateway::new(
            topology,
            TaskEntries {
                generation: ["b", "a"]
                    .into_iter()
                    .map(|id| Entry {
                        chat_profile: Default::default(),
                        model: "public".into(),
                        protocol: crate::protocol::ClientProtocol::Responses,
                        endpoint: EndpointId::new(id).unwrap(),
                    })
                    .collect(),
                ..Default::default()
            },
            credentials,
            std::collections::BTreeMap::from([(
                "fixture-agent".into(),
                SecretMaterial::new(crate::gateway::tests::support::KEY).unwrap(),
            )]),
            Limits {
                exchange_timeout: std::time::Duration::from_millis(3000),
                ..Limits::default()
            },
            None,
        )
        .unwrap();
        if status == 503 && !stream {
            Arc::get_mut(&mut gateway.state).unwrap().diagnostics =
                Some(crate::gateway::diagnostics::Sink::open(&diagnostic_path).unwrap());
        }
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
        let send = |session: bool| {
            let mut request = client
                .post(format!("http://{addr}/v1/responses"))
                .bearer_auth(crate::gateway::tests::support::KEY);
            if session {
                request = request.header("x-session-affinity", "synthetic-conversation");
            }
            tokio::time::timeout(std::time::Duration::from_secs(5),request.json(&json!({"model":"public","prompt_cache_key":"synthetic-affinity","max_output_tokens":32,"stream":stream,
                "metadata":{"ticket":"original"},"input":[{"role":"user","content":[{"type":"input_text","text":"keep"},{"type":"input_image","image_url":"data:image/png;base64,AQ==","detail":"auto"}]}]})).send())
        };
        let mut response = send(true).await.unwrap().unwrap();
        assert_eq!(response.status().as_u16(), expected_status);
        if status == 201 {
            let mut received = Vec::new();
            loop {
                match tokio::time::timeout(std::time::Duration::from_secs(5), response.chunk())
                    .await
                    .unwrap()
                {
                    Ok(Some(bytes)) => {
                        received.extend_from_slice(&bytes);
                        if String::from_utf8_lossy(&received).contains("visible") {
                            cut_stream.notify_one();
                        }
                    }
                    Ok(None) => panic!("truncated response must not finish normally"),
                    Err(_) => break,
                }
            }
            assert!(String::from_utf8_lossy(&received).contains("visible"));
        } else if stream && expected_status == 200 {
            assert!(
                tokio::time::timeout(std::time::Duration::from_secs(5), response.bytes())
                    .await
                    .unwrap()
                    .is_err()
            );
        } else if expected_status == 200 {
            let value: Value = response.json().await.unwrap();
            assert_eq!(value["model"], "public");
            assert_eq!(value["metadata"], json!({"ticket":"original"}));
            assert_eq!(value["output"][0]["content"][0]["text"], "ok");
        } else {
            let value: Value = response.json().await.unwrap();
            assert!(value["error"]["code"].is_string());
            assert!(!value.to_string().contains("failed_candidate"));
        }
        assert_eq!(*calls.lock().unwrap(), expected_calls);
        gateway.shutdown();
        serving.await.unwrap();
        upstream.abort();
        let _ = upstream.await;
    }
}

#[tokio::test]
async fn recovery_ownership_spans_final_handoff_and_distinguishes_deadlines() {
    use std::{
        sync::atomic::{AtomicUsize, Ordering},
        time::Duration,
    };
    use tower::ServiceExt;
    let calls = Arc::new(AtomicUsize::new(0));
    let mode = Arc::new(AtomicUsize::new(0));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let observed = calls.clone();
    let selected = mode.clone();
    let router = Router::new().route("/chat/completions", post(move || {
        let calls = observed.clone();
        let mode = selected.clone();
        async move {
            calls.fetch_add(1, Ordering::SeqCst);
            match mode.load(Ordering::SeqCst) {
                0 => (StatusCode::SERVICE_UNAVAILABLE, [("retry-after", "0")]).into_response(),
                1 => Json(json!({"id":"synthetic","object":"chat.completion","created":1,"model":"private-fixture","choices":[{"index":0,"message":{"role":"assistant","content":"ok"},"finish_reason":"stop"}]})).into_response(),
                _ => std::future::pending().await,
            }
        }
    }));
    let upstream = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let mut credentials = Credentials::new();
    credentials.insert(
        CredentialBindingId::new("fixture-key").unwrap(),
        Arc::new(SecretMaterial::new("synthetic-key").unwrap()),
    );
    let gate = Gateway::new(
        super::topology::topology_at(&origin),
        TaskEntries {
            generation: vec![Entry {
                chat_profile: Default::default(),
                model: "fixture-model".into(),
                protocol: crate::protocol::ClientProtocol::Chat,
                endpoint: EndpointId::new("fixture-chat").unwrap(),
            }],
            ..Default::default()
        },
        credentials,
        std::collections::BTreeMap::from([(
            "fixture-agent".into(),
            SecretMaterial::new(super::support::KEY).unwrap(),
        )]),
        Limits {
            exchange_timeout: Duration::from_millis(100),
            ..Default::default()
        },
        None,
    )
    .unwrap();
    let send = || {
        tokio::time::timeout(Duration::from_secs(3), gate.router().oneshot(
        axum::http::Request::builder().method("POST").uri("/v1/chat/completions")
            .header("authorization",format!("Bearer {}",super::support::KEY))
            .header("content-type","application/json")
            .body(axum::body::Body::from(r#"{"model":"fixture-model","messages":[{"role":"user","content":"synthetic"}]}"#)).unwrap()
    ))
    };
    assert_eq!(send().await.unwrap().unwrap().status(), 503);
    tokio::time::pause();
    tokio::time::advance(Duration::from_secs(1)).await;
    tokio::time::resume();
    mode.store(1, Ordering::SeqCst);
    let probe = send().await.unwrap().unwrap();
    assert_eq!(probe.status(), 200);
    // Upstream JSON and EOF are already valid, but the final body is unpolled.
    let unavailable = send().await.unwrap().unwrap();
    assert_eq!(unavailable.status(), 503);
    assert_eq!(calls.load(Ordering::SeqCst), 3);
    drop(probe);
    drop(
        tokio::time::timeout(
            Duration::from_secs(1),
            gate.state.permits.clone().acquire_many_owned(16),
        )
        .await
        .unwrap()
        .unwrap(),
    );
    let retry = send().await.unwrap().unwrap();
    assert_eq!(retry.status(), 200);
    axum::body::to_bytes(retry.into_body(), 8192).await.unwrap();
    drop(
        tokio::time::timeout(
            Duration::from_secs(1),
            gate.state.permits.clone().acquire_many_owned(16),
        )
        .await
        .unwrap()
        .unwrap(),
    );
    // An unpolled successful reply times out locally, without cooling upstream.
    let slow = send().await.unwrap().unwrap();
    assert_eq!(slow.status(), 200);
    tokio::time::pause();
    tokio::time::advance(Duration::from_secs(2)).await;
    tokio::time::resume();
    drop(
        tokio::time::timeout(
            Duration::from_secs(1),
            gate.state.permits.clone().acquire_many_owned(16),
        )
        .await
        .unwrap()
        .unwrap(),
    );
    assert!(axum::body::to_bytes(slow.into_body(), 8192).await.is_err());
    let healthy = send().await.unwrap().unwrap();
    assert_eq!(healthy.status(), 200);
    axum::body::to_bytes(healthy.into_body(), 8192)
        .await
        .unwrap();
    mode.store(2, Ordering::SeqCst);
    assert_eq!(send().await.unwrap().unwrap().status(), 504);
    let count = calls.load(Ordering::SeqCst);
    assert_eq!(send().await.unwrap().unwrap().status(), 503);
    assert_eq!(calls.load(Ordering::SeqCst), count);
    gate.shutdown();
    upstream.abort();
    let _ = upstream.await;
}
