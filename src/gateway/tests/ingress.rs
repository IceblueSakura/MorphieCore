//! Lowest-owner ingress checks; no real Provider I/O.
use crate::{
    gateway::{
        ingress::{
            admission,
            http::{authenticate, handle},
        },
        tests::{
            support::{KEY, gateway},
            topology::topology,
        },
        *,
    },
    protocol::ClientProtocol,
    provider::CredentialBindingId,
    topology::EndpointId,
};
use axum::{
    Router,
    http::{HeaderMap, Request as HttpRequest, StatusCode},
    middleware,
};
use std::time::Duration;
use tower::ServiceExt;
#[test]
fn client_profile_cannot_depend_on_candidate_order_or_select_responses_extensions() {
    let entries = |profiles: &[(GenerationApi, ChatProfile)]| {
        profiles
            .iter()
            .map(|(protocol, chat_profile)| Entry {
                model: "fixture-model".into(),
                protocol: (*protocol).into(),
                chat_profile: *chat_profile,
                endpoint: EndpointId::new("fixture-chat").unwrap(),
            })
            .collect()
    };
    for profiles in [
        vec![
            (GenerationApi::Chat, ChatProfile::Standard),
            (GenerationApi::Chat, ChatProfile::ReadableReasoning),
        ],
        vec![
            (GenerationApi::Chat, ChatProfile::ReadableReasoning),
            (GenerationApi::Chat, ChatProfile::Standard),
        ],
        vec![(GenerationApi::Responses, ChatProfile::ReadableReasoning)],
    ] {
        assert!(
            Gateway::new(
                topology(),
                TaskEntries {
                    generation: entries(&profiles),
                    ..Default::default()
                },
                BTreeMap::from([(
                    CredentialBindingId::new("fixture-key").unwrap(),
                    Arc::new(SecretMaterial::new("synthetic-upstream").unwrap())
                )]),
                std::collections::BTreeMap::from([(
                    "fixture-agent".into(),
                    SecretMaterial::new(KEY).unwrap()
                )]),
                Limits::default(),
                None
            )
            .is_err()
        );
    }
}

#[tokio::test]
async fn source_decode_and_planning_failures_have_separate_boundaries_without_attempts() {
    use crate::execution::report::Report;
    let mut gate = gateway(Limits::default());
    let state = Arc::get_mut(&mut gate.state).unwrap();
    let mut entry = state.entries.values().next().unwrap().as_ref().clone();
    entry.client = crate::adapter::ClientAdapter::openai(
        crate::protocol::openai::GenerationCodec::new(
            GenerationApi::Responses,
            crate::protocol::openai::Dialect::Standard,
            None,
        ),
        crate::protocol::openai::TargetContract::full(),
    );
    entry
        .public
        .contract
        .set_native(crate::semantic::task::generation::Feature::Tools, false);
    state.entries.insert(
        (ClientProtocol::Responses, "fixture-model".into()),
        Arc::new(entry),
    );
    let wire = serde_json::json!({"model":"fixture-model",
        "input":[{"role":"assistant","content":"prior"}],
        "tools":[{"type":"function","name":"lookup","strict":false}]});
    let report = Report::default();
    admission::prepare(
        state,
        ClientProtocol::Responses,
        &serde_json::to_vec(&wire).unwrap(),
        &report,
    )
    .unwrap();
    let mut invalid = wire.clone();
    invalid["tools"][0]["name"] = serde_json::Value::Null;
    let failed_decode = Report::default();
    assert!(
        admission::prepare(
            state,
            ClientProtocol::Responses,
            &serde_json::to_vec(&invalid).unwrap(),
            &failed_decode
        )
        .is_err()
    );
    assert_eq!(
        failed_decode.cause().unwrap().boundary,
        Some(crate::protocol::conversion::generation::ConversionBoundary::RequestDecode)
    );
    let entry = Arc::make_mut(
        state
            .entries
            .get_mut(&(ClientProtocol::Responses, "fixture-model".into()))
            .unwrap(),
    );
    entry
        .public
        .contract
        .set_native(crate::semantic::task::generation::Feature::Tools, true);
    for candidate in &mut entry.candidates {
        Arc::make_mut(candidate)
            .endpoint
            .execution
            .request_body_limit = 1;
    }
    let (sink, exporter) = crate::gateway::diagnostics::Sink::memory();
    let trace = crate::gateway::diagnostics::Trace::generation_trace(Some(&sink));
    let report = trace.report.clone();
    let (entry, request) = admission::prepare(
        state,
        ClientProtocol::Responses,
        &serde_json::to_vec(&wire).unwrap(),
        &report,
    )
    .unwrap();
    let error = crate::gateway::generation::exchange::run(
        gate.state.clone(),
        entry,
        request,
        None,
        tokio::time::Instant::now() + Duration::from_secs(1),
        gate.state.permits.clone().acquire_owned().await.unwrap(),
        trace,
    )
    .await
    .unwrap_err();
    assert_eq!(error.status, 400);
    assert_eq!(report.request_error().unwrap().1, 400);
    let logs = exporter.get_emitted_logs().unwrap();
    assert_eq!(logs.len(), 1);
    let encoded = format!("{logs:?}");
    assert!(!encoded.contains("fixture-chat"));
    assert!(!encoded.contains("prior"));
}

#[tokio::test]
async fn activated_members_keep_route_order_with_a_shared_client_carrier() {
    let entries = ["fixture-chat", "fixture-responses"]
        .into_iter()
        .map(|id| Entry {
            chat_profile: Default::default(),
            model: "fixture-model".into(),
            protocol: crate::protocol::ClientProtocol::Responses,
            endpoint: EndpointId::new(id).unwrap(),
        })
        .collect();
    let gate = Gateway::new(
        topology(),
        TaskEntries {
            generation: entries,
            ..Default::default()
        },
        BTreeMap::from([(
            CredentialBindingId::new("fixture-key").unwrap(),
            Arc::new(SecretMaterial::new("synthetic-upstream-credential-0001").unwrap()),
        )]),
        std::collections::BTreeMap::from([(
            "fixture-agent".into(),
            SecretMaterial::new(KEY).unwrap(),
        )]),
        Limits::default(),
        None,
    )
    .unwrap();
    let entry = &gate.state.entries[&(
        crate::protocol::ClientProtocol::Responses,
        "fixture-model".into(),
    )];
    assert_eq!(
        entry
            .client
            .as_openai()
            .unwrap()
            .0
            .adaptation
            .scope
            .as_ref(),
        Some(&*crate::protocol::replay_envelope::CLIENT_ORIGIN)
    );
    let request = entry
        .client
        .as_openai()
        .unwrap()
        .0
        .decode_request(br#"{"model":"fixture-model","input":"x"}"#)
        .unwrap();
    assert_eq!(
        entry
            .eligible(&request, &gate.state.affinity)
            .unwrap()
            .iter()
            .map(|c| c.candidate.endpoint.id.as_str())
            .collect::<Vec<_>>(),
        vec!["fixture-responses", "fixture-chat"]
    );
    let opaque = entry
        .client
        .as_openai()
        .unwrap()
        .0
        .decode_request(
            br#"{"model":"fixture-model","input":"x","include":["reasoning.encrypted_content"]}"#,
        )
        .unwrap();
    let before = opaque.clone();
    let selected = entry.eligible(&opaque, &gate.state.affinity).unwrap();
    assert_eq!(
        selected
            .iter()
            .map(|c| c.candidate.endpoint.id.as_str())
            .collect::<Vec<_>>(),
        ["fixture-responses", "fixture-chat"]
    );
    let (_, rules) = selected[1]
        .candidate
        .endpoint
        .encode_request_with_rules(&opaque)
        .unwrap();
    assert_eq!(rules, ["generation.omit-encrypted-reasoning-include.v1"]);
    assert_eq!(opaque, before);
}

#[test]
fn ingress_unwraps_only_the_candidate_compatible_opaque_and_never_selects_a_target() {
    use crate::{protocol::replay_envelope, semantic::task::generation::ReplayFormat};
    use serde_json::json;
    let gate = Gateway::new(
        topology(),
        TaskEntries {
            generation: vec![Entry {
                chat_profile: Default::default(),
                model: "fixture-model".into(),
                protocol: crate::protocol::ClientProtocol::Responses,
                endpoint: EndpointId::new("fixture-responses").unwrap(),
            }],
            ..Default::default()
        },
        BTreeMap::from([(
            CredentialBindingId::new("fixture-key").unwrap(),
            Arc::new(SecretMaterial::new("synthetic-upstream-credential-0001").unwrap()),
        )]),
        std::collections::BTreeMap::from([(
            "fixture-agent".into(),
            SecretMaterial::new(KEY).unwrap(),
        )]),
        Limits::default(),
        None,
    )
    .unwrap();
    for (provider, model, matches) in [
        ("fixture", "private-fixture", true),
        ("other", "private-fixture", false),
        ("fixture", "other", false),
    ] {
        let mut issuer = gate.state.entries[&(
            crate::protocol::ClientProtocol::Responses,
            "fixture-model".into(),
        )]
            .candidates[0]
            .endpoint
            .clone();
        issuer.provider = crate::provider::ProviderId::new(provider).unwrap();
        issuer.upstream_model = model.into();
        if !matches {
            issuer.bound_replay_scope = None;
        }
        let token = replay_envelope::seal(
            &issuer.replay_origin(),
            ReplayFormat::ResponsesEncrypted,
            "synthetic-client-payload",
        )
        .unwrap();
        let body = json!({"model":"fixture-model","input":[
            {"type":"reasoning","id":"rs","summary":[],"encrypted_content":token.as_str()},
            {"role":"user","content":"next"}
        ]});
        let (entry, request) = admission::prepare(
            &gate.state,
            ClientProtocol::Responses,
            body.to_string().as_bytes(),
            &crate::execution::report::Report::default(),
        )
        .unwrap();
        let before = request.clone();
        let candidates = entry.eligible(&request, &gate.state.affinity).unwrap();
        assert_eq!(candidates.len(), 1);
        let candidate = &candidates[0].candidate;
        let secret = candidate.secret.resolve().unwrap().0;
        let prepared =
            crate::execution::prepare(&candidate.endpoint, &candidate.provider, &secret, &request)
                .unwrap();
        let wire: serde_json::Value = serde_json::from_slice(&prepared.body).unwrap();
        assert_eq!(wire["model"], "private-fixture");
        assert_eq!(
            wire["input"].as_array().unwrap().len(),
            if matches { 2 } else { 1 }
        );
        assert_eq!(
            wire.to_string().contains("synthetic-client-payload"),
            matches
        );
        assert!(!wire.to_string().contains("mc1."));
        assert_eq!(request, before);
    }
}

#[tokio::test]
async fn model_request_limits_do_not_suppress_client_reported_facts() {
    use serde_json::json;
    let gate = Gateway::new(
        topology(),
        TaskEntries {
            generation: vec![Entry {
                chat_profile: Default::default(),
                model: "limited-model".into(),
                protocol: crate::protocol::ClientProtocol::Responses,
                endpoint: EndpointId::new("limited-responses").unwrap(),
            }],
            ..Default::default()
        },
        BTreeMap::from([(
            CredentialBindingId::new("fixture-key").unwrap(),
            Arc::new(SecretMaterial::new("synthetic-upstream-credential-0001").unwrap()),
        )]),
        std::collections::BTreeMap::from([(
            "fixture-agent".into(),
            SecretMaterial::new(KEY).unwrap(),
        )]),
        Limits::default(),
        None,
    )
    .unwrap();
    let entry = &gate.state.entries[&(
        crate::protocol::ClientProtocol::Responses,
        "limited-model".into(),
    )];
    let request = entry
        .client
        .as_openai()
        .unwrap()
        .0
        .decode_request(
            &serde_json::to_vec(&json!({
                "model":"limited-model","input":"synthetic","temperature":0.5
            }))
            .unwrap(),
        )
        .unwrap();
    let (wire, rules) = entry
        .client
        .as_openai()
        .unwrap()
        .0
        .encode_request_with_rules(
            &request,
            "synthetic",
            entry.candidates[0]
                .endpoint
                .representation
                .openai()
                .unwrap(),
        )
        .unwrap();
    assert!(wire.get("temperature").is_none());
    assert_eq!(rules, ["generation.omit-temperature.v1"]);
    let provider = GenerationCodec {
        protocol: GenerationApi::Responses,
        adaptation: entry.candidates[0]
            .endpoint
            .representation
            .openai()
            .unwrap()
            .adaptation
            .clone(),
    };
    let response = provider.decode_response(&serde_json::to_vec(&json!({
        "object":"response","id":"r","model":"synthetic","created_at":1,"status":"completed",
        "output":[{"type":"message","id":"msg","role":"assistant","status":"completed",
            "content":[{"type":"output_text","text":"x","annotations":[],"logprobs":[]}]}],
        "temperature":1,"top_p":1,"parallel_tool_calls":true
    })).unwrap()).unwrap();
    let (_, encoded) = entry.client.snapshot(response).unwrap();
    assert_eq!(encoded["output"][0]["content"][0]["logprobs"], json!([]));
    assert_eq!(encoded["temperature"].as_f64(), Some(1.0));
    assert_eq!(encoded["top_p"].as_f64(), Some(1.0));
    assert_eq!(encoded["parallel_tool_calls"], true);
}

#[tokio::test]
async fn diagnostics_only_record_authenticated_requests_and_never_expose_headers() {
    let (sink, exporter) = crate::gateway::diagnostics::Sink::memory();
    let mut gate = gateway(Limits::default());
    let state = Arc::get_mut(&mut gate.state).unwrap();
    state.diagnostics = Some(sink);
    state.capture = Some(crate::gateway::capture::Sink::new(16 << 10));
    for authenticated in [false, true] {
        let mut request = HttpRequest::builder()
            .method("POST")
            .uri("/v1/chat/completions?api_key=synthetic-query-secret")
            .header("x-private", "synthetic-private-header")
            .header("content-type", "application/json");
        if authenticated {
            request = request.header("authorization", format!("Bearer {KEY}"));
        }
        let response = gate
            .router()
            .oneshot(
                request
                    .body(axum::body::Body::from("invalid json"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), if authenticated { 400 } else { 401 });
        axum::body::to_bytes(response.into_body(), 65536)
            .await
            .unwrap();
    }
    let logs = exporter.get_emitted_logs().unwrap();
    assert!(!logs.is_empty());
    for log in &logs {
        assert!(
            log.record
                .attributes_iter()
                .any(|(key, value)| key.as_str() == "agent_id"
                    && *value == opentelemetry::logs::AnyValue::from("fixture-agent"))
        );
    }
    let text = format!("{logs:?}");
    for secret in [
        KEY,
        "synthetic-query-secret",
        "synthetic-private-header",
        "invalid json",
    ] {
        assert!(!text.contains(secret));
    }
    assert!(text.contains("client_response"));
}
#[tokio::test]
async fn authenticated_agent_is_fixed_before_optional_capture() {
    use crate::gateway::ingress::{
        auth::{Auth, AuthenticatedAgent},
        correlation::RequestId,
    };
    use axum::extract::Extension;
    const OTHER: &str = "synthetic-second-client-token-00002";
    for (diagnostics, capture) in [(false, false), (true, false), (true, true)] {
        let mut gate = gateway(Limits::default());
        Arc::get_mut(&mut gate.state).unwrap().auth = Auth::new(BTreeMap::from([
            ("pi".into(), SecretMaterial::new(KEY).unwrap()),
            ("opencode".into(), SecretMaterial::new(OTHER).unwrap()),
        ]))
        .unwrap();
        if diagnostics {
            Arc::get_mut(&mut gate.state).unwrap().diagnostics =
                Some(crate::gateway::diagnostics::Sink::memory().0);
        }
        if capture {
            Arc::get_mut(&mut gate.state).unwrap().capture =
                Some(crate::gateway::capture::Sink::new(16 << 10));
        }
        let app = Router::new()
            .fallback(
                |Extension(agent): Extension<AuthenticatedAgent>,
                 Extension(id): Extension<RequestId>| async move {
                    axum::Json(serde_json::json!({
                        "agent_id":agent.agent_id.as_ref(),"request_id":id.0.as_ref()
                    }))
                },
            )
            .layer(middleware::from_fn_with_state(
                gate.state.clone(),
                authenticate,
            ));
        let mut references = std::collections::BTreeSet::new();
        for (key, expected) in [(KEY, "pi"), (OTHER, "opencode"), (KEY, "pi")] {
            let forbidden = futures_util::stream::poll_fn(
                |_| -> std::task::Poll<Option<Result<bytes::Bytes, std::io::Error>>> {
                    panic!("identity observer must not poll the body")
                },
            );
            let response = app
                .clone()
                .oneshot(
                    HttpRequest::builder()
                        .uri("/synthetic-identity-observer")
                        .header("x-api-key", key)
                        .header("agent_id", "forged")
                        .header("x-morphiecore-agent-id", "forged")
                        .header("x-request-id", "forged")
                        .body(axum::body::Body::from_stream(forbidden))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), 200);
            let bytes = axum::body::to_bytes(response.into_body(), 512)
                .await
                .unwrap();
            let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(value["agent_id"], expected);
            let id = value["request_id"].as_str().unwrap();
            assert_ne!(id, "forged");
            assert!(references.insert(id.to_owned()));
        }
    }
}

#[tokio::test]
async fn authentication_runs_before_any_body_poll() {
    let gate = gateway(Limits::default());
    for (path, expected, kind) in [
        ("/v1/messages", 405, "invalid_request_error"),
        ("/v1/messages/unsupported", 404, "not_found_error"),
    ] {
        let response = gate
            .router()
            .oneshot(
                HttpRequest::builder()
                    .method("GET")
                    .uri(path)
                    .header("x-api-key", KEY)
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), expected);
        let body: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(response.into_body(), 4096)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(body["type"], "error");
        assert_eq!(body["error"]["type"], kind);
    }
    let app = Router::new()
        .fallback(|| async {
            panic!("unauthenticated handler ran");
            #[allow(unreachable_code)]
            StatusCode::OK
        })
        .layer(middleware::from_fn_with_state(
            gate.state.clone(),
            authenticate,
        ));
    for path in ["/v1/chat/completions", "/v1/responses", "/v1/messages"] {
        let forbidden = futures_util::stream::poll_fn(
            |_| -> std::task::Poll<Option<Result<bytes::Bytes, std::io::Error>>> {
                panic!("unauthenticated body was polled")
            },
        );
        let response = app
            .clone()
            .oneshot(
                HttpRequest::builder()
                    .uri(path)
                    .body(axum::body::Body::from_stream(forbidden))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), 401);
        assert_eq!(response.headers()["www-authenticate"], "Bearer");
        let bytes = axum::body::to_bytes(response.into_body(), 4096)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        if path == "/v1/messages" {
            assert_eq!(body["type"], "error");
            assert_eq!(body["error"]["type"], "authentication_error");
        } else {
            assert_eq!(body["error"]["code"], "unauthorized");
        }
    }
}
#[tokio::test]
async fn messages_versions_are_single_and_checked_before_body_poll() {
    let gate = gateway(Limits::default());
    for header in ["anthropic-user-profile-id", "anthropic-workspace-id"] {
        let forbidden = futures_util::stream::poll_fn(
            |_| -> std::task::Poll<Option<Result<bytes::Bytes, std::io::Error>>> {
                panic!("unsupported Messages attribution must not poll body")
            },
        );
        let response = gate
            .router()
            .oneshot(
                HttpRequest::builder()
                    .method("POST")
                    .uri("/v1/messages")
                    .header("x-api-key", KEY)
                    .header("content-type", "application/json")
                    .header("anthropic-version", "2026-10-21")
                    .header(header, "")
                    .body(axum::body::Body::from_stream(forbidden))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), 400);
    }
    for path in ["/v1/messages", "/v1/messages?beta=true"] {
        for versions in [
            vec![],
            vec![""],
            vec!["2026-10-22"],
            vec!["invalid"],
            vec!["2026-10-21, 2026-10-21"],
            vec!["2026-10-21", "2026-10-21"],
            vec!["2023-06-01", "2026-10-21"],
        ] {
            let mut request = HttpRequest::builder()
                .method("POST")
                .uri(path)
                .header("x-api-key", KEY)
                .header("content-type", "application/json");
            for version in versions {
                request = request.header("anthropic-version", version);
            }
            let forbidden = futures_util::stream::poll_fn(
                |_| -> std::task::Poll<Option<Result<bytes::Bytes, std::io::Error>>> {
                    panic!("invalid Messages version must not poll body")
                },
            );
            let response = gate
                .router()
                .oneshot(
                    request
                        .body(axum::body::Body::from_stream(forbidden))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), 400);
            assert_eq!(response.headers()["cache-control"], "no-store");
            let body: serde_json::Value = serde_json::from_slice(
                &axum::body::to_bytes(response.into_body(), 4096)
                    .await
                    .unwrap(),
            )
            .unwrap();
            assert_eq!(body["type"], "error");
            assert_eq!(body["error"]["type"], "invalid_request_error");
        }
        for version in ["2023-06-01", "2026-10-21"] {
            // Beta headers do not select body semantics or upstream headers.
            let response = gate.router().oneshot(
            HttpRequest::builder().method("POST").uri(path)
                .header("x-api-key", KEY).header("content-type", "application/json")
                .header("anthropic-version", version)
                .header("anthropic-beta", "interleaved-thinking-2025-05-14")
                .header("anthropic-beta", "unknown-feature")
                .body(axum::body::Body::from(r#"{"model":"unregistered","max_tokens":32,"messages":[{"role":"user","content":"hi"}]}"#))
                .unwrap()
        ).await.unwrap();
            assert_eq!(response.status(), 404);
            let body: serde_json::Value = serde_json::from_slice(
                &axum::body::to_bytes(response.into_body(), 4096)
                    .await
                    .unwrap(),
            )
            .unwrap();
            assert!(
                body["error"]["message"]
                    .as_str()
                    .unwrap()
                    .contains("model_not_found")
            );
        }
    }
}
#[tokio::test]
async fn identity_budget_and_scope_are_owned_by_trusted_ingress() {
    let gate = gateway(Limits::default());
    // The output cap is the client's choice: absent stays absent, large values pass.
    for (wire, cap) in [
        (
            br#"{"model":"fixture-model","messages":[{"role":"user","content":"hello"}]}"#
                .as_slice(),
            None,
        ),
        (
            br#"{"model":"fixture-model","messages":[{"role":"user","content":"hello"}],"max_completion_tokens":200000}"#,
            Some(200_000),
        ),
    ] {
        let (_, request) = admission::prepare(&gate.state, ClientProtocol::Chat, wire,&crate::execution::report::Report::default()).unwrap();
        assert_eq!(request.task.semantic.controls().max_output_tokens, cap);
    }
    assert_eq!(
        admission::prepare(
            &gate.state,
            ClientProtocol::Chat,
            br#"{"model":"fixture-model","model":"missing","messages":[]}"#,
            &crate::execution::report::Report::default()
        )
        .err()
        .unwrap()
        .status,
        400
    );
    // Public identity resolves before task contents, not by guessing a Provider.
    assert_eq!(
        admission::prepare(
            &gate.state,
            ClientProtocol::Chat,
            br#"{"model":"missing","messages":null}"#,
            &crate::execution::report::Report::default()
        )
        .err()
        .unwrap()
        .status,
        404
    );
    let mut bad_headers = HeaderMap::new();
    bad_headers.insert("content-type", "application/json".parse().unwrap());
    bad_headers.insert("content-encoding", "gzip".parse().unwrap());
    assert_eq!(
        admission::headers(&bad_headers, 100).unwrap_err().status,
        415
    );
    let chunks = futures_util::stream::iter([
        Ok::<_, std::io::Error>(bytes::Bytes::from_static(b"123")),
        Ok(bytes::Bytes::from_static(b"456")),
    ]);
    assert_eq!(
        admission::collect(axum::body::Body::from_stream(chunks), 5)
            .await
            .unwrap_err()
            .status,
        413
    );
}
#[test]
fn clients_only_resolve_public_labels_and_cannot_select_a_provider() {
    // Activate one Chat target for both client wire families without I/O.
    let gate = Gateway::new(
        topology(),
        TaskEntries {
            generation: [GenerationApi::Chat, GenerationApi::Responses]
                .into_iter()
                .map(|protocol| Entry {
                    chat_profile: Default::default(),
                    model: "fixture-model".into(),
                    protocol: protocol.into(),
                    endpoint: EndpointId::new("fixture-chat").unwrap(),
                })
                .collect(),
            ..Default::default()
        },
        BTreeMap::from([(
            CredentialBindingId::new("fixture-key").unwrap(),
            Arc::new(SecretMaterial::new("synthetic-upstream-credential-0001").unwrap()),
        )]),
        std::collections::BTreeMap::from([(
            "fixture-agent".into(),
            SecretMaterial::new(KEY).unwrap(),
        )]),
        Limits::default(),
        None,
    )
    .unwrap();
    for profile in [GenerationApi::Chat, GenerationApi::Responses] {
        let original = if profile == GenerationApi::Chat {
            serde_json::json!({"model":"fixture-model","messages":[{"role":"user","content":"hi"}]})
        } else {
            serde_json::json!({"model":"fixture-model","input":"hi"})
        };
        let (base, admitted) = admission::prepare(
            &gate.state,
            crate::protocol::ClientProtocol::from(profile),
            &serde_json::to_vec(&original).unwrap(),
            &crate::execution::report::Report::default(),
        )
        .unwrap();
        // Unregistered selectors are ignored by name and never reach the request.
        for (field, value) in [
            ("provider", serde_json::json!("other")),
            ("provider", serde_json::json!({"order":["fixture"]})),
            (
                "upstream_url",
                serde_json::json!("https://untrusted.invalid"),
            ),
        ] {
            let mut wire = original.clone();
            wire[field] = value;
            let (entry, request) = admission::prepare(
                &gate.state,
                crate::protocol::ClientProtocol::from(profile),
                &serde_json::to_vec(&wire).unwrap(),
                &crate::execution::report::Report::default(),
            )
            .unwrap();
            assert_eq!(request.ignored_fields, [field]);
            assert_eq!(request.task, admitted.task);
            assert_eq!(
                entry.candidates[0].endpoint.id,
                base.candidates[0].endpoint.id
            );
        }
        let (nested, unknown_kind) = if profile == GenerationApi::Chat {
            (
                serde_json::json!({"model":"fixture-model","messages":[{"role":"user","content":"hi","x_trace":1}]}),
                serde_json::json!({"model":"fixture-model","messages":[{"role":"critic","content":"hi"}]}),
            )
        } else {
            (
                serde_json::json!({"model":"fixture-model","input":[{"type":"message","role":"user","content":"hi","x_trace":1}]}),
                serde_json::json!({"model":"fixture-model","input":[{"type":"future_item","content":"hi"}]}),
            )
        };
        let (_, request) = admission::prepare(
            &gate.state,
            crate::protocol::ClientProtocol::from(profile),
            &serde_json::to_vec(&nested).unwrap(),
            &crate::execution::report::Report::default(),
        )
        .unwrap();
        assert_eq!(request.ignored_fields, ["x_trace"]);
        // Discriminators are values; ignoring an unknown kind would drop content.
        assert!(
            admission::prepare(
                &gate.state,
                crate::protocol::ClientProtocol::from(profile),
                &serde_json::to_vec(&unknown_kind).unwrap(),
                &crate::execution::report::Report::default()
            )
            .is_err()
        );
        let mut wire = original;
        wire["model"] = serde_json::json!("fixture/fixture-model");
        assert_eq!(
            admission::prepare(
                &gate.state,
                crate::protocol::ClientProtocol::from(profile),
                &serde_json::to_vec(&wire).unwrap(),
                &crate::execution::report::Report::default()
            )
            .err()
            .unwrap()
            .status,
            404
        );
    }
}

#[tokio::test(start_paused = true)]
async fn task_upload_timeouts_keep_status_severity_and_deadline_attribution() {
    use opentelemetry::logs::{AnyValue, Severity};
    for path in [
        "/v1/embeddings",
        "/v1/images/generations",
        "/v1/audio/speech",
        "/v1/audio/transcriptions",
    ] {
        for (body_ms, exchange_ms, status, severity, cause) in [
            (5, 100, 408, Severity::Warn, "deadline.request_body"),
            (100, 5, 504, Severity::Error, "deadline.exchange"),
        ] {
            let mut gate = gateway(Limits {
                concurrency: 1,
                body_timeout: Duration::from_millis(body_ms),
                exchange_timeout: Duration::from_millis(exchange_ms),
                ..Default::default()
            });
            let (sink, exporter) = crate::gateway::diagnostics::Sink::memory();
            Arc::get_mut(&mut gate.state).unwrap().diagnostics = Some(sink);
            let response = gate.router().oneshot(
                HttpRequest::builder()
                    .method("POST")
                    .uri(path)
                    .header("authorization", format!("Bearer {KEY}"))
                    .header(
                        "content-type",
                        if path.ends_with("transcriptions") {
                            "multipart/form-data; boundary=synthetic"
                        } else {
                            "application/json"
                        },
                    )
                    .body(axum::body::Body::from_stream(
                        futures_util::stream::pending::<Result<bytes::Bytes, std::io::Error>>(),
                    ))
                    .unwrap(),
            );
            tokio::pin!(response);
            assert!(futures_util::poll!(&mut response).is_pending());
            // Poll again only after both clocks expired: attribution follows
            // the earlier deadline, not select branch order or scheduler delay.
            tokio::time::advance(Duration::from_millis(200)).await;
            let response = response.await.unwrap();
            assert_eq!(response.status(), status);
            assert_eq!(gate.state.permits.available_permits(), 1);
            let logs = exporter.get_emitted_logs().unwrap();
            assert_eq!(logs.len(), 1);
            assert_eq!(logs[0].record.severity_number(), Some(severity));
            for (name, expected) in [
                ("gateway.error.status_code", AnyValue::from(status as i64)),
                ("error.type", AnyValue::from(cause)),
            ] {
                assert!(
                    logs[0]
                        .record
                        .attributes_iter()
                        .any(|(key, value)| key.as_str() == name && value == &expected)
                );
            }
            gate.shutdown();
        }
    }
}

#[tokio::test]
async fn concurrency_and_collection_timeout_fail_before_upstream() {
    let gate = gateway(Limits {
        concurrency: 1,
        body_timeout: Duration::from_millis(20),
        ..Limits::default()
    });
    let held = gate.state.permits.clone().acquire_owned().await.unwrap();
    let request = || {
        HttpRequest::builder()
            .header("content-type", "application/json")
            .body(axum::body::Body::from_stream(
                futures_util::stream::pending::<Result<bytes::Bytes, std::io::Error>>(),
            ))
            .unwrap()
    };
    assert_eq!(
        handle(gate.state.clone(), ClientProtocol::Chat, request())
            .await
            .unwrap_err()
            .status,
        429
    );
    drop(held);
    assert_eq!(
        tokio::time::timeout(
            Duration::from_secs(1),
            handle(gate.state.clone(), ClientProtocol::Chat, request())
        )
        .await
        .unwrap()
        .unwrap_err()
        .status,
        408
    );
    assert_eq!(gate.state.permits.available_permits(), 1);
    gate.shutdown();
    assert_eq!(
        handle(gate.state.clone(), ClientProtocol::Chat, request())
            .await
            .unwrap_err()
            .status,
        503
    );
}
#[test]
fn entries_cannot_escape_compiled_route_or_credential_ownership() {
    let topo = topology();
    let result = Gateway::new(
        topo,
        TaskEntries {
            generation: vec![Entry {
                chat_profile: Default::default(),
                model: "fixture-model".into(),
                protocol: crate::protocol::ClientProtocol::Chat,
                endpoint: EndpointId::new("limited-responses").unwrap(),
            }],
            ..Default::default()
        },
        BTreeMap::new(),
        std::collections::BTreeMap::from([(
            "fixture-agent".into(),
            SecretMaterial::new(KEY).unwrap(),
        )]),
        Limits::default(),
        None,
    );
    assert!(matches!(result, Err(StartupError::Binding)));
    let gate = gateway(Limits::default());
    let scope = gate.state.entries.values().next().unwrap().candidates[0]
        .cache_scope()
        .clone();
    let other = Gateway::new(
        topology(),
        TaskEntries {
            generation: vec![Entry {
                chat_profile: Default::default(),
                model: "fixture-model".into(),
                protocol: crate::protocol::ClientProtocol::Chat,
                endpoint: EndpointId::new("fixture-chat").unwrap(),
            }],
            ..Default::default()
        },
        BTreeMap::from([(
            CredentialBindingId::new("fixture-key").unwrap(),
            Arc::new(SecretMaterial::new("different-synthetic-upstream-key").unwrap()),
        )]),
        std::collections::BTreeMap::from([(
            "fixture-agent".into(),
            SecretMaterial::new(KEY).unwrap(),
        )]),
        Limits::default(),
        None,
    )
    .unwrap();
    assert_ne!(
        &scope,
        other.state.entries.values().next().unwrap().candidates[0].cache_scope()
    );
}
#[tokio::test]
async fn normal_shutdown_is_not_reclassified_as_an_admission_error_by_middleware() {
    use axum::{body::Body, http::Request};
    for during_body in [false, true] {
        let mut gate = gateway(Limits::default());
        let (sink, exporter) = crate::gateway::diagnostics::Sink::memory();
        Arc::get_mut(&mut gate.state).unwrap().diagnostics = Some(sink);
        let started = Arc::new(tokio::sync::Notify::new());
        let notification = started.clone();
        let body = if during_body {
            Body::from_stream(futures_util::stream::once(async move {
                notification.notify_one();
                std::future::pending::<Result<bytes::Bytes, std::io::Error>>().await
            }))
        } else {
            gate.shutdown();
            Body::empty()
        };
        let app = gate.router();
        let task = tokio::spawn(async move {
            app.oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/chat/completions")
                    .header("authorization", format!("Bearer {KEY}"))
                    .header("content-type", "application/json")
                    .body(body)
                    .unwrap(),
            )
            .await
            .unwrap()
        });
        if during_body {
            tokio::time::timeout(std::time::Duration::from_secs(2), started.notified())
                .await
                .unwrap();
            gate.shutdown();
        }
        let response = task.await.unwrap();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        axum::body::to_bytes(response.into_body(), 65536)
            .await
            .unwrap();
        assert!(exporter.get_emitted_logs().unwrap().is_empty());
    }
}

#[tokio::test]
async fn standalone_admission_logs_closed_reasons_and_selected_error_status_without_bodies() {
    use axum::{body::Body, http::Request};
    let mut gate = gateway(Limits::default());
    let (sink, exporter) = crate::gateway::diagnostics::Sink::memory();
    Arc::get_mut(&mut gate.state).unwrap().diagnostics = Some(sink);
    for (body, status) in [
        ("invalid JSON", 400),
        (
            r#"{"model":"missing","input":"synthetic-private-prompt"}"#,
            404,
        ),
    ] {
        let response = gate
            .router()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/embeddings")
                    .header("authorization", format!("Bearer {KEY}"))
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status().as_u16(), status);
        axum::body::to_bytes(response.into_body(), 65536)
            .await
            .unwrap();
    }
    use opentelemetry::logs::AnyValue;
    let logs = exporter.get_emitted_logs().unwrap();
    assert_eq!(logs.len(), 2);
    for (log, status) in logs.iter().zip([400, 404]) {
        assert!(
            log.record
                .attributes_iter()
                .any(|(key, value)| key.as_str() == "gateway.error.status_code"
                    && *value == AnyValue::Int(status))
        );
        assert!(
            log.record
                .attributes_iter()
                .any(|(key, value)| key.as_str() == "error.type"
                    && *value != AnyValue::from("gateway.http_rejected"))
        );
    }
    let text = format!("{logs:?}");
    assert!(!text.contains("synthetic-private-prompt") && !text.contains("invalid JSON"));
}
