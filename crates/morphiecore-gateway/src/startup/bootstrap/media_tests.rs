//! Explicit standalone task bindings and once-only exchanges.
use super::{
    test_support::{Fixture, selected_models, service},
    *,
};
use serde_json::json;

async fn assert_native_jpeg(
    mut gateway: Fixture,
    model: &str,
    path: &str,
    auth: (&'static str, &'static str),
    wire: serde_json::Value,
    report: serde_json::Value,
) {
    use axum::{
        Json, Router,
        body::Body,
        http::{HeaderMap, Request, StatusCode},
        routing::post,
    };
    use tower::ServiceExt;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    gateway.loopback_authority(&format!("http://{}", listener.local_addr().unwrap()));
    let server = tokio::spawn(
        axum::serve(
            listener,
            Router::new().route(
                path,
                post(
                    move |headers: HeaderMap, Json(body): Json<serde_json::Value>| {
                        assert_eq!(headers[auth.0], auth.1);
                        assert!(!headers.contains_key("x-morphiecore-image-profile"));
                        assert_eq!(body, wire);
                        let report = report.clone();
                        async { Json(report) }
                    },
                ),
            ),
        )
        .into_future(),
    );
    let response = gateway
        .gateway
        .router()
        .oneshot(
            Request::post("/v1/images/generations")
                .header(
                    "authorization",
                    "Bearer synthetic-gateway-client-token-0001",
                )
                .header("content-type", "application/json")
                .header("x-morphiecore-image-profile", "native-static-v1")
                .body(Body::from(
                    json!({"model":model,"prompt":"pattern"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 4096)
        .await
        .unwrap();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&body).unwrap(),
        json!({"output_format":"jpeg","data":[{"b64_json":"AQID"}]})
    );
    server.abort();
    let _ = server.await;
}

#[tokio::test]
async fn gemini_image_uses_trusted_generate_content_path_and_preserves_jpeg() {
    let model = "gemini-nano-banana-2.1";
    let fixture = Fixture::new(
        service(),
        json!({"models":{(model):{"account":"primary"}}}),
        &[("google", "primary", "synthetic-google-key")],
    )
    .unwrap();
    assert_native_jpeg(
        fixture,
        model,
        "/v1beta/models/gemini-nano-banana-2.1:generateContent",
        ("x-goog-api-key", "synthetic-google-key"),
        json!({"contents":[{"role":"user","parts":[{"text":"pattern"}]}],
            "generationConfig":{"responseModalities":["TEXT","IMAGE"]}}),
        json!({"candidates":[{"finishReason":"STOP","content":{"role":"model","parts":[
            {"inlineData":{"mimeType":"image/jpeg","data":"AQID"}}
        ]}}]}),
    )
    .await;
}

#[tokio::test]
async fn grok_image_uses_managed_oauth_and_delivers_only_the_native_artifact() {
    let model = "grok-imagine-image-2.0";
    let configuration = json!({"models":{(model):{"account":"primary"}}});
    assert!(
        Fixture::new(
            service(),
            configuration.clone(),
            &[("openai", "primary", "synthetic-unrelated-key")]
        )
        .is_err()
    );
    let fixture = Fixture::with_oauth(service(), configuration, "grok", "primary").unwrap();
    assert_native_jpeg(
        fixture,
        model,
        "/v1/images/generations",
        ("authorization", "Bearer synthetic-oauth-access"),
        json!({"model":model,"prompt":"pattern","n":1,"response_format":"b64_json"}),
        json!({"data":[{"b64_json":"AQID","mime_type":"image/jpeg"}],
            "usage":{"cost_in_usd_ticks":400000000}}),
    )
    .await;
}

#[tokio::test]
async fn native_image_routes_new_requests_around_cooldown_without_replaying() {
    use axum::{
        Json, Router,
        body::Body,
        http::{HeaderMap, Request, StatusCode},
        response::IntoResponse,
        routing::post,
    };
    use std::sync::{Arc, Mutex};
    use tower::ServiceExt;

    let calls = Arc::new(Mutex::new(Vec::new()));
    let observed = calls.clone();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let result = || {
        Json(json!({
        "output": {"choices":[{"finish_reason":"stop","message":{"role":"assistant",
            "content":[{"image":"data:image/png;base64,AQID"}]}}]},
        "usage":{"output_width":512,"output_height":512,"output_image_count":1,
            "input_image_count":0,"input_image_type":"qima_input_1k","output_image_type":"qima_output_1k"}
    })).into_response()
    };
    let suffix = "/services/aigc/multimodal-generation/generation";
    let upstream = Router::new()
        .route(&format!("/plan{suffix}"), post(move |headers: HeaderMap, Json(body): Json<serde_json::Value>| {
            let mut calls = observed.lock().unwrap();
            calls.push("plan");
            assert_eq!(headers["authorization"], "Bearer synthetic-plan-key");
            assert!(!headers.contains_key("x-morphiecore-image-profile"));
            assert_eq!(body, json!({
                "model":"qwen-image-3.0-pro",
                "input":{"messages":[{"role":"user","content":[{"text":"a blue circle"}]}]},
                "parameters":{"size":"512*512","n":1,"response_format":"b64_json"}
            }));
            let failure = calls.len() != 1;
            async move {
                if failure {
                    (StatusCode::TOO_MANY_REQUESTS, [("retry-after", "60")]).into_response()
                } else {
                    result()
                }
            }
        }))
        .route(&format!("/metered{suffix}"), post({
            let calls = calls.clone();
            move |headers: HeaderMap, Json(body): Json<serde_json::Value>| {
                calls.lock().unwrap().push("metered");
                assert_eq!(headers["authorization"], "Bearer synthetic-metered-key");
                assert_eq!(body["model"], "qwen-image-3.0-pro");
                async move { result() }
            }
        }));
    let server = tokio::spawn(async move { axum::serve(listener, upstream).await.unwrap() });
    let model = "qwen-image-3.0-pro";
    let catalog_candidates = [
        json!({"binding":"aliyun-tokenplan-cn-qwen-image-3.0-pro-images","account":"plan"}),
        json!({"binding":"aliyun-dashscope-cn-qwen-image-3.0-pro-images","account":"metered"}),
    ];
    let accounts = json!({"version":1,"providers":{
        "aliyun-tokenplan-cn":{"accounts":[{"name":"plan","enabled":true,"credential":{"alias":"plan"},"base_url":format!("{base}/plan")}]},
        "aliyun-dashscope-cn":{"accounts":[{"name":"metered","enabled":true,"credential":{"alias":"metered"},"base_url":format!("{base}/metered")}]}
    }});
    let keys = [
        ("aliyun-tokenplan-cn", "plan", "synthetic-plan-key"),
        ("aliyun-dashscope-cn", "metered", "synthetic-metered-key"),
    ];
    let configured = |candidates| json!({"models":{(model):{"candidates":candidates}}});
    let mut reversed = catalog_candidates.clone();
    reversed.reverse();
    assert!(
        Fixture::with_accounts(service(), configured(reversed), &keys, accounts.clone()).is_err()
    );
    let gateway =
        Fixture::with_accounts(service(), configured(catalog_candidates), &keys, accounts).unwrap();
    let router = gateway.gateway.router();
    let send = |profile: Option<&'static str>| {
        let mut request = Request::builder()
            .method("POST")
            .uri("/v1/images/generations")
            .header(
                "authorization",
                "Bearer synthetic-gateway-client-token-0001",
            )
            .header("content-type", "application/json");
        if let Some(profile) = profile {
            request = request.header("x-morphiecore-image-profile", profile);
        }
        router.clone().oneshot(request.body(Body::from(
            r#"{"model":"qwen-image-3.0-pro","prompt":"a blue circle","size":"512x512","output_format":"png"}"#,
        )).unwrap())
    };
    assert_eq!(
        send(Some("native-static-v2")).await.unwrap().status(),
        StatusCode::BAD_REQUEST
    );
    assert!(calls.lock().unwrap().is_empty());
    let profile = Some("native-static-v1");
    let expected = json!({"data":[{"b64_json":"AQID"}],"output_format":"png","size":"512x512"});
    let before = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let first = send(None).await.unwrap();
    assert_eq!(first.status(), StatusCode::OK);
    let first = axum::body::to_bytes(first.into_body(), 4096).await.unwrap();
    let mut delivered: serde_json::Value = serde_json::from_slice(&first).unwrap();
    let created = delivered
        .as_object_mut()
        .unwrap()
        .remove("created")
        .unwrap()
        .as_u64()
        .unwrap();
    let after = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    assert!((before..=after).contains(&created));
    assert_eq!(delivered, expected);
    assert_eq!(*calls.lock().unwrap(), ["plan"]);
    assert_eq!(
        send(profile).await.unwrap().status(),
        StatusCode::TOO_MANY_REQUESTS
    );
    assert_eq!(*calls.lock().unwrap(), ["plan", "plan"]);
    let next = send(profile).await.unwrap();
    assert_eq!(next.status(), StatusCode::OK);
    let next = axum::body::to_bytes(next.into_body(), 4096).await.unwrap();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&next).unwrap(),
        expected
    );
    assert_eq!(*calls.lock().unwrap(), ["plan", "plan", "metered"]);
    server.abort();
    let _ = server.await;
}

#[tokio::test]
async fn configured_task_sends_once_to_its_named_account_despite_generation_recovery() {
    use axum::{Json, Router, http::HeaderMap, routing::post};
    use std::sync::{Arc, Mutex};
    use tower::ServiceExt;
    let calls = Arc::new(Mutex::new(Vec::new()));
    let observed = calls.clone();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let root = format!("http://{}/custom/v1", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new().route(
                "/custom/v1/embeddings",
                post(
                    move |headers: HeaderMap, Json(body): Json<serde_json::Value>| {
                        let calls = observed.clone();
                        async move {
                            calls.lock().unwrap().push((
                                headers["authorization"].to_str().unwrap().to_owned(),
                                body,
                            ));
                            axum::http::StatusCode::SERVICE_UNAVAILABLE
                        }
                    },
                ),
            ),
        )
        .await
        .unwrap();
    });
    let boot = Fixture::with_accounts(
        service(),
        json!({"models":{"text-embedding-3-small":{"account":"backup"}}}),
        &[
            ("openai", "primary", "synthetic-primary"),
            ("openai", "backup", "synthetic-backup"),
        ],
        json!({"version":1,"providers":{"openai":{
            "recovery":{"recovery_timeout_ms":5000},
            "accounts":(["primary","backup"].map(|name| json!({
                "name":name,"enabled":true,"credential":{"alias":name},"base_url":root
            })))
        }}}),
    )
    .unwrap();
    let response = boot
        .gateway
        .router()
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/v1/embeddings")
                .header(
                    "authorization",
                    "Bearer synthetic-gateway-client-token-0001",
                )
                .header("content-type", "application/json")
                .body(axum::body::Body::from(
                    r#"{"model":"text-embedding-3-small","input":"synthetic"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(response.status().is_server_error());
    axum::body::to_bytes(response.into_body(), 65536)
        .await
        .unwrap();
    {
        let calls = calls.lock().unwrap();
        assert_eq!(
            calls.len(),
            1,
            "standalone tasks do not inherit Generation recovery"
        );
        assert_eq!(calls[0].0, "Bearer synthetic-backup");
        assert_eq!(calls[0].1["model"], "text-embedding-3-small");
    }
    server.abort();
    let _ = server.await;
}

#[tokio::test]
async fn abandoned_embedding_releases_upstream_and_its_permit_before_and_during_handoff() {
    use axum::{
        Router,
        body::{Body, Bytes},
        http::Request,
        routing::post,
    };
    use futures_util::StreamExt;
    use std::{
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
        time::Duration,
    };
    use tokio::sync::mpsc;
    use tower::ServiceExt;

    let calls = Arc::new(AtomicUsize::new(0));
    let observed = calls.clone();
    let (opened, mut upstream_bodies) = mpsc::channel(2);
    let upstream = Router::new().route(
        "/v1/embeddings",
        post(move || {
            let opened = opened.clone();
            observed.fetch_add(1, Ordering::Relaxed);
            async move {
                let (tx, mut rx) = mpsc::channel::<Result<Bytes, std::io::Error>>(1);
                let body = futures_util::stream::once(async move {
                    opened.send(tx).await.unwrap();
                    Ok(Bytes::from_static(
                        br#"{"object":"list","model":"text-embedding-3-small",
                       "data":[{"object":"embedding","index":0,"embedding":[0.5,1]}],
                       "usage":{"prompt_tokens":1,"total_tokens":1}}"#,
                    ))
                })
                .chain(futures_util::stream::poll_fn(move |cx| rx.poll_recv(cx)));
                (
                    [("content-type", "application/json")],
                    Body::from_stream(body),
                )
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let root = format!("http://{}/v1", listener.local_addr().unwrap());
    let mut settings = service();
    settings["resources"] = json!({"concurrency":1});
    let mut boot = Fixture::with_accounts(
        settings,
        json!({"models":{"text-embedding-3-small":{"account":"fixture"}}}),
        &[("openai", "fixture", "synthetic-embedding-key")],
        json!({"version":1,"providers":{"openai":{"accounts":[{
            "name":"fixture","enabled":true,"credential":{"alias":"fixture"},"base_url":root
        }]}}}),
    )
    .unwrap();
    let server = tokio::spawn(async move { axum::serve(listener, upstream).await.unwrap() });
    let spans = boot.record_spans();
    let router = boot.gateway.router();
    let permits = &boot.gateway.state.permits;
    let request = || {
        Request::post("/v1/embeddings")
            .header(
                "authorization",
                format!("Bearer {}", crate::tests::support::KEY),
            )
            .header("content-type", "application/json")
            .body(Body::from(
                r#"{"model":"text-embedding-3-small","input":"synthetic","dimensions":2}"#,
            ))
            .unwrap()
    };
    let checked = tokio::time::timeout(Duration::from_secs(5), async {
        // Keep the first upstream body open so the route cannot publish a result.
        let pending = tokio::spawn(router.clone().oneshot(request()));
        let upstream = upstream_bodies.recv().await.unwrap();
        assert!(!pending.is_finished());
        assert!(!upstream.is_closed());
        assert_eq!(permits.available_permits(), 0);
        assert!(
            spans
                .get_finished_spans()
                .unwrap()
                .iter()
                .all(|span| span.name != "gateway.upstream.http")
        );
        pending.abort();
        assert!(pending.await.unwrap_err().is_cancelled());
        upstream.closed().await;
        drop(permits.acquire().await.unwrap());
        assert_eq!(calls.load(Ordering::Relaxed), 1);

        // The next request can complete intake, but headers alone do not
        // acknowledge the final JSON frame or release its permit.
        let pending = tokio::spawn(router.oneshot(request()));
        drop(upstream_bodies.recv().await.unwrap());
        let response = pending.await.unwrap().unwrap();
        assert_eq!(response.status(), 200);
        assert_eq!(permits.available_permits(), 0);
        drop(response);
        drop(permits.acquire().await.unwrap());
        assert_eq!(calls.load(Ordering::Relaxed), 2);
    })
    .await;
    server.abort();
    let _ = server.await;
    checked.expect("standalone exchange did not release its upstream body and permit");
    let spans = spans.get_finished_spans().unwrap();
    let clients: Vec<_> = spans
        .iter()
        .filter(|span| span.name == "gateway.upstream.http")
        .collect();
    assert_eq!(clients.len(), 2);
    for outcome in ["not_drained", "eof"] {
        let client = clients
            .iter()
            .find(|span| {
                span.attributes.contains(&opentelemetry::KeyValue::new(
                    "morphiecore.io.outcome",
                    outcome,
                ))
            })
            .unwrap();
        assert!(spans.iter().any(|span| span.name == "gateway.attempt"
            && span.span_context.span_id() == client.parent_span_id
            && span.span_context.trace_id() == client.span_context.trace_id()));
    }
}

#[test]
fn embeddings_select_one_named_account_and_publish_only_the_selected_task() {
    for binding in catalog::embedding_bindings() {
        let provider = binding.provider();
        let boot = Fixture::new(
            service(),
            json!({"models":{binding.model:{"account":"primary"}}}),
            &[
                (provider.id.as_str(), "primary", "synthetic-embedding-key"),
                (provider.id.as_str(), "standby", "synthetic-other-key"),
            ],
        )
        .unwrap();
        assert!(boot.gateway.state.entries.is_empty());
        assert_eq!(boot.gateway.state.embeddings.len(), 1);
        let selected = &boot.gateway.state.embeddings[binding.model];
        assert_eq!(
            selected.secret.resolve().unwrap().0.expose(),
            "synthetic-embedding-key"
        );
        assert!(boot.gateway.state.models.models.contains_key(binding.model));
        let snapshot: serde_json::Value =
            serde_json::from_slice(&boot.gateway.state.capabilities.0).unwrap();
        assert_eq!(snapshot["data"].as_array().unwrap().len(), 1);
        assert_eq!(snapshot["data"][0]["model"], binding.model);
        assert_eq!(snapshot["data"][0]["protocol"], "embeddings");
        assert_eq!(
            snapshot["data"][0]["embedding"]["default_dimensions"],
            binding.default_dimensions
        );
        assert_eq!(
            snapshot["data"][0]["embedding"]["output_encodings"],
            json!(["float", "base64"])
        );
        assert!(!snapshot.to_string().contains(provider.origin.as_str()));
        assert!(!snapshot.to_string().contains("synthetic-embedding-key"));
        if let Some(text) = catalog::api_key_bindings().iter().find(|b| {
            b.provider().id == provider.id
                && !b
                    .public_model()
                    .contract
                    .native(crate::semantic::task::generation::Feature::AudioInput)
        }) {
            let generation = Fixture::new(
                service(),
                selected_models(&[text.model], &[provider.id.as_str()]),
                &[(provider.id.as_str(), "primary", "synthetic-embedding-key")],
            )
            .unwrap();
            assert!(generation.gateway.state.embeddings.is_empty());
            assert!(!generation.gateway.state.entries.is_empty());
        }
        for account in [None, Some("missing")] {
            let config = account.map_or_else(|| json!({}), |name| json!({"account":name}));
            assert!(
                Fixture::new(
                    service(),
                    json!({"models":{binding.model:config}}),
                    &[(provider.id.as_str(), "primary", "synthetic-embedding-key")]
                )
                .is_err()
            );
        }
    }
}

#[test]
fn task_accounts_share_target_composition_but_not_automatic_account_selection() {
    let model = "text-embedding-3-small";
    let keys = [
        ("openai", "primary", "synthetic-first"),
        ("openai", "backup", "synthetic-second"),
    ];
    let declarations = json!({"version":1,"providers":{"openai":{"accounts":[
        {"name":"primary","enabled":true,"credential":{"alias":"primary"}},
        {"name":"backup","enabled":true,"credential":{"alias":"backup"},"base_url":"https://relay.invalid/custom/v1"}
    ]}}});
    let boot = Fixture::with_accounts(
        service(),
        json!({"models":{model:{"account":"backup"}}}),
        &keys,
        declarations.clone(),
    )
    .unwrap();
    let selected = &boot.gateway.state.embeddings[model];
    assert_eq!(selected.provider.origin.as_str(), "https://relay.invalid");
    assert_eq!(
        selected.route.endpoint.target.path.as_str(),
        "/custom/v1/embeddings"
    );
    assert_eq!(
        selected.secret.resolve().unwrap().0.expose(),
        "synthetic-second"
    );
    let mut disabled = declarations;
    disabled["providers"]["openai"]["accounts"][1]["enabled"] = json!(false);
    assert!(matches!(
        Fixture::with_accounts(
            service(),
            json!({"models":{model:{"account":"backup"}}}),
            &keys,
            disabled
        ),
        Err(StartupError::Credentials)
    ));
}

#[test]
fn tokenplan_audio_stays_explicit_and_each_task_uses_its_fixed_account() {
    let keys = [
        ("aliyun-tokenplan-cn", "primary", "synthetic-first"),
        ("aliyun-tokenplan-cn", "backup", "synthetic-second"),
    ];
    let boot = Fixture::new(
        service(),
        json!({"models":{
            "qwen-audio-3.0-tts-plus":{"account":"primary"},
            "qwen-audio-3.0-asr-flash":{"binding":"tokenplan-qwen-asr","account":"backup"}
        }}),
        &keys,
    )
    .unwrap();
    assert!(boot.gateway.state.entries.is_empty());
    assert_eq!(boot.gateway.state.speech.len(), 1);
    assert_eq!(boot.gateway.state.transcriptions.len(), 1);
    assert_eq!(
        boot.gateway.state.speech["qwen-audio-3.0-tts-plus"]
            .secret
            .resolve()
            .unwrap()
            .0
            .expose(),
        "synthetic-first"
    );
    assert_eq!(
        boot.gateway.state.transcriptions["qwen-audio-3.0-asr-flash"]
            .secret
            .resolve()
            .unwrap()
            .0
            .expose(),
        "synthetic-second"
    );
    let text = catalog::api_key_bindings()
        .iter()
        .find(|b| b.provider().id.as_str() == "aliyun-tokenplan-cn")
        .unwrap();
    let generation = Fixture::new(
        service(),
        selected_models(&[text.model], &["aliyun-tokenplan-cn"]),
        &keys,
    )
    .unwrap();
    assert!(
        generation.gateway.state.speech.is_empty()
            && generation.gateway.state.transcriptions.is_empty()
    );
    for invalid in [
        json!({"binding":"dashscope-qwen-asr","account":"primary"}),
        json!({"account":"primary"}),
    ] {
        assert!(
            Fixture::new(
                service(),
                json!({"models":{"qwen-audio-3.0-asr-flash":invalid}}),
                &keys
            )
            .is_err()
        );
    }
}

#[test]
fn speech_and_images_do_not_activate_from_generation_accounts() {
    let keys = [
        ("aliyun-tokenplan-cn", "primary", "synthetic-first"),
        ("openai", "backup", "synthetic-second"),
    ];
    let boot = Fixture::new(
        service(),
        json!({"models":{
            "qwen-audio-3.0-tts-plus":{"account":"primary"},
            "gpt-4o-mini-tts":{"account":"backup"},
            "gpt-transcribe":{"account":"backup"},
            "gpt-image-2.6-flare":{"account":"backup"}
        }}),
        &keys,
    )
    .unwrap();
    assert!(boot.gateway.state.entries.is_empty());
    assert_eq!(boot.gateway.state.speech.len(), 2);
    assert_eq!(boot.gateway.state.transcriptions.len(), 1);
    assert_eq!(boot.gateway.state.images.len(), 1);
    assert_eq!(
        boot.gateway.state.speech["gpt-4o-mini-tts"]
            .secret
            .resolve()
            .unwrap()
            .0
            .expose(),
        "synthetic-second"
    );
    assert_eq!(
        boot.gateway.state.transcriptions["gpt-transcribe"]
            .secret
            .resolve()
            .unwrap()
            .0
            .expose(),
        "synthetic-second"
    );
    assert_eq!(
        boot.gateway.state.images["gpt-image-2.6-flare"][0]
            .secret
            .resolve()
            .unwrap()
            .0
            .expose(),
        "synthetic-second"
    );
    let view: serde_json::Value = serde_json::from_slice(&boot.gateway.state.models.list).unwrap();
    assert!(view["data"].as_array().unwrap().contains(&json!({
        "id":"qwen-audio-3.0-tts-plus","object":"model","created":1784592000,"owned_by":"Alibaba"
    })));
    assert!(view["data"].as_array().unwrap().contains(&json!({
        "id":"gpt-4o-mini-tts","object":"model","created":1742428800,"owned_by":"OpenAI"
    })));
    assert!(view["data"].as_array().unwrap().contains(&json!({
        "id":"gpt-transcribe","object":"model","created":1785196800,"owned_by":"OpenAI"
    })));
    let generation = Fixture::new(
        service(),
        selected_models(
            &["gpt-6-luna", "qwen4-max"],
            &["openai", "aliyun-tokenplan-cn"],
        ),
        &keys,
    )
    .unwrap();
    assert!(
        generation.gateway.state.images.is_empty()
            && generation.gateway.state.speech.is_empty()
            && generation.gateway.state.transcriptions.is_empty()
    );
    assert!(!generation.gateway.state.entries.is_empty());
}
