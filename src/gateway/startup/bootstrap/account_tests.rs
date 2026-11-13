//! Account admission through production construction, not a pool-shaped test adapter.
use super::test_support::{Fixture, service};
use axum::{
    Json, Router,
    http::{HeaderMap, StatusCode},
    routing::post,
};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
use tower::ServiceExt;

#[tokio::test(start_paused = true)]
async fn configured_body_budgets_apply_before_execution_and_release_capacity() {
    use axum::{body::Body, http::Request};
    for (exchange_ms, expected, elapsed_ms) in [
        (1, StatusCode::GATEWAY_TIMEOUT, 1),
        (30, StatusCode::REQUEST_TIMEOUT, 20),
    ] {
        let boot = Fixture::new(
            json!({"resources":{"concurrency":1,"request_bytes":1024,"body_timeout_ms":20},
            "delivery":{"exchange_timeout_ms":exchange_ms}}),
            json!({"models":{"deepseek-v4.1-flash-1017":{"candidates":[{"binding":"deepseek-chat"}]}}}),
            &[("deepseek", "primary", "synthetic-key")],
        )
        .unwrap();
        let request = |body| {
            Request::builder()
                .method("POST")
                .uri("/v1/chat/completions")
                .header(
                    "authorization",
                    "Bearer synthetic-gateway-client-token-0001",
                )
                .header("content-type", "application/json")
                .body(body)
                .unwrap()
        };
        let response = boot
            .gateway
            .router()
            .oneshot(request(Body::from("x".repeat(1025))))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
        let start = tokio::time::Instant::now();
        let response = boot
            .gateway
            .router()
            .oneshot(request(Body::from_stream(futures_util::stream::pending::<
                Result<bytes::Bytes, std::io::Error>,
            >())))
            .await
            .unwrap();
        assert_eq!(response.status(), expected);
        assert_eq!(
            start.elapsed(),
            std::time::Duration::from_millis(elapsed_ms)
        );
        assert_eq!(boot.gateway.state.permits.available_permits(), 1);
    }
}

#[test]
fn resource_budgets_and_task_shapes_remain_strict() {
    let models =
        json!({"models":{"deepseek-v4.1-flash-1017":{"candidates":[{"binding":"deepseek-chat"}]}}});
    for settings in [
        json!({"routing":null}),
        json!({"resources":{"request_bytes":null}}),
        json!({"resources":{"history_entries":0}}),
        json!({"resources":{"history_bytes":0}}),
        json!({"resources":{"history_ttl_ms":0}}),
        json!({"resources":{"request_bytes":67108865u64}}),
    ] {
        assert!(
            Fixture::new(
                settings,
                models.clone(),
                &[("deepseek", "primary", "synthetic-key")]
            )
            .is_err()
        );
    }
    let result = Fixture::new(
        service(),
        json!({"models":{"text-embedding-3-small":{"account":"primary","candidates":[]}}}),
        &[("openrouter", "primary", "synthetic-key")],
    );
    assert!(matches!(result, Err(super::StartupError::Detail(error)) if error.domain == "models"));
}

#[test]
fn disabled_candidates_still_validate_model_controls() {
    // Validation does not become optional when a Provider is disabled.
    for enabled in [false, true] {
        for controls in [
            json!({"allowed_effort":["typo"]}),
            json!({"maximum_output_tokens":0}),
            json!({"default_output_tokens":null}),
        ] {
            let mut candidate = controls;
            candidate["binding"] = json!("openrouter-haiku-chat");
            let result = Fixture::with_accounts(
                service(),
                json!({"models":{"claude-haiku-5-5":{"candidates":[candidate,
                    {"binding":"opencode-go-haiku-messages"}]}}}),
                &[
                    ("openrouter", "primary", "synthetic-one"),
                    ("opencode-go", "primary", "synthetic-two"),
                ],
                json!({"version":1,"providers":{
                    "openrouter":{"accounts":[{"name":"primary","enabled":enabled,"credential":{"alias":"primary"}}]},
                    "opencode-go":{"accounts":[{"name":"primary","enabled":true,"credential":{"alias":"primary"}}]}
                }}),
            );
            assert!(
                result.is_err(),
                "invalid controls admitted with enabled={enabled}"
            );
        }
    }
}

#[test]
fn account_failures_locate_records_without_echoing_names_or_aliases() {
    let account = json!({"name":"sensitive-account-marker","enabled":true,
        "credential":{"alias":"sensitive-alias-marker"}});
    for declarations in [json!([account]), json!([account, account])] {
        let error = Fixture::with_accounts(
            service(),
            json!({"models":{"deepseek-v4.1-flash-1017":{"candidates":[{"binding":"deepseek-chat"}]}}}),
            &[],
            json!({"version":1,"providers":{"deepseek":{"accounts":declarations}}}),
        )
        .err()
        .unwrap();
        let diagnostic = error.diagnostic();
        assert_eq!(diagnostic["domain"], "accounts");
        assert!(
            diagnostic["field"]
                .as_str()
                .unwrap()
                .starts_with("providers[0].accounts[")
        );
        for output in [error.to_string(), diagnostic.to_string()] {
            assert!(!output.contains("sensitive-account-marker"));
            assert!(!output.contains("sensitive-alias-marker"));
        }
    }
}

#[test]
fn task_explanation_reports_the_resolved_binding_without_target_locators() {
    for (label, provider, binding) in [
        ("mimo-v2.5-asr", "xiaomi", None),
        (
            "qwen-audio-3.0-asr-flash",
            "aliyun-tokenplan-cn",
            Some("tokenplan-qwen-asr"),
        ),
    ] {
        let mut model = json!({"account":"primary"});
        if let Some(binding) = binding {
            model["binding"] = json!(binding);
        }
        let fixture = Fixture::new(
            service(),
            json!({"models":{label:model}}),
            &[(provider, "primary", "synthetic-secret")],
        )
        .unwrap();
        let value = super::Bootstrap::explain(
            &fixture.directory().join("gateway.json"),
            Some(&fixture.directory().join("credentials")),
            None,
        )
        .unwrap();
        assert_eq!(
            value["selections"][0]["binding"],
            binding.unwrap_or("xiaomi-asr")
        );
        assert!(
            value["selections"][0]["endpoint_timeout_ms"]
                .as_u64()
                .unwrap()
                > 0
        );
        assert!(value["selections"][0]["request_bytes"].as_u64().unwrap() > 0);
        assert!(!value.to_string().contains("synthetic-secret"));
    }
}

#[tokio::test]
async fn public_label_preserves_upstream_identity_and_discovery() {
    const LABEL: &str = "deepseek-v4.1-flash-1017";
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let root = format!("http://{}/beta", listener.local_addr().unwrap());
    let upstream = tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new().route(
                "/beta/chat/completions",
                post(|Json(body): Json<Value>| async move {
                    assert_eq!(body["model"], "deepseek-flash");
                    let chunk = json!({
                        "id":"synthetic-chat", "object":"chat.completion.chunk",
                        "created":1, "model":"deepseek-flash",
                        "choices":[{"index":0,"delta":{"role":"assistant","content":"healthy"},
                            "finish_reason":"stop"}]
                    });
                    (
                        [("content-type", "text/event-stream")],
                        format!("data: {chunk}\n\ndata: [DONE]\n\n"),
                    )
                }),
            ),
        )
        .await
        .unwrap();
    });
    let mut boot = Fixture::with_accounts(
        service(),
        json!({"models":{LABEL:{"candidates":[{"binding":"deepseek-chat"}]}}}),
        &[("deepseek", "primary", "synthetic-key")],
        json!({"version":1,"providers":{"deepseek":{"accounts":[
            {"name":"primary","enabled":true,"credential":{"alias":"primary"},"base_url":root}
        ]}}}),
    )
    .unwrap();
    boot.force_upstream_stream();
    for path in ["/v1/models", "/v1/morphiecore/models"] {
        let request = axum::http::Request::builder()
            .uri(path)
            .header(
                "authorization",
                "Bearer synthetic-gateway-client-token-0001",
            )
            .body(axum::body::Body::empty())
            .unwrap();
        let response = boot.gateway.router().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(response.into_body(), 65536)
            .await
            .unwrap();
        let value: Value = serde_json::from_slice(&bytes).unwrap();
        let rows = value["data"].as_array().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["id"], LABEL);
    }
    let request = axum::http::Request::builder()
        .method("POST")
        .uri("/v1/responses")
        .header(
            "authorization",
            "Bearer synthetic-gateway-client-token-0001",
        )
        .header("content-type", "application/json")
        .body(axum::body::Body::from(
            json!({"model":LABEL,"input":"go","store":false}).to_string(),
        ))
        .unwrap();
    let response = boot.gateway.router().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), 65536)
        .await
        .unwrap();
    let value: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(value["model"], LABEL);
    assert_eq!(value["output"][0]["content"][0]["text"], "healthy");
    boot.gateway.shutdown();
    upstream.abort();
    let _ = upstream.await;
}

#[tokio::test]
async fn loader_binds_named_accounts_without_switching_after_dispatch() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let observed = calls.clone();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let root = format!("http://{}/custom/v1", listener.local_addr().unwrap());
    let upstream = tokio::spawn(async move {
        axum::serve(listener,Router::new().route("/custom/v1/responses",post(move |headers:HeaderMap,Json(body):Json<Value>| {
            let calls=observed.clone();
            async move {
                use axum::response::IntoResponse;
                assert_eq!(body["model"],"deepseek-flash");
                let first=headers["authorization"]=="Bearer synthetic-primary";
                calls.lock().unwrap().push(if first {"primary"}else{"backup"});
                if first {return (StatusCode::SERVICE_UNAVAILABLE, [("retry-after","60")]).into_response();}
                assert_eq!(headers["authorization"],"Bearer synthetic-backup");
                ([ ("content-type","text/event-stream") ],crate::test_responses_wire::response_stream(json!({
                    "object":"response","id":"synthetic-response","model":"deepseek-flash","created_at":1,
                    "status":"completed","output":[{"type":"message","id":"synthetic-message","role":"assistant","status":"completed",
                        "content":[{"type":"output_text","text":"healthy","annotations":[],"logprobs":[]}]}]
                }))).into_response()
            }
        }))).await.unwrap();
    });
    let accounts = json!({"version":1,"providers":{"deepseek":{"accounts":[
        {"name":"primary","enabled":true,"credential":{"alias":"primary"},"base_url":root},
        {"name":"backup","enabled":true,"credential":{"alias":"backup"},"base_url":root}
    ]}}});
    let boot = Fixture::with_accounts(
        service(),
        json!({"models":{"deepseek-v4.1-flash-1017":{"candidates":[{"binding":"deepseek-responses"}]}}}),
        &[
            ("deepseek", "primary", "synthetic-primary"),
            ("deepseek", "backup", "synthetic-backup"),
        ],
        accounts,
    )
    .unwrap();
    for expected in [StatusCode::SERVICE_UNAVAILABLE, StatusCode::OK] {
        let request = axum::http::Request::builder()
            .method("POST")
            .uri("/v1/responses")
            .header(
                "authorization",
                "Bearer synthetic-gateway-client-token-0001",
            )
            .header("content-type", "application/json")
            .body(axum::body::Body::from(
                r#"{"model":"deepseek-v4.1-flash-1017","input":"go","store":false}"#,
            ))
            .unwrap();
        let response = boot.gateway.router().oneshot(request).await.unwrap();
        assert_eq!(response.status(), expected);
        if expected == StatusCode::SERVICE_UNAVAILABLE {
            assert!(
                !response.headers().contains_key("retry-after"),
                "a ready backup prevents a route-wide delay"
            );
        }
        let bytes = axum::body::to_bytes(response.into_body(), 65536)
            .await
            .unwrap();
        if expected == StatusCode::OK {
            let value: Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(value["output"][0]["content"][0]["text"], "healthy");
        } else {
            assert_eq!(
                *calls.lock().unwrap(),
                ["primary"],
                "a hint beyond the recovery budget prevents a second send"
            );
        }
        let permit = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            boot.gateway.state.permits.acquire(),
        )
        .await
        .unwrap()
        .unwrap();
        drop(permit);
    }
    assert_eq!(*calls.lock().unwrap(), ["primary", "backup"]);
    boot.gateway.shutdown();
    upstream.abort();
    let _ = upstream.await;
}

#[test]
fn provider_recovery_configuration_reaches_bound_health_and_keeps_schema_strict() {
    use std::time::Duration;
    let mut settings = service();
    settings["routing"] = json!({"max_wait_ms":0});
    let models = json!({"models":{"deepseek-v4.1-flash-1017":{"candidates":[{"binding":"deepseek-responses"}]}}});
    let keys = [("deepseek", "primary", "synthetic-key")];
    let mut accounts = json!({"version":1,"providers":{"deepseek":{
        "accounts":[{"name":"primary","enabled":true,"credential":{"alias":"primary"}}],
        "recovery":{"transient_cooldown_ms":7,"transient_multiplier":0}
    }}});
    let boot =
        Fixture::with_accounts(settings.clone(), models.clone(), &keys, accounts.clone()).unwrap();
    assert!(boot.gateway.state.limits.max_wait.is_zero());
    let candidate = &boot
        .gateway
        .state
        .entries
        .values()
        .next()
        .unwrap()
        .candidates[0];
    let now = tokio::time::Instant::now();
    boot.gateway.state.executors.failed(
        candidate,
        crate::gateway::ApiError::failure(503, b"", None),
        now,
        None,
    );
    assert!(
        boot.gateway
            .state
            .executors
            .unavailable(&candidate.health, now + Duration::from_millis(6))
            .is_some()
    );
    assert!(
        boot.gateway
            .state
            .executors
            .unavailable(&candidate.health, now + Duration::from_millis(7))
            .is_none()
    );
    for invalid in [
        json!({"transient_cooldown_ms":"7"}),
        json!({"transient_cooldown_ms":-1}),
        json!({"unknown_policy":1}),
    ] {
        accounts["providers"]["deepseek"]["recovery"] = invalid;
        assert!(
            Fixture::with_accounts(settings.clone(), models.clone(), &keys, accounts.clone())
                .is_err()
        );
    }
}

#[test]
fn enabled_account_expansion_has_one_request_bound_independent_of_candidate_count() {
    let names: Vec<_> = (0..64).map(|n| format!("account-{n}")).collect();
    let keys: Vec<_> = names
        .iter()
        .map(|name| ("deepseek", name.as_str(), "synthetic-key"))
        .collect();
    let models = json!({"models":{"deepseek-v4.1-flash-1017":{"candidates":[{"binding":"deepseek-responses"}]}}});
    let single = Fixture::new(service(), models, &keys).unwrap();
    let entry = &single.gateway.state.entries[&(
        crate::protocol::ClientProtocol::Responses,
        "deepseek-v4.1-flash-1017".into(),
    )];
    assert_eq!(entry.candidates.len(), 64);
    assert!(
        Fixture::new(
            service(),
            json!({"models":{"deepseek-v4.1-flash-1017":{"candidates":[
                {"binding":"deepseek-responses"},{"binding":"deepseek-chat"}
            ]}}}),
            &keys
        )
        .is_err()
    );
}
#[tokio::test]
async fn read_only_validation_checks_observation_configuration_without_creating_sinks() {
    let boot = Fixture::new(
        service(),
        json!({"models":{"deepseek-v4.1-flash-1017":{"candidates":[{"binding":"deepseek-responses"}]}}}),
        &[("deepseek", "primary", "synthetic-primary")],
    )
    .unwrap();
    let root = boot.directory();
    let store = root.join("credentials");
    super::Bootstrap::validate(&root.join("gateway.json"), Some(&store), None).unwrap();
    let collector = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    collector.set_nonblocking(true).unwrap();
    let endpoint = format!("http://{}/v1/logs", collector.local_addr().unwrap());
    let mut config = json!({"version":1,"auth_file":"auth.json","diagnostics":{"endpoint":endpoint,"private_bodies":true}});
    super::test_support::write_private(
        &root.join("gateway.json"),
        &serde_json::to_vec(&config).unwrap(),
    );
    super::Bootstrap::validate(&root.join("gateway.json"), Some(&store), None).unwrap();
    let explanation =
        super::Bootstrap::explain(&root.join("gateway.json"), Some(&store), None).unwrap();
    assert!(!explanation.to_string().contains(&endpoint));
    assert!(
        matches!(collector.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock)
    );
    for invalid in [
        json!({"endpoint":"file:///private/diagnostics"}),
        json!({"endpoint":"https://user:password@example.invalid/v1/logs"}),
        json!({"endpoint":"https://example.invalid/v1/logs?token=private"}),
        json!({"endpoint":endpoint,"body_bytes":0}),
    ] {
        config["diagnostics"] = invalid;
        super::test_support::write_private(
            &root.join("gateway.json"),
            &serde_json::to_vec(&config).unwrap(),
        );
        assert!(
            super::Bootstrap::validate(&root.join("gateway.json"), Some(&store), None).is_err()
        );
    }
}
#[test]
fn account_names_cannot_duplicate_one_security_target() {
    let mut accounts = json!({"version":1,"providers":{"deepseek":{"accounts":[
        {"name":"primary","enabled":true,"credential":{"alias":"shared"}},
        {"name":"another-name","enabled":true,"credential":{"alias":"shared"}}
    ]}}});
    let models = json!({"models":{"deepseek-v4.1-flash-1017":{"candidates":[{"binding":"deepseek-responses"}]}}});
    let keys = [("deepseek", "shared", "synthetic-key")];
    assert!(matches!(
        Fixture::with_accounts(service(), models.clone(), &keys, accounts.clone()),
        Err(super::StartupError::Binding)
    ));
    accounts["providers"]["deepseek"]["accounts"][1]["base_url"] =
        json!("https://relay.invalid/custom/v1");
    let distinct = Fixture::with_accounts(service(), models, &keys, accounts).unwrap();
    let entry = &distinct.gateway.state.entries[&(
        crate::protocol::ClientProtocol::Responses,
        "deepseek-v4.1-flash-1017".into(),
    )];
    assert_eq!(entry.candidates.len(), 2);
    assert_ne!(
        entry.candidates[0].cache_scope(),
        entry.candidates[1].cache_scope()
    );
}

#[test]
fn model_switch_keeps_reasoning_but_not_compaction_or_cache_identity() {
    use crate::{
        execution::replay::{open_from_client, seal_for_client},
        protocol::{
            ClientProtocol,
            openai::{Dialect, GenerationApi, GenerationCodec, TargetContract},
            replay_envelope::CLIENT_ORIGIN,
        },
    };
    let boot = Fixture::new(
        service(),
        json!({"models":{
            "gpt-6-astra":{"candidates":[{"binding":"openai-astra-responses"}]},
            "gpt-6-luna":{"candidates":[{"binding":"openai-luna-responses"}]}
        }}),
        &[
            ("openai", "primary", "synthetic-key"),
            ("openai", "secondary", "other-synthetic-key"),
        ],
    )
    .unwrap();
    let source = &boot.gateway.state.entries[&(ClientProtocol::Responses, "gpt-6-astra".into())]
        .candidates[0];
    let target = &boot.gateway.state.entries[&(ClientProtocol::Responses, "gpt-6-luna".into())]
        .candidates[0];
    assert_ne!(source.cache_scope(), target.cache_scope());
    let codec = source.endpoint.adapter().unwrap().openai().unwrap().clone();
    let client = GenerationCodec::new(
        GenerationApi::Responses,
        Dialect::Standard,
        Some(CLIENT_ORIGIN.clone()),
    );
    let client_contract = TargetContract {
        replay_origin: Some(CLIENT_ORIGIN.clone()),
        ..TargetContract::full()
    };
    for compacted in [false, true] {
        for stream in [false, true] {
            let mut output = json!([
                {"id":"reason","type":"reasoning","status":"completed","summary":[],
                    "encrypted_content":"synthetic-reasoning"},
                {"id":"call","type":"function_call","status":"completed",
                    "call_id":"read-call","name":"read","arguments":"{ \"key\": \"A\" }"}
            ]);
            if compacted {
                output.as_array_mut().unwrap().insert(
                    0,
                    json!({
                        "id":"compact","type":"compaction","encrypted_content":"synthetic-context"
                    }),
                );
            }
            let wire = json!({
                "id":"response","object":"response","created_at":1,"model":"gpt-6-astra",
                "status":"completed","output":output
            });
            let mut response = if stream {
                let mut decoder = codec.event_decoder();
                let mut opening = wire.clone();
                opening["status"] = json!("in_progress");
                opening["output"] = json!([]);
                decoder
                    .push(&json!({"type":"response.created","response":opening}))
                    .unwrap();
                for (index, item) in output.as_array().unwrap().iter().enumerate() {
                    let mut opening = item.clone();
                    if item["type"] == "compaction" {
                        opening["encrypted_content"] = json!("");
                    } else {
                        opening["status"] = json!("in_progress");
                        if item["type"] == "reasoning" {
                            opening.as_object_mut().unwrap().remove("encrypted_content");
                        } else {
                            opening["arguments"] = json!("");
                        }
                    }
                    decoder.push(&json!({"type":"response.output_item.added","output_index":index,"item":opening})).unwrap();
                    if item["type"] == "function_call" {
                        decoder
                            .push(&json!({"type":"response.function_call_arguments.delta",
                        "output_index":index,"item_id":item["id"],"delta":item["arguments"]}))
                            .unwrap();
                        decoder
                            .push(&json!({"type":"response.function_call_arguments.done",
                        "output_index":index,"item_id":item["id"],"arguments":item["arguments"]}))
                            .unwrap();
                    }
                    decoder.push(&json!({"type":"response.output_item.done","output_index":index,"item":item})).unwrap();
                }
                decoder
                    .push(&json!({"type":"response.completed","response":wire}))
                    .unwrap();
                decoder.materialize().unwrap()
            } else {
                codec.decode_response(wire.to_string().as_bytes()).unwrap()
            };
            seal_for_client(&mut response, &CLIENT_ORIGIN).unwrap();
            let delivered = client.encode_response(&response, &client_contract).unwrap();
            let mut input = delivered["output"].as_array().unwrap().clone();
            input.push(
                json!({"type":"function_call_output","call_id":"read-call","output":"value-A"}),
            );
            let mut request = client
                .decode_client_request(
                    json!({
                        "model":"gpt-6-luna","store":false,"input":input
                    })
                    .to_string()
                    .as_bytes(),
                )
                .unwrap();
            open_from_client(&mut request).unwrap();
            let before = request.clone();
            let encode = |candidate: &crate::gateway::BoundCandidate| {
                if compacted {
                    // Exercise compaction ownership independently of catalog activation.
                    let adapter = candidate.endpoint.adapter().unwrap();
                    adapter.openai().unwrap().encode_request(
                        &request,
                        &candidate.endpoint.upstream_model,
                        &TargetContract {
                            replay_origin: Some(candidate.endpoint.replay_origin()),
                            ..TargetContract::full()
                        },
                    )
                } else {
                    candidate.endpoint.encode_request(&request)
                }
            };
            let same = encode(source).unwrap();
            assert!(
                same["input"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|item| item["encrypted_content"] == "synthetic-reasoning")
            );
            let switched = encode(target);
            if compacted {
                assert_eq!(
                    switched.unwrap_err().cause,
                    crate::protocol::conversion::ConversionCause::Codec(
                        crate::protocol::CodecError::Unsupported(
                            crate::protocol::failure::CarrierReason::Replay
                        )
                    ),
                );
            } else {
                let switched = switched.unwrap();
                assert_eq!(switched["model"], "gpt-6-luna");
                assert_eq!(
                    switched["input"][0]["encrypted_content"],
                    "synthetic-reasoning"
                );
                assert_eq!(switched["input"][1]["call_id"], "read-call");
                assert_eq!(switched["input"][1]["arguments"], "{ \"key\": \"A\" }");
                assert_eq!(switched["input"][2]["output"], "value-A");
                let native = target
                    .endpoint
                    .adapter()
                    .unwrap()
                    .openai()
                    .unwrap()
                    .decode_request(switched.to_string().as_bytes())
                    .unwrap();
                let replayed = target.endpoint.encode_request(&native).unwrap();
                assert_eq!(
                    replayed["input"][0]["encrypted_content"],
                    "synthetic-reasoning"
                );
                let other = &boot.gateway.state.entries
                    [&(ClientProtocol::Responses, "gpt-6-luna".into())]
                    .candidates[1];
                let transferred = other.endpoint.encode_request(&request).unwrap();
                assert_eq!(transferred["input"].as_array().unwrap().len(), 2);
                assert_eq!(transferred["input"][0]["call_id"], "read-call");
                assert_eq!(transferred["input"][1]["output"], "value-A");

                let mut explicit = request.clone();
                let mut settings = explicit.task.semantic.settings().clone();
                settings.reasoning.presence =
                    crate::semantic::task::generation::ReasoningPresence::Present;
                settings.reasoning.read_history = Some(true);
                explicit.task.semantic = explicit.task.semantic.with_settings(settings).unwrap();
                assert_eq!(
                    other.endpoint.encode_request(&explicit).unwrap_err().cause,
                    crate::protocol::conversion::ConversionCause::Codec(
                        crate::protocol::CodecError::Unsupported(
                            crate::protocol::failure::CarrierReason::Reasoning
                        )
                    ),
                );

                // Missing recorded owners must fail before target-specific pruning.
                let mut missing = request.clone();
                missing.task.semantic = missing
                    .task
                    .semantic
                    .retain_items(|_, item| {
                        !matches!(item, crate::semantic::task::generation::Item::Reasoning(_))
                    })
                    .unwrap();
                assert!(target.endpoint.encode_request(&missing).is_err());
            }
            assert_eq!(request, before);
        }
    }
}
