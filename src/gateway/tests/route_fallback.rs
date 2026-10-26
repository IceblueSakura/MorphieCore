//! One Router smoke exercises independent targets, request preservation and visibility.
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
        // Exhausted quota or credit advances like a rate limit.
        (
            402,
            false,
            FallbackPolicy::BeforeCommit,
            2,
            vec!["a", "b"],
            200,
        ),
        (1, false, FallbackPolicy::BeforeCommit, 2, vec!["a"], 504),
        // A 200 that is not an API reply (an HTML page) fails over like a 5xx.
        (
            2,
            false,
            FallbackPolicy::BeforeCommit,
            2,
            vec!["a", "b"],
            200,
        ),
        (302, false, FallbackPolicy::BeforeCommit, 2, vec!["a"], 502),
        (403, false, FallbackPolicy::BeforeCommit, 2, vec!["a"], 502),
        (429, false, FallbackPolicy::Disabled, 2, vec!["a"], 429),
        (429, false, FallbackPolicy::BeforeCommit, 1, vec!["a"], 429),
        // Only `response.created` before the cut: held, so the next candidate answers.
        (
            200,
            true,
            FallbackPolicy::BeforeCommit,
            2,
            vec!["a", "b"],
            200,
        ),
        // Visible content followed by EOF must not call the next target.
        (201, true, FallbackPolicy::BeforeCommit, 2, vec!["a"], 200),
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
                    assert!(body.get("session_id").is_none());
                    assert!(!headers.contains_key("x-session-affinity"));
                    assert_eq!(body["input"][0]["content"],json!([{"type":"input_text","text":"keep"},{"type":"input_image","image_url":"data:image/png;base64,AQ==","detail":"auto"}]));
                    calls.lock().unwrap().push(id);
                    if id=="a" && status<=1 { return std::future::pending::<axum::response::Response>().await; }
                    if id=="a" && status==2 { return ([("content-type","text/html")],"<html>").into_response(); }
                    // The rate limit resets at once, so its cooldown has already expired.
                    if id=="a" && status==429 {return (StatusCode::TOO_MANY_REQUESTS,[("retry-after","0")]).into_response();}
                    if id=="a" && status!=200 && status!=201 {return StatusCode::from_u16(status).unwrap().into_response();}
                    if stream {
                        let initial=json!({"type":"response.created","sequence_number":1,"response":{"object":"response","id":"a-r","model":"a-alias","created_at":1,"status":"in_progress","output":[]}});
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
                    Json(json!({"object":"response","id":"b-r","model":"b-alias","created_at":1,"status":"completed","output":[{"type":"message","id":"b-msg","role":"assistant","status":"completed","content":[{"type":"output_text","text":"ok","annotations":[],"logprobs":[]}]}]})).into_response()
                }
            }));
        }
        let upstream =
            tokio::spawn(async move { axum::serve(upstream_listener, router).await.unwrap() });
        let mut providers = vec![];
        let mut endpoints = vec![];
        let mut credentials = Credentials::new();
        let directory = crate::test_files::private_directory();
        let manager = crate::credential::CredentialManager::new(directory.path(), vec![]).unwrap();
        for id in ["a", "b"] {
            let provider = ProviderDefinition {
                id: ProviderId::new(id).unwrap(),
                origin: TrustedOrigin::parse(&format!("http://{upstream_addr}")).unwrap(),
                endpoints: [
                    (
                        crate::provider::ProviderOperation::ChatCompletions,
                        crate::provider::ProviderEndpoint {
                            path: EndpointPath::new(&format!("/{id}")).unwrap(),
                            auth: None,
                        },
                    ),
                    (
                        crate::provider::ProviderOperation::Responses,
                        crate::provider::ProviderEndpoint {
                            path: EndpointPath::new(&format!("/{id}")).unwrap(),
                            auth: None,
                        },
                    ),
                ]
                .into(),
                credential_pool: None,
                auth: AuthScheme::Bearer,
            };
            let credential = CredentialBindingId::new(&format!("{id}-key")).unwrap();
            credentials.insert(
                credential.clone(),
                Arc::new(SecretMaterial::new(&format!("synthetic-{id}-credential")).unwrap()),
            );
            if id == "a" && status == 429 {
                for (alias, material) in [
                    ("primary", "synthetic-a-credential"),
                    ("spare", "synthetic-a-spare"),
                ] {
                    manager
                        .add_api_key(
                            "a",
                            alias,
                            crate::credential::Secret::new(material.into()).unwrap(),
                        )
                        .unwrap();
                }
                credentials
                    .insert_pool(
                        credential.clone(),
                        manager
                            .bind_pool(
                                "a",
                                &crate::credential::CredentialPool {
                                    members: ["primary", "spare"]
                                        .map(|alias| crate::credential::CredentialRef::ApiKey {
                                            alias: alias.into(),
                                        })
                                        .into(),
                                    fallback: true,
                                    max_attempts: 2,
                                },
                            )
                            .unwrap(),
                    )
                    .unwrap();
            }
            endpoints.push(Endpoint {
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
                total_timeout_ms: (status == 1).then_some(50),
                ..RoutePolicy::default()
            },
        };
        let public = PublicModel::new(
            ModelId::new("public").unwrap(),
            ModelId::new("canonical").unwrap(),
            TaskKind::Generation,
            route.id.clone(),
            GenerationFeatureSet::text_images(),
        );
        let topology = compile(
            providers,
            endpoints,
            vec![route],
            vec![public],
            vec![CanonicalModel {
                id: ModelId::new("canonical").unwrap(),
                task: TaskKind::Generation,
                contract: GenerationFeatureSet::full(),
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
            tokio::time::timeout(std::time::Duration::from_secs(5),request.json(&json!({"model":"public","prompt_cache_key":"synthetic-affinity","max_output_tokens":32,"stream":stream,"input":[{"role":"user","content":[{"type":"input_text","text":"keep"},{"type":"input_image","image_url":"data:image/png;base64,AQ==","detail":"auto"}]}]})).send())
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
        } else if stream {
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
        if expected_calls == ["a", "b"] && !stream {
            if status == 503 {
                // A legal suffix blocked solely by health returns local
                // availability, not the last upstream failure.
                let entry = &gateway.state.entries
                    [&(crate::protocol::ClientProtocol::Responses, "public".into())];
                let b = entry
                    .candidates
                    .iter()
                    .find(|c| c.provider.id.as_str() == "b")
                    .unwrap();
                let lease = gateway
                    .state
                    .executors
                    .acquire(b, tokio::time::Instant::now())
                    .unwrap();
                lease.failed(
                    ApiError::failure(429, b"", Some(60)),
                    tokio::time::Instant::now(),
                );
                drop(lease);
                tokio::time::pause();
                tokio::time::advance(std::time::Duration::from_secs(1)).await;
                tokio::time::resume();
                calls.lock().unwrap().clear();
                let response = send(false).await.unwrap().unwrap();
                assert_eq!(response.status(), 503);
                assert!(response.headers().contains_key("retry-after"));
                assert_eq!(
                    response.json::<Value>().await.unwrap()["error"]["code"],
                    "upstream_unavailable"
                );
                assert_eq!(*calls.lock().unwrap(), ["a"]);
                gateway.flush_diagnostics().await;
                let records: Vec<Value> = std::fs::read_to_string(&diagnostic_path)
                    .unwrap()
                    .lines()
                    .map(|line| serde_json::from_str(line).unwrap())
                    .filter(|value: &Value| value["event"] == "request")
                    .collect();
                let recovered = &records[0]["generation"];
                assert_eq!(recovered["result"], "succeeded");
                assert!(recovered["failure_code"].is_null());
                assert_eq!(recovered["attempts"][0]["advanced"], true);
                let blocked = records.last().unwrap();
                assert_eq!(blocked["generation"]["failure_code"], "gateway.unavailable");
                assert_eq!(blocked["generation"]["failure_status_hint"], 503);
                assert_eq!(
                    blocked["generation"]["attempts"].as_array().unwrap().len(),
                    1
                );
                assert_eq!(blocked["generation"]["attempts"][0]["advanced"], false);
                assert_eq!(blocked["candidates"][0]["advanced"], false);
                assert_eq!(blocked["candidates"][0]["health"]["reason"], "failure");
                assert_eq!(blocked["candidates"][0]["health"]["scope"], "target");
                assert_eq!(blocked["health"]["reason"], "cooling");
                assert_eq!(blocked["health"]["scope"], "pool");
                // Expire both scopes before the remaining affinity assertions.
                tokio::time::pause();
                tokio::time::advance(std::time::Duration::from_secs(60)).await;
                tokio::time::resume();
            }
            // Upstream failures now gate dispatch; a reported immediate reset
            // admits one recovery probe rather than bypassing cooling.
            calls.lock().unwrap().clear();
            assert_eq!(send(false).await.unwrap().unwrap().status(), 200);
            let cooled = !matches!(status, 429 | 503);
            assert_eq!(
                *calls.lock().unwrap(),
                if cooled { vec!["b"] } else { vec!["a", "b"] }
            );
            // The conversation stays on the executor that answered it last.
            calls.lock().unwrap().clear();
            assert_eq!(send(true).await.unwrap().unwrap().status(), 200);
            assert_eq!(*calls.lock().unwrap(), ["b"]);
        }
        if status == 201 {
            calls.lock().unwrap().clear();
            let response = send(true).await.unwrap().unwrap();
            assert_eq!(response.status(), 200);
            drop(response);
            assert_eq!(*calls.lock().unwrap(), ["b"]);
        }
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
    mode.store(1, Ordering::SeqCst);
    let probe = send().await.unwrap().unwrap();
    assert_eq!(probe.status(), 200);
    // Upstream JSON and EOF are already valid, but the final body is unpolled.
    let unavailable = send().await.unwrap().unwrap();
    assert_eq!(unavailable.status(), 503);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
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
