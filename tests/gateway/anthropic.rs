//! Independent HTTP oracle for standard clients over a native Messages endpoint.
use super::Guard;
use crate::messages_stream::messages_stream_body;
use crate::messages_wire::{native_response, native_stream_body};
use axum::response::{IntoResponse, Response};
use axum::{Json, Router, extract::State, http::HeaderMap, routing::post};
use morphiecore::{
    gateway::{Entry, Gateway, Limits},
    protocol::{openai::Profile, replay_envelope},
    provider::{CredentialBindingId, SecretMaterial, TrustedOrigin},
    topology::{self, ModelMetadata, catalog::MESSAGES_BINDINGS},
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::net::TcpListener;

async fn client_body(response: reqwest::Response, profile: Profile, stream: bool) -> Value {
    if !stream || response.status() != 200 {
        return response.json().await.unwrap();
    }
    let wire = response.text().await.unwrap();
    stream_body(&wire, profile)
}
fn stream_body(wire: &str, profile: Profile) -> Value {
    assert!(!wire.contains("synthetic-native-signature") && !wire.contains("synthetic-redacted"));
    let client = morphiecore::protocol::openai::Adapter::new(
        profile,
        morphiecore::protocol::openai::Dialect::Standard,
        Some(morphiecore::execution::replay::CLIENT_ORIGIN.clone()),
    );
    let mut decoder = client.event_decoder();
    for data in wire.lines().filter_map(|line| line.strip_prefix("data: ")) {
        if data == "[DONE]" {
            decoder.done().unwrap();
        } else {
            decoder.push(&serde_json::from_str(data).unwrap()).unwrap();
        }
    }
    decoder.finish().unwrap();
    client
        .encode_response(
            &decoder.materialize().unwrap(),
            &morphiecore::protocol::openai::TargetCaps {
                replay_origin: Some(morphiecore::execution::replay::CLIENT_ORIGIN.clone()),
                ..morphiecore::protocol::openai::TargetCaps::full()
            },
        )
        .unwrap()
}

#[derive(Default)]
struct Control {
    release: tokio::sync::Notify,
    dropped: tokio::sync::Notify,
}
struct DropSignal(Arc<Control>);
type FixtureState = (Arc<Mutex<Vec<Value>>>, Arc<Control>);
impl Drop for DropSignal {
    fn drop(&mut self) {
        self.0.dropped.notify_one();
    }
}
fn controlled_response(mode: &str, control: Arc<Control>) -> Response {
    let message = json!({"id":"held","type":"message","role":"assistant","model":"claude-haiku-5-5",
        "content":[{"type":"text","text":"early"},
            {"type":"tool_use","id":"held-call","name":"lookup","input":{"key":"A"}}],
        "stop_reason":"tool_use",
        "usage":{"input_tokens":1,"output_tokens":1}});
    let bytes = native_stream_body(&message);
    let prefix = if mode == "hold" {
        bytes
    } else {
        bytes
            .rsplit_once("event: content_block_stop")
            .unwrap()
            .0
            .to_owned()
    };
    let cut = mode == "cut";
    let stream = futures_util::stream::unfold(
        (0, DropSignal(control), prefix, cut),
        |(phase, guard, prefix, cut)| async move {
            match phase {
                0 => Some((
                    Ok::<_, std::io::Error>(bytes::Bytes::from(prefix.clone())),
                    (1, guard, prefix, cut),
                )),
                1 => {
                    guard.0.release.notified().await;
                    if cut {
                        Some((
                            Err(std::io::Error::other("synthetic disconnect")),
                            (2, guard, prefix, cut),
                        ))
                    } else {
                        None
                    }
                }
                _ => None,
            }
        },
    );
    Response::builder()
        .header("content-type", "text/event-stream")
        .body(axum::body::Body::from_stream(stream))
        .unwrap()
}

async fn answer(
    State((seen, control)): State<FixtureState>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    assert_eq!(headers["anthropic-version"], "2026-10-21");
    assert!(matches!(
        headers["x-api-key"].to_str().unwrap(),
        "synthetic-upstream" | "synthetic-secondary"
    ));
    assert!(!headers.contains_key("authorization"));
    assert!(headers.contains_key("x-opencode-session"));
    assert!(!headers.contains_key("x-never-forward"));
    assert_eq!(body["model"], "claude-haiku-5-5");
    assert_eq!(body["max_tokens"], 64);
    assert_eq!(
        body["tools"][0]["input_schema"],
        json!({"type":"object","properties":{"key":{"type":"string"}}})
    );
    assert_eq!(body["tools"][0]["strict"], false);
    let explicit_cache = body["tools"][0].get("cache_control").is_some();
    if explicit_cache {
        assert!(body.get("cache_control").is_none());
        assert_eq!(
            body["tools"][0]["cache_control"],
            json!({"type":"ephemeral","ttl":"1h"})
        );
        assert_eq!(
            body["system"],
            json!([
                {"type":"text","text":"Be brief.","cache_control":{"type":"ephemeral","ttl":"1h"}},
                {"type":"text","text":"Use lookup.","cache_control":{"type":"ephemeral"}}
            ])
        );
        assert_eq!(
            body["messages"].as_array().unwrap().last().unwrap()["content"][0]["cache_control"],
            json!({"type":"ephemeral"})
        );
    } else {
        assert_eq!(body["cache_control"], json!({"type":"ephemeral"}));
    }
    assert!(body.get("prompt_cache_key").is_none() && body.get("prompt_cache_retention").is_none());
    seen.lock().unwrap().push(body.clone());
    let prompt = body["messages"][0]["content"][0]["text"].as_str().unwrap();
    if matches!(prompt, "hold" | "cut" | "cancel") {
        assert_eq!(body["stream"], true);
        return controlled_response(prompt, control);
    }
    if prompt == "overflow" {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            Json(
                json!({"type":"error","error":{"type":"invalid_request_error",
                "message":"prompt is too long: 999999 tokens > 200000 maximum"}}),
            ),
        )
            .into_response();
    }
    if matches!(prompt, "overloaded" | "overloaded-early") {
        let overloaded =
            json!({"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}});
        if body["stream"] != true {
            return (
                axum::http::StatusCode::from_u16(529).unwrap(),
                Json(overloaded),
            )
                .into_response();
        }
        // Anthropic reports overload inside a started stream.
        let message = json!({"id":"msg-overloaded","type":"message","role":"assistant","model":"claude-haiku-5-5",
            "content":[{"type":"text","text":"partial"}],"stop_reason":"end_turn",
            "usage":{"input_tokens":1,"output_tokens":1}});
        let prefix = native_stream_body(&message)
            .split_once(if prompt == "overloaded" {
                "event: content_block_stop"
            } else {
                // Overload before any content: only `message_start` precedes it.
                "event: content_block_start"
            })
            .unwrap()
            .0
            .to_owned();
        return (
            [(axum::http::header::CONTENT_TYPE, "text/event-stream")],
            format!("{prefix}event: error\ndata: {overloaded}\n\n"),
        )
            .into_response();
    }
    if prompt == "upstream-error" {
        return (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({"error":"synthetic-private-body"})),
        )
            .into_response();
    }
    if prompt != "lookup" {
        let mut response = json!({"id":"msg-case","type":"message","role":"assistant","model":"claude-haiku-5-5",
            "content":[{"type":"text","text":"partial"}],"stop_reason":"end_turn",
            "usage":{"input_tokens":11,"output_tokens":7}});
        match prompt {
            "truncated" => response["stop_reason"] = json!("max_tokens"),
            "refusal" => {
                response["stop_reason"] = json!("refusal");
                // This lane tests explanation-only cross-protocol delivery.
                // A reported policy category has no selected OpenAI carrier.
                response["stop_details"] =
                    json!({"type":"refusal","category":null,"explanation":"synthetic"});
            }
            "malformed" => response["usage"]["input_tokens"] = json!("invalid"),
            "bad-signature" => {
                response["content"] =
                    json!([{"type":"thinking","thinking":"private","signature":7}])
            }
            _ => panic!("unexpected synthetic prompt"),
        }
        return native_response(response, body["stream"] == true);
    }
    assert_eq!(
        body["tool_choice"],
        json!({"type":"auto","disable_parallel_tool_use":true})
    );
    let turn = body["messages"].as_array().unwrap().len() > 1;
    if turn {
        let mut expected = json!([
            {"role":"user","content":[{"type":"text","text":"lookup"}]},
            {"role":"assistant","content":[
                {"type":"thinking","thinking":"plan","signature":"synthetic-native-signature"},
                {"type":"redacted_thinking","data":"synthetic-redacted"},
                {"type":"tool_use","id":"call-local","name":"lookup","input":{"key":"A"}}
            ]},
            {"role":"user","content":[{"type":"tool_result","tool_use_id":"call-local","content":"found"}]}
        ]);
        if explicit_cache {
            expected[2]["content"][0]["cache_control"] = json!({"type":"ephemeral"});
        }
        assert_eq!(body["messages"], expected);
    }
    native_response(
        json!({"id":"msg-local","type":"message","role":"assistant","model":"claude-haiku-5-5",
            "content": if turn { json!([{"type":"text","text":"answer"}]) } else { json!([
                {"type":"thinking","thinking":"plan","signature":"synthetic-native-signature"},
                {"type":"redacted_thinking","data":"synthetic-redacted"},
                {"type":"tool_use","id":"call-local","name":"lookup","input":{"key":"A"}}
            ]) },
            "stop_reason":if turn {"end_turn"}else{"tool_use"},"stop_sequence":null,
            "usage":{"input_tokens":11,"cache_read_input_tokens":3,"cache_creation_input_tokens":2,"output_tokens":7},
            "future_report":{"ignored":true}
        }),
        body["stream"] == true,
    )
}

struct NativeFixture {
    base: String,
    seen: Arc<Mutex<Vec<Value>>>,
    control: Arc<Control>,
    replay_origin: morphiecore::semantic::value::ReplayOrigin,
    gateway: Gateway,
    diagnostics: std::path::PathBuf,
    client: reqwest::Client,
    _gateway_task: Guard,
    _upstream: Guard,
    _directory: tempfile::TempDir,
}

async fn native_fixture() -> NativeFixture {
    let seen = Arc::new(Mutex::new(vec![]));
    let control = Arc::new(Control::default());
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let app = Router::new()
        .route("/zen/go/v1/messages", post(answer))
        .with_state((seen.clone(), control.clone()));
    let upstream = Guard(tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap()
    }));
    let binding = &MESSAGES_BINDINGS[0];
    let mut provider = binding.provider();
    provider.origin = TrustedOrigin::parse(&origin).unwrap();
    let mut endpoint = binding.endpoint();
    endpoint.target.origin = provider.origin.clone();
    let endpoint_id = endpoint.id.clone();
    let mut secondary = endpoint.clone();
    secondary.id = topology::EndpointId::new("native-secondary").unwrap();
    // Independent credential targets may fail over; a duplicate alias of the
    // same credential/deployment now intentionally shares its health.
    secondary.credential = CredentialBindingId::new("native-secondary-key").unwrap();
    let endpoint_ids = vec![endpoint_id.clone(), secondary.id.clone()];
    let model = endpoint.canonical_model.clone();
    let route = topology::Route {
        id: topology::RouteId::new("native").unwrap(),
        task: topology::TaskKind::Generation,
        endpoints: endpoint_ids.clone(),
        policy: topology::RoutePolicy {
            fallback: topology::FallbackPolicy::BeforeCommit,
            max_attempts: 2,
            ..Default::default()
        },
    };
    let contract = binding.canonical_model().contract;
    let mut public = topology::PublicModel::new(
        model.clone(),
        model.clone(),
        topology::TaskKind::Generation,
        route.id.clone(),
        contract.clone(),
    );
    public.standard_context = false;
    let replay_origin = endpoint.replay_origin();
    let topology = topology::compile(
        vec![provider],
        vec![endpoint, secondary],
        vec![route],
        vec![public],
        vec![topology::CanonicalModel {
            id: model.clone(),
            task: topology::TaskKind::Generation,
            contract,
        }],
    )
    .unwrap()
    .with_model_metadata([(model, ModelMetadata::new(1, "Synthetic").unwrap())])
    .unwrap();
    let directory = tempfile::tempdir().unwrap();
    let diagnostics = directory.path().join("diagnostics.jsonl");
    let gate = Gateway::new(
        topology,
        morphiecore::gateway::TaskEntries {
            generation: morphiecore::adapter::ClientProtocol::ALL
                .into_iter()
                .flat_map(|protocol| {
                    endpoint_ids.iter().map(move |endpoint| Entry {
                        chat_profile: Default::default(),
                        model: binding.model.into(),
                        protocol,
                        endpoint: endpoint.clone(),
                    })
                })
                .collect(),
            ..Default::default()
        },
        BTreeMap::from([
            (
                binding.credential(),
                Arc::new(SecretMaterial::new("synthetic-upstream").unwrap()),
            ),
            (
                CredentialBindingId::new("native-secondary-key").unwrap(),
                Arc::new(SecretMaterial::new("synthetic-secondary").unwrap()),
            ),
        ]),
        std::collections::BTreeMap::from([(
            "fixture-agent".into(),
            SecretMaterial::new("synthetic-client-identity-00000001").unwrap(),
        )]),
        Limits::default(),
        None,
    )
    .unwrap()
    .with_diagnostics(&diagnostics)
    .unwrap();
    let diagnostic_owner = gate.clone();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let gateway_task = Guard(tokio::spawn(async move {
        gate.serve(listener, std::future::pending()).await.unwrap()
    }));
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(3))
        .build()
        .unwrap();

    NativeFixture {
        base,
        seen,
        control,
        replay_origin,
        gateway: diagnostic_owner,
        diagnostics,
        client,
        _gateway_task: gateway_task,
        _upstream: upstream,
        _directory: directory,
    }
}

#[tokio::test]
async fn json_and_stream_clients_replay_native_thinking_and_tools() {
    let fixture = native_fixture().await;
    let base = &fixture.base;
    let seen = &fixture.seen;
    let replay_origin = fixture.replay_origin.clone();
    let diagnostic_owner = &fixture.gateway;
    let diagnostics = &fixture.diagnostics;
    let client = &fixture.client;
    let binding = &MESSAGES_BINDINGS[0];
    for profile in [Profile::Responses, Profile::Chat] {
        for stream in [false, true] {
            let responses = profile == Profile::Responses;
            let path = if responses {
                "/v1/responses"
            } else {
                "/v1/chat/completions"
            };
            let function = json!({"name":"lookup","parameters":{"type":"object","properties":{"key":{"type":"string"}}},"strict":false});
            let mut input = if responses {
                let mut tool = function.clone();
                tool["type"] = json!("function");
                json!({"model":binding.model,"max_output_tokens":64,"store":false,"background":false,
                "previous_response_id":null,"include":["reasoning.encrypted_content"],"temperature":0.375,
                "input":[{"role":"user","content":"lookup"}],"tools":[tool]})
            } else {
                json!({"model":binding.model,"max_completion_tokens":64,"messages":[{"role":"user","content":"lookup"}],
                "tools":[{"type":"function","function":function}]})
            };
            input["stream"] = json!(stream);
            // Cache grouping is advisory; native targets cannot erase retention intent.
            input["prompt_cache_key"] = json!("pi-session");
            if responses && !stream {
                input["prompt_cache_retention"] = json!("24h");
                let count = seen.lock().unwrap().len();
                let rejected = client
                    .post(format!("{base}{path}"))
                    .bearer_auth("synthetic-client-identity-00000001")
                    .json(&input)
                    .send()
                    .await
                    .unwrap();
                assert_eq!(rejected.status(), 400);
                let error: Value = rejected.json().await.unwrap();
                assert_eq!(error["error"]["code"], "invalid_request");
                assert_eq!(seen.lock().unwrap().len(), count);
                input
                    .as_object_mut()
                    .unwrap()
                    .remove("prompt_cache_retention");
            }
            // Codex always sends `client_metadata`, which no target takes.
            input["client_metadata"] = json!({"session_id":"synthetic-session"});
            // Without `tool_choice`, the switch rides on Messages' default `auto`.
            input["parallel_tool_calls"] = json!(false);
            if stream && !responses {
                input["stream_options"] = json!({"include_usage":true,"include_obfuscation":false});
            }
            let count_before = seen.lock().unwrap().len();
            let result = client
                .post(format!("{base}{path}"))
                .bearer_auth("synthetic-client-identity-00000001")
                .header("x-never-forward", "private")
                .json(&input)
                .send()
                .await
                .unwrap();
            if !responses {
                let mut result = result;
                let published = stream && result.status() == 200;
                if published {
                    let mut bytes = vec![];
                    let aborted = loop {
                        match result.chunk().await {
                            Ok(Some(chunk)) => bytes.extend_from_slice(&chunk),
                            Ok(None) => break false,
                            Err(_) => break true,
                        }
                    };
                    let wire = String::from_utf8(bytes).unwrap();
                    assert!(aborted || wire.contains("\"error\""));
                    assert!(!wire.contains("[DONE]"));
                    assert!(!wire.contains("\"finish_reason\":\""));
                    assert!(!wire.contains("reasoning_details"));
                    assert!(!wire.contains("synthetic-native-signature"));
                } else {
                    assert_eq!(result.status(), 502);
                    let error: Value = result.json().await.unwrap();
                    assert_eq!(error["error"]["code"], "upstream_error");
                }
                let attempts = seen.lock().unwrap().len() - count_before;
                if published {
                    assert_eq!(attempts, 1, "published failures must not replay");
                } else {
                    assert!((1..=2).contains(&attempts));
                }
                tokio::time::pause();
                tokio::time::advance(Duration::from_secs(60)).await;
                tokio::time::resume();
            } else {
                assert_eq!(seen.lock().unwrap().len(), count_before + 1);
                let status = result.status();
                let body: Value = client_body(result, profile, stream).await;
                if status != 200 {
                    diagnostic_owner.flush_diagnostics().await;
                    panic!(
                        "{profile:?}: {body}; {}",
                        std::fs::read_to_string(diagnostics).unwrap()
                    );
                }
                assert_eq!(body["model"], binding.model);
                assert!(body["created_at"].as_u64().is_some());
                assert_eq!(body["usage"]["input_tokens"], 16);
                assert_eq!(body["usage"]["total_tokens"], 23);
                assert!(!body.to_string().contains("synthetic-native-signature"));
                assert!(!body.to_string().contains("synthetic-redacted"));
                let token = &body["output"][0]["encrypted_content"];
                let (issuer, format, payload) =
                    replay_envelope::open(token.as_str().unwrap()).unwrap();
                assert_eq!(issuer, replay_origin);
                assert_eq!(
                format,
                morphiecore::semantic::task::generation::ReplayFormat::AnthropicMessagesThinking
            );
                assert_eq!(payload.as_str(), "synthetic-native-signature");
                let redacted = &body["output"][1]["encrypted_content"];
                let (_, format, payload) =
                    replay_envelope::open(redacted.as_str().unwrap()).unwrap();
                assert_eq!(format,morphiecore::semantic::task::generation::ReplayFormat::AnthropicMessagesRedactedThinking);
                assert_eq!(payload.as_str(), "synthetic-redacted");
            }
            if stream {
                continue;
            }
            input["stream"] = json!(false);
            for (prompt, expected) in [
                ("upstream-error", 503),
                ("malformed", 502),
                ("bad-signature", 502),
                ("truncated", 200),
                ("refusal", 200),
            ] {
                // Each negative codec case must reach its own upstream failure,
                // not be rejected by the preceding case's cooldown.
                tokio::time::pause();
                tokio::time::advance(Duration::from_secs(60)).await;
                tokio::time::resume();
                input[if responses { "input" } else { "messages" }] =
                    json!([{"role":"user","content":prompt}]);
                let count = seen.lock().unwrap().len();
                let result = client
                    .post(format!("{base}{path}"))
                    .bearer_auth("synthetic-client-identity-00000001")
                    .json(&input)
                    .send()
                    .await
                    .unwrap();
                let status = result.status();
                let body: Value = result.json().await.unwrap();
                assert_eq!(status, expected, "{profile:?} {prompt}: {body}");
                assert_eq!(
                    seen.lock().unwrap().len(),
                    // An unusable reply fails over like a server error.
                    count
                        + if matches!(prompt, "upstream-error" | "malformed" | "bad-signature") {
                            2
                        } else {
                            1
                        }
                );
                assert!(!body.to_string().contains("synthetic-private"));
                if expected == 200 {
                    assert_eq!(
                        body["usage"][if responses {
                            "input_tokens"
                        } else {
                            "prompt_tokens"
                        }],
                        11
                    );
                    assert_eq!(
                        body["usage"][if responses {
                            "output_tokens"
                        } else {
                            "completion_tokens"
                        }],
                        7
                    );
                    assert_eq!(body["usage"]["total_tokens"], 18);
                    let (reason, finish) = if prompt == "truncated" {
                        (json!("max_output_tokens"), "length")
                    } else {
                        (json!("content_filter"), "content_filter")
                    };
                    if responses {
                        assert_eq!(body["status"], "incomplete");
                        assert_eq!(body["incomplete_details"]["reason"], reason);
                    } else {
                        assert_eq!(body["choices"][0]["finish_reason"], finish);
                    }
                }
            }
        }
    }
    // A Messages client keeps four explicit owners across a tool round with thinking replay.
    for stream in [false, true] {
        let mut input = json!({"model":binding.model,"max_tokens":64,"stream":stream,
        "thinking":{"type":"adaptive"},
        "metadata":{"user_id":"synthetic-user"},
        "system":[
            {"type":"text","text":"Be brief.","cache_control":{"type":"ephemeral","ttl":"1h"}},
            {"type":"text","text":"Use lookup.","cache_control":{"type":"ephemeral"}}
        ],
        "tools":[{"name":"lookup","strict":false,"cache_control":{"type":"ephemeral","ttl":"1h"},
            "input_schema":{"type":"object","properties":{"key":{"type":"string"}}}}],
        "tool_choice":{"type":"auto","disable_parallel_tool_use":true},
        "messages":[{"role":"user","content":[{"type":"text","text":"lookup",
            "cache_control":{"type":"ephemeral"}}]}]});
        for turn in 0..2 {
            let result = client
                .post(format!("{base}/v1/messages"))
                .header("x-api-key", "synthetic-client-identity-00000001")
                .header("anthropic-version", "2026-10-21")
                .json(&input)
                .send()
                .await
                .unwrap();
            let status = result.status();
            assert_eq!(result.headers()["x-morphiecore-adaptation"], "possible");
            assert_eq!(
                result.headers()["link"],
                "</v1/morphiecore/capabilities>; rel=\"describedby\""
            );
            let body = if stream {
                messages_stream_body(&result.text().await.unwrap())
            } else {
                result.json().await.unwrap()
            };
            assert_eq!(status, 200, "Messages turn {turn}: {body}");
            assert_eq!(body["type"], "message");
            assert_eq!(body["role"], "assistant");
            assert_eq!(body["model"], binding.model);
            assert_eq!(
                body["usage"],
                json!({"input_tokens":11,"cache_read_input_tokens":3,"cache_creation_input_tokens":2,"output_tokens":7})
            );
            if turn == 1 {
                assert_eq!(body["content"], json!([{"type":"text","text":"answer"}]));
                assert_eq!(body["stop_reason"], "end_turn");
                break;
            }
            assert_eq!(body["stop_reason"], "tool_use");
            assert_eq!(body["content"][0]["type"], "thinking");
            assert_eq!(body["content"][0]["thinking"], "plan");
            let (_, format, payload) =
                replay_envelope::open(body["content"][0]["signature"].as_str().unwrap()).unwrap();
            assert_eq!(
                format,
                morphiecore::semantic::task::generation::ReplayFormat::AnthropicMessagesThinking
            );
            assert_eq!(payload.as_str(), "synthetic-native-signature");
            assert_eq!(body["content"][1]["type"], "redacted_thinking");
            let (_, format, payload) =
                replay_envelope::open(body["content"][1]["data"].as_str().unwrap()).unwrap();
            assert_eq!(
                format,
                morphiecore::semantic::task::generation::ReplayFormat::AnthropicMessagesRedactedThinking
            );
            assert_eq!(payload.as_str(), "synthetic-redacted");
            assert_eq!(
                body["content"][2],
                json!({"type":"tool_use","id":"call-local","name":"lookup","input":{"key":"A"}})
            );
            let history = input["messages"].as_array_mut().unwrap();
            history[0]["content"][0]
                .as_object_mut()
                .unwrap()
                .remove("cache_control");
            history.push(json!({"role":"assistant","content":body["content"]}));
            history.push(json!({"role":"user","content":[
            {"type":"tool_result","tool_use_id":"call-local","content":"found",
                "cache_control":{"type":"ephemeral"}}]}));
        }
    }
    assert_eq!(seen.lock().unwrap().len(), 24);
    // Error classes in Anthropic's shapes: an HTTP envelope, or an `error` event
    // after content.
    for (prompt, stream) in [("overflow", false), ("overloaded", true)] {
        tokio::time::pause();
        tokio::time::advance(Duration::from_secs(60)).await;
        tokio::time::resume();
        let result = client
            .post(format!("{base}/v1/messages"))
            .header("x-api-key", "synthetic-client-identity-00000001")
            .header("anthropic-version", "2026-10-21")
            .json(&json!({"model":binding.model,"max_tokens":64,"stream":stream,
                "tools":[{"name":"lookup","strict":false,"input_schema":{"type":"object","properties":{"key":{"type":"string"}}}}],
                "tool_choice":{"type":"auto","disable_parallel_tool_use":true},
                "messages":[{"role":"user","content":prompt}]}))
            .send()
            .await
            .unwrap();
        let status = result.status().as_u16();
        let mut wire = vec![];
        let mut result = result;
        while let Ok(Some(bytes)) = result.chunk().await {
            wire.extend_from_slice(&bytes);
        }
        let wire = String::from_utf8(wire).unwrap();
        assert!(!wire.contains("999999"));
        if stream {
            assert_eq!(status, 200);
            assert!(wire.contains("partial") && !wire.contains("message_stop"));
            let mut last: Value = serde_json::from_str(
                wire.lines()
                    .filter_map(|l| l.strip_prefix("data: "))
                    .next_back()
                    .unwrap(),
            )
            .unwrap();
            last.as_object_mut().unwrap().remove("morphiecore");
            assert_eq!(
                last,
                json!({"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}})
            );
        } else {
            assert_eq!(status, 400);
            let body: Value = serde_json::from_str(&wire).unwrap();
            assert_eq!(body["type"], "error");
            assert!(
                body["error"]["message"]
                    .as_str()
                    .unwrap()
                    .starts_with("prompt is too long")
            );
        }
    }
    assert_eq!(seen.lock().unwrap().len(), 26);
    // Standard error events preserve classes without inventing a failed Response.
    for profile in [Profile::Responses, Profile::Chat] {
        let responses = profile == Profile::Responses;
        for stream in [false, true] {
            for prompt in ["overflow", "overloaded", "overloaded-early"] {
                tokio::time::pause();
                tokio::time::advance(Duration::from_secs(60)).await;
                tokio::time::resume();
                let function = json!({"name":"lookup","parameters":{"type":"object","properties":{"key":{"type":"string"}}},"strict":false});
                let input = if responses {
                    let mut tool = function.clone();
                    tool["type"] = json!("function");
                    json!({"model":binding.model,"max_output_tokens":64,"input":prompt,"stream":stream,"tools":[tool]})
                } else {
                    json!({"model":binding.model,"max_completion_tokens":64,"stream":stream,
                        "messages":[{"role":"user","content":prompt}],
                        "tools":[{"type":"function","function":function}]})
                };
                let count = seen.lock().unwrap().len();
                let mut response = client
                    .post(format!(
                        "{base}/v1/{}",
                        if responses {
                            "responses"
                        } else {
                            "chat/completions"
                        }
                    ))
                    .bearer_auth("synthetic-client-identity-00000001")
                    .json(&input)
                    .send()
                    .await
                    .unwrap();
                let status = response.status().as_u16();
                let mut wire = vec![];
                while let Ok(Some(bytes)) = response.chunk().await {
                    wire.extend_from_slice(&bytes);
                }
                let wire = String::from_utf8(wire).unwrap();
                assert!(!wire.contains("999999") && !wire.contains("Overloaded"));
                let (code, sse) = if prompt == "overflow" {
                    ("context_length_exceeded", stream && responses)
                } else {
                    ("server_is_overloaded", stream)
                };
                // A stream that fails before content is held, so it still fails over.
                let calls = match prompt {
                    "overflow" => 1,
                    "overloaded" if stream => 1,
                    _ => 2,
                };
                assert_eq!(
                    seen.lock().unwrap().len(),
                    count + calls,
                    "{profile:?} {prompt}"
                );
                if !sse {
                    let body: Value = serde_json::from_str(&wire).unwrap();
                    assert_eq!(status, if prompt == "overflow" { 400 } else { 503 });
                    assert_eq!(body["error"]["code"], code);
                    continue;
                }
                assert_eq!(status, 200, "{profile:?} {prompt}");
                assert!(!wire.contains("[DONE]") && !wire.contains("response.completed"));
                let standard: Vec<Value> = wire
                    .lines()
                    .filter_map(|line| line.strip_prefix("data: "))
                    .filter_map(|line| serde_json::from_str::<Value>(line).ok())
                    .map(|mut packet| {
                        if let Some(root) = packet.as_object_mut() {
                            root.remove("morphiecore");
                        }
                        if let Some(response) =
                            packet.get_mut("response").and_then(Value::as_object_mut)
                        {
                            response.remove("morphiecore");
                        }
                        packet
                    })
                    .collect();
                assert_eq!(
                    serde_json::to_string(&standard)
                        .unwrap()
                        .contains("partial"),
                    prompt == "overloaded"
                );
                let last: Value = serde_json::from_str(
                    wire.lines()
                        .filter_map(|l| l.strip_prefix("data: "))
                        .next_back()
                        .unwrap(),
                )
                .unwrap();
                if responses {
                    assert_eq!(last["type"], "error");
                    assert_eq!(last["code"], code);
                    assert!(last["sequence_number"].is_u64());
                    assert!(last.get("response").is_none());
                } else {
                    assert_eq!(last["error"]["code"], code);
                }
            }
        }
    }
    assert_eq!(seen.lock().unwrap().len(), 44);
    diagnostic_owner.flush_diagnostics().await;
    let records = std::fs::read_to_string(diagnostics).unwrap();
    let reports: Vec<Value> = records
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert!(reports.iter().any(|record| {
        record["generation"]["attempts"]
            .as_array()
            .is_some_and(|attempts| {
                attempts.iter().any(|attempt| {
                    attempt["evidence"]["entries"]
                        .as_array()
                        .is_some_and(|entries| {
                            [
                                "cache.omit-key.v1",
                                "messages.omit-sampling.v1",
                                "messages.default-automatic-cache.v1",
                            ]
                            .iter()
                            .all(|rule| {
                                entries.iter().any(|entry| {
                                    entry["view"] == "upstream_request" && entry["rule"] == *rule
                                })
                            })
                        })
                })
            })
    }));
    assert!(records.contains(r#""ignored_request_field_count":1"#));
    assert!(!records.contains("client_metadata"));
    // Native adaptive intent stays adaptive, without a numeric-budget approximation.
    assert!(!records.contains("messages.budget-as-effort.v1"));
    assert!(!records.contains("0.375"));
}

#[tokio::test]
async fn native_tool_publication_waits_for_eof_and_releases_upstream_without_replay() {
    let fixture = native_fixture().await;
    let base = &fixture.base;
    let seen = &fixture.seen;
    let control = &fixture.control;
    let client = &fixture.client;
    let binding = &MESSAGES_BINDINGS[0];
    for profile in [Some(Profile::Responses), Some(Profile::Chat), None] {
        let responses = profile == Some(Profile::Responses);
        let path = if profile.is_none() {
            "/v1/messages"
        } else if responses {
            "/v1/responses"
        } else {
            "/v1/chat/completions"
        };
        for mode in ["hold", "cut", "cancel"] {
            tokio::time::pause();
            tokio::time::advance(Duration::from_secs(60)).await;
            tokio::time::resume();
            let function = json!({"name":"lookup","parameters":{"type":"object","properties":{"key":{"type":"string"}}},"strict":false});
            let input = if profile.is_none() {
                json!({"model":binding.model,"max_tokens":64,"stream":true,
                    "tools":[{"name":"lookup","strict":false,
                        "input_schema":{"type":"object","properties":{"key":{"type":"string"}}}}],
                    "messages":[{"role":"user","content":mode}]})
            } else if responses {
                let mut tool = function.clone();
                tool["type"] = json!("function");
                json!({"model":binding.model,"max_output_tokens":64,"input":mode,"store":false,"stream":true,"tools":[tool]})
            } else {
                json!({"model":binding.model,"max_completion_tokens":64,"messages":[{"role":"user","content":mode}],
                    "tools":[{"type":"function","function":function}],"stream":true,
                    "stream_options":{"include_usage":true,"include_obfuscation":false}})
            };
            let count = seen.lock().unwrap().len();
            let mut outgoing = client
                .post(format!("{base}{path}"))
                .bearer_auth("synthetic-client-identity-00000001");
            if profile.is_none() {
                outgoing = outgoing.header("anthropic-version", "2026-10-21");
            }
            let mut response =
                tokio::time::timeout(Duration::from_secs(1), outgoing.json(&input).send())
                    .await
                    .expect("headers before release")
                    .unwrap();
            assert_eq!(response.status(), 200);
            let mut output = vec![];
            tokio::time::timeout(Duration::from_secs(1), async {
                while !String::from_utf8_lossy(&output).contains("early")
                    || !String::from_utf8_lossy(&output).contains("lookup")
                {
                    output
                        .extend_from_slice(&response.chunk().await.unwrap().expect("live prefix"));
                }
            })
            .await
            .expect("text and tool publication before release");
            if mode == "cancel" {
                drop(response);
            } else {
                // Drain ready frames, then prove a native message_stop is not transport EOF.
                while let Ok(next) =
                    tokio::time::timeout(Duration::from_millis(100), response.chunk()).await
                {
                    output.extend_from_slice(&next.unwrap().expect("upstream held open"));
                }
                assert!(!String::from_utf8_lossy(&output).contains("response.completed"));
                assert!(!String::from_utf8_lossy(&output).contains("[DONE]"));
                assert!(!String::from_utf8_lossy(&output).contains("message_stop"));
                for data in String::from_utf8_lossy(&output)
                    .lines()
                    .filter_map(|line| line.strip_prefix("data: "))
                {
                    if let Ok(event) = serde_json::from_str::<Value>(data) {
                        assert!(event["delta"]["stop_reason"].is_null());
                        if let Some(choices) = event["choices"].as_array() {
                            assert!(choices.iter().all(|c| c["finish_reason"].is_null()));
                        }
                    }
                }
                if mode == "hold" {
                    tokio::time::sleep(Duration::from_millis(1100)).await;
                }
                control.release.notify_one();
                let mut failed = false;
                loop {
                    match response.chunk().await {
                        Ok(Some(bytes)) => output.extend_from_slice(&bytes),
                        Ok(None) => break,
                        Err(_) => {
                            failed = true;
                            break;
                        }
                    }
                }
                assert_eq!(failed, mode == "cut");
                let output = String::from_utf8(output).unwrap();
                if mode == "hold" {
                    if let Some(profile) = profile {
                        let value = stream_body(&output, profile);
                        assert_eq!(
                            value[if responses { "status" } else { "object" }],
                            if responses {
                                "completed"
                            } else {
                                "chat.completion"
                            }
                        );
                    } else {
                        let value = messages_stream_body(&output);
                        assert_eq!(value["stop_reason"], "tool_use");
                        assert_eq!(value["content"][1]["id"], "held-call");
                    }
                } else {
                    assert!(!output.contains("response.completed") && !output.contains("[DONE]"));
                    assert!(!output.contains("message_stop"));
                }
            }
            tokio::time::timeout(Duration::from_secs(2), control.dropped.notified())
                .await
                .expect("upstream body released");
            assert_eq!(
                seen.lock().unwrap().len(),
                count + 1,
                "no replay after commitment"
            );
        }
    }
    assert_eq!(seen.lock().unwrap().len(), 9);
}
