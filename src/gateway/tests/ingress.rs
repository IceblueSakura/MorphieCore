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
    use crate::execution::{
        plan::{PlanError, RejectionReason},
        report::{Report, ResultState},
    };
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
    entry.public.contract.tools = false;
    state.entries.insert(
        (ClientProtocol::Responses, "fixture-model".into()),
        Arc::new(entry),
    );
    let wire = serde_json::json!({"model":"fixture-model",
        "input":[{"role":"assistant","content":"prior"}],
        "tools":[{"type":"function","name":"lookup","strict":false}]});
    let report = Report::default();
    assert_eq!(
        admission::prepare(
            state,
            ClientProtocol::Responses,
            &serde_json::to_vec(&wire).unwrap(),
            &report
        )
        .err()
        .unwrap()
        .status,
        400
    );
    let snapshot = report.snapshot();
    assert!(snapshot.intake.entries.is_empty());
    assert!(snapshot.attempts.is_empty());
    assert_eq!(
        snapshot.failure.unwrap().boundary,
        Some(crate::protocol::conversion::generation::ConversionBoundary::RequestProject)
    );
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
    assert!(failed_decode.snapshot().intake.entries.is_empty());
    let entry = Arc::make_mut(
        state
            .entries
            .get_mut(&(ClientProtocol::Responses, "fixture-model".into()))
            .unwrap(),
    );
    entry.public.contract.tools = true;
    for candidate in &mut entry.candidates {
        Arc::make_mut(candidate)
            .endpoint
            .execution
            .request_body_limit = 1;
    }
    let trace = crate::gateway::diagnostics::Trace::generation_trace(None);
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
    let snapshot = report.snapshot();
    assert_eq!(snapshot.result, ResultState::Failed);
    assert_eq!(snapshot.failure_code, Some(error.diagnostic_code()));
    assert_eq!(snapshot.failure_status_hint, Some(400));
    assert!(snapshot.attempts.is_empty());
    assert!(snapshot.intake.entries.is_empty());
    assert!(matches!(
        snapshot.planning_failure,
        Some(PlanError::CandidateRejected(RejectionReason::BodyLimit))
            | Some(PlanError::NoCandidate { .. })
    ));
    let encoded = serde_json::to_string(&report.snapshot()).unwrap();
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
            .eligible(&request)
            .unwrap()
            .iter()
            .map(|c| c.endpoint.id.as_str())
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
    // RequireAll still rejects controls unsupported by the Chat candidate.
    assert!(entry.eligible(&opaque).is_err());
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
        let candidates = entry.eligible(&request).unwrap();
        assert_eq!(candidates.len(), 1);
        let candidate = &candidates[0];
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
    assert!(
        entry
            .client
            .as_openai()
            .unwrap()
            .0
            .encode_request(
                &request,
                "synthetic",
                entry.candidates[0]
                    .endpoint
                    .representation
                    .openai()
                    .unwrap()
            )
            .is_err()
    );
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
    let directory = crate::test_files::private_directory();
    let path = directory.path().join("events.jsonl");
    let capture_path = directory.path().join("capture.jsonl");
    let config = serde_json::from_value(serde_json::json!({
        "path":capture_path,
        "client_request":{"headers":true,"body":true},
        "client_response":{"headers":true,"body":true}
    }))
    .unwrap();
    let gate = gateway(Limits::default())
        .with_diagnostics(&path)
        .unwrap()
        .with_capture(config)
        .unwrap();
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
    gate.flush_diagnostics().await;
    let text = std::fs::read_to_string(&path).unwrap();
    let records: Vec<serde_json::Value> = text
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .filter(|value: &serde_json::Value| value["event"] == "request")
        .collect();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["agent_id"], "fixture-agent");
    let request_id = records[0]["request_id"].as_str().unwrap();
    assert!(!request_id.is_empty());
    assert!(!text.contains(KEY));
    assert!(!text.contains("invalid json"));
    assert!(text.contains("admission"));
    let captured = std::fs::read_to_string(&capture_path).unwrap();
    assert!(!captured.contains(KEY));
    assert!(!captured.contains("synthetic-query-secret"));
    assert!(!captured.contains("invalid json"));
    assert!(captured.contains("synthetic-private-header"));
    assert!(captured.contains("client_response"));
    assert!(captured.lines().all(|line| {
        let row: serde_json::Value = serde_json::from_str(line).unwrap();
        row["exchange"] == 1
            && row["request_id"] == request_id
            && row["agent_id"] == records[0]["agent_id"]
    }));
}
#[tokio::test]
async fn authenticated_agent_is_fixed_before_optional_capture() {
    use crate::gateway::ingress::{
        auth::{Auth, AuthenticatedAgent},
        correlation::RequestId,
    };
    use axum::extract::Extension;
    const OTHER: &str = "synthetic-second-client-token-00002";
    for (diagnostics, capture) in [(false, false), (false, true), (true, false), (true, true)] {
        let directory = crate::test_files::private_directory();
        let mut gate = gateway(Limits::default());
        Arc::get_mut(&mut gate.state).unwrap().auth = Auth::new(BTreeMap::from([
            ("pi".into(), SecretMaterial::new(KEY).unwrap()),
            ("opencode".into(), SecretMaterial::new(OTHER).unwrap()),
        ]))
        .unwrap();
        if diagnostics {
            gate = gate
                .with_diagnostics(&directory.path().join("events.jsonl"))
                .unwrap();
        }
        if capture {
            gate = gate
                .with_capture(
                    serde_json::from_value(serde_json::json!({
                        "path":directory.path().join("capture.jsonl"),
                        "client_request":{"headers":true}
                    }))
                    .unwrap(),
                )
                .unwrap();
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
        gate.flush_diagnostics().await;
        if capture {
            let captured = std::fs::read_to_string(directory.path().join("capture.jsonl")).unwrap();
            for row in captured
                .lines()
                .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
            {
                assert!(references.contains(row["request_id"].as_str().unwrap()));
                assert!(matches!(row["agent_id"].as_str(), Some("pi" | "opencode")));
            }
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
    let directory = crate::test_files::private_directory();
    let disabled = directory.path().join("disabled.jsonl");
    let config = serde_json::from_value(serde_json::json!({"path":disabled})).unwrap();
    let gate = gate.with_capture(config).unwrap();
    assert!(gate.capture_status().is_none());
    assert!(!disabled.exists());
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
async fn messages_version_is_single_current_and_checked_before_body_poll() {
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
        for (versions, beta) in [
            (vec![], None),
            (vec![""], None),
            (vec!["2023-06-01"], None),
            (vec!["2026-10-22"], None),
            (vec!["invalid"], None),
            (vec!["2026-10-21, 2026-10-21"], None),
            (vec!["2026-10-21", "2026-10-21"], None),
            (
                vec!["2026-10-21"],
                Some("fine-grained-tool-streaming-2025-05-14"),
            ),
        ] {
            let mut request = HttpRequest::builder()
                .method("POST")
                .uri(path)
                .header("x-api-key", KEY)
                .header("content-type", "application/json");
            for version in versions {
                request = request.header("anthropic-version", version);
            }
            if let Some(beta) = beta {
                request = request.header("anthropic-beta", beta);
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
        // An empty beta list selects no features and reaches model resolution.
        let response = gate.router().oneshot(
            HttpRequest::builder().method("POST").uri(path)
                .header("x-api-key", KEY).header("content-type", "application/json")
                .header("anthropic-version", "2026-10-21")
                .header("anthropic-beta", "")
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
        .cache_scope
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
        scope,
        other.state.entries.values().next().unwrap().candidates[0].cache_scope
    );
    // Bind the entire authorized pool even when both runtime ceilings are
    // smaller. The expanded-entry bound, not a pool validator, rejects #65.
    let directory = crate::test_files::private_directory();
    let manager = crate::credential::CredentialManager::new(directory.path(), vec![]).unwrap();
    let mut members = Vec::new();
    for index in 0..64 {
        let alias = format!("key-{index}");
        manager
            .add_api_key(
                "fixture",
                &alias,
                crate::credential::Secret::new(format!("synthetic-key-{index}")).unwrap(),
            )
            .unwrap();
        members.push(crate::credential::CredentialRef::ApiKey { alias });
    }
    for endpoints in [
        vec!["fixture-chat"],
        vec!["fixture-chat", "fixture-responses"],
    ] {
        let mut credentials = Credentials::new();
        credentials
            .insert_pool(
                CredentialBindingId::new("fixture-key").unwrap(),
                manager
                    .bind_pool(
                        "fixture",
                        &crate::credential::CredentialPool {
                            members: members.clone(),
                            fallback: false,
                            max_attempts: 1,
                        },
                    )
                    .unwrap(),
            )
            .unwrap();
        let bound = Gateway::new(
            topology(),
            TaskEntries {
                generation: endpoints
                    .iter()
                    .map(|id| Entry {
                        chat_profile: Default::default(),
                        model: "fixture-model".into(),
                        protocol: crate::protocol::ClientProtocol::Chat,
                        endpoint: EndpointId::new(id).unwrap(),
                    })
                    .collect(),
                ..Default::default()
            },
            credentials,
            std::collections::BTreeMap::from([(
                "fixture-agent".into(),
                SecretMaterial::new(KEY).unwrap(),
            )]),
            Limits::default(),
            None,
        );
        if endpoints.len() == 1 {
            let bound = bound.unwrap();
            let candidates = &bound.state.entries[&(
                crate::protocol::ClientProtocol::Chat,
                "fixture-model".into(),
            )]
                .candidates;
            assert_eq!(candidates.len(), 64);
            assert!(
                candidates
                    .iter()
                    .all(|candidate| candidate.credential_attempts == 1)
            );
        } else {
            assert!(matches!(bound, Err(StartupError::Binding)));
        }
    }
}
