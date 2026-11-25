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
                        assert_eq!(body, wire);
                        let report = report.clone();
                        async { Json(report) }
                    },
                ),
            ),
        )
        .into_future(),
    );
    // Without configured ffmpeg, Gemini/Grok reject P4 before upstream dispatch.
    let no_ffmpeg = gateway
        .gateway
        .router()
        .oneshot(
            Request::post("/v1/images/generations")
                .header(
                    "authorization",
                    "Bearer synthetic-gateway-client-token-0001",
                )
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"model":model,"prompt":"pattern","output_format":"jpeg"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(no_ffmpeg.status(), StatusCode::SERVICE_UNAVAILABLE);

    // With configured ffmpeg, requesting output_format="jpeg" matches the native JPEG output with zero FFmpeg invocation.
    gateway.set_ffmpeg(std::env::current_exe().unwrap());
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
                .body(Body::from(
                    json!({"model":model,"prompt":"pattern","output_format":"jpeg","quality":"high"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 4096)
        .await
        .unwrap();
    let mut delivered: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(
        delivered
            .as_object_mut()
            .unwrap()
            .remove("created")
            .unwrap()
            .is_u64()
    );
    assert_eq!(
        delivered,
        json!({"output_format":"jpeg","data":[{"b64_json":"AQID"}]})
    );
    server.abort();
    let _ = server.await;
}

#[tokio::test]
async fn gemini_image_uses_trusted_interactions_path_and_preserves_jpeg() {
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
        "/v1/interactions",
        ("x-goog-api-key", "synthetic-google-key"),
        json!({"model":model,"store":false,"input":[{"type":"text","text":"pattern"}],"response_format":{"type":"image"}}),
        json!({"status":"completed","steps":[{"type":"model_output","content":[
            {"type":"image","mime_type":"image/jpeg","data":"AQID"}
        ]}]}),
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
    let send = |body: &'static str| {
        router.clone().oneshot(
            Request::post("/v1/images/generations")
                .header(
                    "authorization",
                    "Bearer synthetic-gateway-client-token-0001",
                )
                .header("content-type", "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
    };
    let ok_body = r#"{"model":"qwen-image-3.0-pro","prompt":"a blue circle","size":"512x512","output_format":"png","quality":"high"}"#;
    assert_eq!(
        send(
            r#"{"model":"qwen-image-3.0-pro","prompt":"a blue circle","background":"transparent"}"#
        )
        .await
        .unwrap()
        .status(),
        StatusCode::BAD_REQUEST
    );
    assert!(calls.lock().unwrap().is_empty());
    let expected = json!({"data":[{"b64_json":"AQID"}],"output_format":"png","size":"512x512"});
    let before = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let first = send(ok_body).await.unwrap();
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
        send(ok_body).await.unwrap().status(),
        StatusCode::TOO_MANY_REQUESTS
    );
    assert_eq!(*calls.lock().unwrap(), ["plan", "plan"]);
    let next = send(ok_body).await.unwrap();
    assert_eq!(next.status(), StatusCode::OK);
    let next = axum::body::to_bytes(next.into_body(), 4096).await.unwrap();
    let mut next_delivered: serde_json::Value = serde_json::from_slice(&next).unwrap();
    assert!(
        next_delivered
            .as_object_mut()
            .unwrap()
            .remove("created")
            .unwrap()
            .is_u64()
    );
    assert_eq!(next_delivered, expected);
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

#[tokio::test]
async fn image_edit_enforces_multipart_limits_and_native_edit_paths() {
    use axum::{
        Json, Router,
        body::Body,
        http::{HeaderMap, Request, StatusCode},
        routing::post,
    };
    use std::sync::{Arc, Mutex};
    use tower::ServiceExt;

    let observed = Arc::new(Mutex::new(Vec::new()));
    let seen = observed.clone();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let upstream = Router::new().route(
        "/v1/images/edits",
        post(move |headers: HeaderMap, body: axum::body::Bytes| {
            let seen = seen.clone();
            async move {
                assert_eq!(headers["authorization"], "Bearer synthetic-oauth-access");
                let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
                seen.lock().unwrap().push(value);
                Json(json!({
                    "data": [{"b64_json": "AQID", "mime_type": "image/jpeg"}]
                }))
            }
        }),
    );
    let server = tokio::spawn(async move { axum::serve(listener, upstream).await.unwrap() });
    let model = "grok-imagine-image-2.0";
    let mut fixture = Fixture::with_oauth(
        service(),
        json!({"models":{(model):{"account":"primary"}}}),
        "grok",
        "primary",
    )
    .unwrap();
    fixture.loopback_authority(&base);
    fixture.set_ffmpeg(std::env::current_exe().unwrap());
    let router = fixture.gateway.router();

    let multipart_body = concat!(
        "--e1\r\nContent-Disposition: form-data; name=\"model\"\r\n\r\n",
        "grok-imagine-image-2.0\r\n",
        "--e1\r\nContent-Disposition: form-data; name=\"prompt\"\r\n\r\n",
        "paint blue\r\n",
        "--e1\r\nContent-Disposition: form-data; name=\"output_format\"\r\n\r\n",
        "jpeg\r\n",
        "--e1\r\nContent-Disposition: form-data; name=\"image\"; filename=\"src.png\"\r\n",
        "Content-Type: image/png\r\n\r\n",
        "pngdata\r\n",
        "--e1--\r\n"
    );

    // 415 on non-multipart Content-Type
    let non_multipart = router
        .clone()
        .oneshot(
            Request::post("/v1/images/edits")
                .header(
                    "authorization",
                    "Bearer synthetic-gateway-client-token-0001",
                )
                .header("content-type", "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(non_multipart.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);

    // 404 on unknown model
    let unknown_model = multipart_body.replace("grok-imagine-image-2.0", "unknown-image");
    let not_found = router
        .clone()
        .oneshot(
            Request::post("/v1/images/edits")
                .header(
                    "authorization",
                    "Bearer synthetic-gateway-client-token-0001",
                )
                .header("content-type", "multipart/form-data; boundary=e1")
                .body(Body::from(unknown_model))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(not_found.status(), StatusCode::NOT_FOUND);

    // 200 on standard Grok /v1/images/edits with matching output_format=jpeg
    let ok = router
        .clone()
        .oneshot(
            Request::post("/v1/images/edits")
                .header(
                    "authorization",
                    "Bearer synthetic-gateway-client-token-0001",
                )
                .header("content-type", "multipart/form-data; boundary=e1")
                .body(Body::from(multipart_body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(ok.status(), StatusCode::OK);
    let body = axum::body::to_bytes(ok.into_body(), 4096).await.unwrap();
    let mut delivered: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(
        delivered
            .as_object_mut()
            .unwrap()
            .remove("created")
            .unwrap()
            .is_u64()
    );
    assert_eq!(
        delivered,
        json!({"output_format":"jpeg","data":[{"b64_json":"AQID"}]})
    );
    {
        let captured = observed.lock().unwrap();
        assert_eq!(captured.len(), 1);
        assert_eq!(
            captured[0]["image"],
            json!({"type": "image_url", "url": "data:image/png;base64,cG5nZGF0YQ=="})
        );
    }

    // 413 when request exceeds image_edit_request_bytes
    let too_large = router
        .oneshot(
            Request::post("/v1/images/edits")
                .header(
                    "authorization",
                    "Bearer synthetic-gateway-client-token-0001",
                )
                .header("content-type", "multipart/form-data; boundary=e1")
                .header(
                    "content-length",
                    (crate::protocol::openai::images::MAX_EDIT_UPLOAD_BYTES + 1).to_string(),
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(too_large.status(), StatusCode::PAYLOAD_TOO_LARGE);

    server.abort();
    let _ = server.await;
}

#[tokio::test]
async fn media_sse_streams_incrementally_holds_terminal_behind_eof_and_aborts_honestly() {
    use axum::{
        Router,
        body::{Body, Bytes},
        http::{Request, StatusCode},
        routing::post,
    };
    use futures_util::StreamExt;
    use tokio::sync::mpsc;
    use tower::ServiceExt;

    type ChunkTx = mpsc::Sender<Result<Bytes, std::io::Error>>;

    let (sp_open_tx, mut sp_rx) = mpsc::channel::<ChunkTx>(2);
    let (im_open_tx, mut im_rx) = mpsc::channel::<ChunkTx>(2);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let sp_open = sp_open_tx.clone();
    let im_open = im_open_tx.clone();
    let upstream = Router::new()
        .route(
            "/api/v1/services/audio/tts/SpeechSynthesizer",
            post(move || {
                let sp_open = sp_open.clone();
                async move {
                    let (tx, mut rx) = mpsc::channel::<Result<Bytes, std::io::Error>>(8);
                    sp_open.send(tx).await.unwrap();
                    (
                        [("content-type", "text/event-stream")],
                        Body::from_stream(futures_util::stream::poll_fn(move |cx| {
                            rx.poll_recv(cx)
                        })),
                    )
                }
            }),
        )
        .route(
            "/v1/images/generations",
            post(move || {
                let im_open = im_open.clone();
                async move {
                    let (tx, mut rx) = mpsc::channel::<Result<Bytes, std::io::Error>>(8);
                    im_open.send(tx).await.unwrap();
                    (
                        [("content-type", "text/event-stream")],
                        Body::from_stream(futures_util::stream::poll_fn(move |cx| {
                            rx.poll_recv(cx)
                        })),
                    )
                }
            }),
        );
    let server = tokio::spawn(async move { axum::serve(listener, upstream).await.unwrap() });
    let mut fixture = Fixture::new(
        service(),
        json!({"models":{
            "qwen-audio-3.0-tts-plus":{"account":"ali"},
            "gpt-image-2.6-flare":{"account":"oai"}
        }}),
        &[
            ("aliyun-tokenplan-cn", "ali", "synthetic-ali"),
            ("openai", "oai", "synthetic-oai"),
        ],
    )
    .unwrap();
    fixture.loopback_authority(&origin);
    let router = fixture.gateway.router();

    // 1. Aliyun Speech SSE emits delta before upstream EOF, holds speech.audio.done until EOF.
    let speech_task = tokio::spawn(
        router.clone().oneshot(
            Request::post("/v1/audio/speech")
                .header(
                    "authorization",
                    "Bearer synthetic-gateway-client-token-0001",
                )
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"model":"qwen-audio-3.0-tts-plus","input":"Hi","voice":"longanlingxin","stream_format":"sse"}"#,
                ))
                .unwrap(),
        ),
    );
    let tx = sp_rx.recv().await.unwrap();
    tx.send(Ok(Bytes::from(format!(
        "data: {}\n\ndata: {}\n\n",
        json!({"request_id":"r1","output":{"finish_reason":"null","type":"sentence-begin","sentence":{"index":0,"words":[]},"audio":{"data":""}}}),
        json!({"request_id":"r1","output":{"finish_reason":"null","type":"sentence-synthesis","original_text":"Hi","sentence":{"index":0,"words":[]},"audio":{"data":"AQID"}}}),
    ))))
    .await
    .unwrap();
    let speech_resp = speech_task.await.unwrap().unwrap();
    assert_eq!(speech_resp.status(), StatusCode::OK);
    assert_eq!(speech_resp.headers()["content-type"], "text/event-stream");
    let mut body = speech_resp.into_body().into_data_stream();
    let first = body.next().await.unwrap().unwrap();
    let first_text = std::str::from_utf8(&first).unwrap();
    assert!(first_text.contains("speech.audio.delta"));
    assert!(first_text.contains("\"audio\":\"AQID\""));

    // Send sentence-end and stop BEFORE closing upstream stream: terminal must wait for EOF.
    tx.send(Ok(Bytes::from(format!(
        "data: {}\n\ndata: {}\n\n",
        json!({"request_id":"r1","output":{"finish_reason":"null","type":"sentence-end","original_text":"Hi","sentence":{"index":0,"words":[]},"audio":{"data":""}}}),
        json!({"request_id":"r1","output":{"finish_reason":"stop","audio":{"data":""}},"usage":{"characters":2}}),
    ))))
    .await
    .unwrap();
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(50), body.next())
            .await
            .is_err(),
        "terminal event must not be emitted before upstream EOF"
    );
    drop(tx);
    let last = body.next().await.unwrap().unwrap();
    let last_text = std::str::from_utf8(&last).unwrap();
    assert!(last_text.contains("speech.audio.done"));
    assert!(!last_text.contains("usage"));
    assert!(body.next().await.is_none());

    // 2. OpenAI Image Preview SSE emits partial_image before EOF, then aborts honestly on mid-stream truncation without fabricating completed.
    let image_task = tokio::spawn(
        router.oneshot(
            Request::post("/v1/images/generations")
                .header(
                    "authorization",
                    "Bearer synthetic-gateway-client-token-0001",
                )
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"model":"gpt-image-2.6-flare","prompt":"cat","stream":true,"partial_images":1}"#,
                ))
                .unwrap(),
        ),
    );
    let tx = im_rx.recv().await.unwrap();
    tx.send(Ok(Bytes::from(format!(
        "event: image_generation.partial_image\ndata: {}\n\n",
        json!({"type":"image_generation.partial_image","partial_image_index":0,"b64_json":"AQID","created_at":10,"output_format":"png","size":"auto","quality":"auto","background":"auto"})
    ))))
    .await
    .unwrap();
    let image_resp = image_task.await.unwrap().unwrap();
    assert_eq!(image_resp.status(), StatusCode::OK);
    let mut body = image_resp.into_body().into_data_stream();
    let first = body.next().await.unwrap().unwrap();
    assert!(
        std::str::from_utf8(&first)
            .unwrap()
            .contains("image_generation.partial_image")
    );
    // Truncate upstream without emitting completed: downstream sees stream error, never completed.
    drop(tx);
    let err = body.next().await.unwrap();
    assert!(err.is_err());

    server.abort();
    let _ = server.await;
}

#[tokio::test]
async fn non_native_media_emulates_sse_omits_advisory_controls_and_guards_ffmpeg_p4() {
    use axum::{
        Json, Router,
        body::Body,
        http::{Request, StatusCode},
        routing::post,
    };
    use futures_util::StreamExt;
    use tower::ServiceExt;

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let upstream = Router::new()
        .route(
            "/v1/chat/completions",
            post(|Json(body): Json<serde_json::Value>| async move {
                assert_eq!(body["stream"], false);
                assert!(body.get("speed").is_none());
                if body["model"] == "mimo-v2.5-tts" {
                    Json(json!({
                        "id": "sp1", "object": "chat.completion", "created": 1, "model": "mimo-v2.5-tts",
                        "choices": [{"index": 0, "finish_reason": "stop", "message": {"role": "assistant", "content": null, "audio": {"id": "a1", "data": "AQID"}}}],
                        "usage": null
                    }))
                } else {
                    assert!(body.get("asr_options").is_none());
                    Json(json!({
                        "id": "asr1",
                        "choices": [{"index": 0, "finish_reason": "stop", "message": {"role": "assistant", "content": "nihao", "audio": null}}],
                        "usage": {"seconds": 3}
                    }))
                }
            }),
        )
        .route(
            "/api/v1/services/aigc/multimodal-generation/generation",
            post(|Json(body): Json<serde_json::Value>| async move {
                assert!(body.get("quality").is_none());
                assert!(body.get("stream").is_none());
                Json(json!({
                    "output": {"choices": [{"finish_reason": "stop", "message": {"role": "assistant", "content": [{"image": "data:image/png;base64,AQID"}]}}]},
                    "usage": {"output_width": 512, "output_height": 512, "output_image_count": 1, "input_image_count": 0, "input_image_type": "qima_input_1k", "output_image_type": "qima_output_1k"}
                }))
            }),
        );
    let server = tokio::spawn(async move { axum::serve(listener, upstream).await.unwrap() });
    let mut fixture = Fixture::new(
        service(),
        json!({"models":{
            "qwen-audio-3.0-tts-plus":{"account":"ali"},
            "qwen-audio-3.0-asr-flash":{"binding":"tokenplan-qwen-asr","account":"ali"},
            "mimo-v2.5-tts":{"account":"mi"},
            "mimo-v2.5-asr":{"account":"mi"},
            "qwen-image-3.0-pro":{"candidates":[{"binding":"aliyun-tokenplan-cn-qwen-image-3.0-pro-images","account":"ali"}]}
        }}),
        &[
            ("aliyun-tokenplan-cn", "ali", "synthetic-ali"),
            ("xiaomi", "mi", "synthetic-mi"),
        ],
    )
    .unwrap();
    fixture.loopback_authority(&origin);
    let router = fixture.gateway.router();

    // 1. P4 checks: missing ffmpeg returns 503 when transcoding is needed; invalid guarantees still 400 first.
    let bad_voice = router
        .clone()
        .oneshot(
            Request::post("/v1/audio/speech")
                .header("authorization", "Bearer synthetic-gateway-client-token-0001")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"model":"qwen-audio-3.0-tts-plus","input":"Hi","voice":"alloy","response_format":"opus"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(bad_voice.status(), StatusCode::BAD_REQUEST);

    let need_speech_ffmpeg = router
        .clone()
        .oneshot(
            Request::post("/v1/audio/speech")
                .header("authorization", "Bearer synthetic-gateway-client-token-0001")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"model":"qwen-audio-3.0-tts-plus","input":"Hi","voice":"longanlingxin","response_format":"opus"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(need_speech_ffmpeg.status(), StatusCode::SERVICE_UNAVAILABLE);

    let need_image_ffmpeg = router
        .clone()
        .oneshot(
            Request::post("/v1/images/generations")
                .header(
                    "authorization",
                    "Bearer synthetic-gateway-client-token-0001",
                )
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"model":"qwen-image-3.0-pro","prompt":"cat","output_format":"webp"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(need_image_ffmpeg.status(), StatusCode::SERVICE_UNAVAILABLE);

    let flac_asr = "--b1\r\nContent-Disposition: form-data; name=\"model\"\r\n\r\nqwen-audio-3.0-asr-flash\r\n--b1\r\nContent-Disposition: form-data; name=\"file\"; filename=\"a.flac\"\r\nContent-Type: audio/flac\r\n\r\nfLaC\r\n--b1--\r\n";
    let need_asr_ffmpeg = router
        .clone()
        .oneshot(
            Request::post("/v1/audio/transcriptions")
                .header(
                    "authorization",
                    "Bearer synthetic-gateway-client-token-0001",
                )
                .header("content-type", "multipart/form-data; boundary=b1")
                .body(Body::from(flac_asr))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(need_asr_ffmpeg.status(), StatusCode::SERVICE_UNAVAILABLE);

    // 2. Emulated Speech SSE on Xiaomi (omitting speed + no fabricated usage)
    let sp_sse = router
        .clone()
        .oneshot(
            Request::post("/v1/audio/speech")
                .header("authorization", "Bearer synthetic-gateway-client-token-0001")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"model":"mimo-v2.5-tts","input":"Hi","voice":"Chloe","speed":1.5,"stream_format":"sse"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(sp_sse.status(), StatusCode::OK);
    let sp_bytes = axum::body::to_bytes(sp_sse.into_body(), 8192)
        .await
        .unwrap();
    let sp_text = std::str::from_utf8(&sp_bytes).unwrap();
    assert!(sp_text.contains("speech.audio.delta"));
    assert!(sp_text.contains("speech.audio.done"));
    assert!(!sp_text.contains("\"usage\""));

    // 3. Emulated ASR SSE on Xiaomi (WAV passthrough, advisory prompt omitted, duration usage omitted from SSE)
    let wav_sse = "--b2\r\nContent-Disposition: form-data; name=\"model\"\r\n\r\nmimo-v2.5-asr\r\n--b2\r\nContent-Disposition: form-data; name=\"prompt\"\r\n\r\nignore me\r\n--b2\r\nContent-Disposition: form-data; name=\"stream\"\r\n\r\ntrue\r\n--b2\r\nContent-Disposition: form-data; name=\"file\"; filename=\"a.wav\"\r\nContent-Type: audio/wav\r\n\r\nRIFF\r\n--b2--\r\n";
    let asr_sse = router
        .clone()
        .oneshot(
            Request::post("/v1/audio/transcriptions")
                .header(
                    "authorization",
                    "Bearer synthetic-gateway-client-token-0001",
                )
                .header("content-type", "multipart/form-data; boundary=b2")
                .body(Body::from(wav_sse))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(asr_sse.status(), StatusCode::OK);
    let asr_bytes = axum::body::to_bytes(asr_sse.into_body(), 8192)
        .await
        .unwrap();
    let asr_text = std::str::from_utf8(&asr_bytes).unwrap();
    assert!(asr_text.contains("transcript.text.delta") && asr_text.contains("\"delta\":\"nihao\""));
    assert!(asr_text.contains("transcript.text.done") && !asr_text.contains("\"usage\""));

    // 4. Emulated completed-only Image SSE on Qwen PNG (quality/partial_images omitted, no fabricated usage)
    let img_sse = router
        .oneshot(
            Request::post("/v1/images/generations")
                .header("authorization", "Bearer synthetic-gateway-client-token-0001")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"model":"qwen-image-3.0-pro","prompt":"cat","stream":true,"partial_images":2,"quality":"high"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(img_sse.status(), StatusCode::OK);
    let mut frames = img_sse.into_body().into_data_stream();
    let only = frames.next().await.unwrap().unwrap();
    let img_text = std::str::from_utf8(&only).unwrap();
    assert!(img_text.contains("image_generation.completed"));
    assert!(img_text.contains("\"size\":\"512x512\""));
    assert!(!img_text.contains("\"usage\""));
    assert!(frames.next().await.is_none());

    server.abort();
    let _ = server.await;
}
