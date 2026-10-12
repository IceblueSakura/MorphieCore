//! Independent HTTP oracle for standard clients over a native Messages endpoint.
use axum::response::{IntoResponse, Response};
use axum::{Json, Router, extract::State, http::HeaderMap, routing::post};
use morphiecore::{
    gateway::{Entry, Gateway, Limits},
    protocol::{anthropic, openai::Profile, replay_envelope},
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

struct Guard(tokio::task::JoinHandle<()>);
impl Drop for Guard {
    fn drop(&mut self) {
        self.0.abort();
    }
}

fn native_stream_body(value: &Value) -> String {
    let mut initial = value.clone();
    initial["content"] = json!([]);
    initial["stop_reason"] = Value::Null;
    initial["usage"]["output_tokens"] = json!(0);
    let mut events = vec![json!({"type":"message_start","message":initial})];
    for (index, block) in value["content"].as_array().unwrap().iter().enumerate() {
        match block["type"].as_str().unwrap() {
            "text" => {
                events.push(json!({"type":"content_block_start","index":index,"content_block":{"type":"text","text":""}}));
                events.push(json!({"type":"content_block_delta","index":index,"delta":{"type":"text_delta","text":block["text"]}}));
            }
            "thinking" => {
                events.push(json!({"type":"content_block_start","index":index,"content_block":{"type":"thinking","thinking":"","signature":""}}));
                events.push(json!({"type":"content_block_delta","index":index,"delta":{"type":"thinking_delta","thinking":block["thinking"]}}));
                events.push(json!({"type":"content_block_delta","index":index,"delta":{"type":"signature_delta","signature":block["signature"]}}));
            }
            "tool_use" => {
                events.push(json!({"type":"content_block_start","index":index,"content_block":{"type":"tool_use","id":block["id"],"name":block["name"],"input":{}}}));
                events.push(json!({"type":"content_block_delta","index":index,"delta":{"type":"input_json_delta","partial_json":block["input"].to_string()}}));
            }
            _ => panic!("fixture block"),
        }
        events.push(json!({"type":"content_block_stop","index":index}));
    }
    events.push(json!({"type":"message_delta","delta":{"stop_reason":value["stop_reason"]},"usage":{"output_tokens":value["usage"]["output_tokens"]}}));
    events.push(json!({"type":"message_stop"}));
    events
        .iter()
        .map(|event| {
            format!(
                "event: {}\ndata: {event}\n\n",
                event["type"].as_str().unwrap()
            )
        })
        .collect()
}
fn native_response(value: Value, stream: bool) -> Response {
    if !stream {
        return Json(value).into_response();
    }
    (
        [(axum::http::header::CONTENT_TYPE, "text/event-stream")],
        native_stream_body(&value),
    )
        .into_response()
}

async fn client_body(response: reqwest::Response, profile: Profile, stream: bool) -> Value {
    if !stream || response.status() != 200 {
        return response.json().await.unwrap();
    }
    let wire = response.text().await.unwrap();
    stream_body(&wire, profile)
}
fn stream_body(wire: &str, profile: Profile) -> Value {
    assert!(!wire.contains("synthetic-native-signature"));
    let client = morphiecore::protocol::openai::Adapter::new(
        profile,
        morphiecore::protocol::openai::Dialect::MorphieCore,
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
        "content":[{"type":"text","text":"early"}],"stop_reason":"end_turn",
        "usage":{"input_tokens":1,"output_tokens":1}});
    let bytes = native_stream_body(&message);
    let prefix = if mode == "hold" {
        bytes
    } else {
        bytes
            .split_once("event: content_block_stop")
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
    assert_eq!(headers["anthropic-version"], "2023-06-01");
    assert_eq!(headers["x-api-key"], "synthetic-upstream");
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
    // Messages has no cache key or retention; the gateway always requests caching.
    assert_eq!(body["cache_control"], json!({"type":"ephemeral"}));
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
            "unknown-usage" => {}
            "truncated" => response["stop_reason"] = json!("max_tokens"),
            "refusal" => {
                response["stop_reason"] = json!("refusal");
                response["stop_details"] = json!({"category":"cyber","explanation":"synthetic"});
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
        assert_eq!(
            body["messages"],
            json!([
                {"role":"user","content":[{"type":"text","text":"lookup"}]},
                {"role":"assistant","content":[
                    {"type":"thinking","thinking":"plan","signature":"synthetic-native-signature"},
                    {"type":"tool_use","id":"call-local","name":"lookup","input":{"key":"A"}}
                ]},
                {"role":"user","content":[{"type":"tool_result","tool_use_id":"call-local","content":"found"}]}
            ])
        );
    }
    native_response(
        json!({"id":"msg-local","type":"message","role":"assistant","model":"claude-haiku-5-5",
            "content": if turn { json!([{"type":"text","text":"answer"}]) } else { json!([
                {"type":"thinking","thinking":"plan","signature":"synthetic-native-signature"},
                {"type":"tool_use","id":"call-local","name":"lookup","input":{"key":"A"}}
            ]) },
            "stop_reason":if turn {"end_turn"}else{"tool_use"},"stop_sequence":null,
            "usage":{"input_tokens":11,"cache_read_input_tokens":3,"cache_creation_input_tokens":2,"output_tokens":7},
            "future_report":{"ignored":true}
        }),
        body["stream"] == true,
    )
}

#[tokio::test]
async fn json_and_stream_clients_replay_native_thinking_and_tools() {
    let seen = Arc::new(Mutex::new(vec![]));
    let control = Arc::new(Control::default());
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let app = Router::new()
        .route("/zen/go/v1/messages", post(answer))
        .with_state((seen.clone(), control.clone()));
    let _upstream = Guard(tokio::spawn(async move {
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
    let contract = anthropic::Profile::AdaptiveTextTools.semantic_contract();
    let mut public = topology::PublicModel::new(
        model.clone(),
        model.clone(),
        topology::TaskKind::Generation,
        route.id.clone(),
        contract.clone(),
    );
    public.standard_context = false;
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
        [Profile::Responses, Profile::Chat]
            .into_iter()
            .flat_map(|protocol| {
                endpoint_ids.iter().map(move |endpoint| Entry {
                    model: binding.model.into(),
                    protocol,
                    endpoint: endpoint.clone(),
                })
            })
            .collect(),
        BTreeMap::from([(
            CredentialBindingId::new(binding.credential).unwrap(),
            Arc::new(SecretMaterial::new("synthetic-upstream").unwrap()),
        )]),
        SecretMaterial::new("synthetic-client-identity-00000001").unwrap(),
        Limits::default(),
        None,
    )
    .unwrap()
    .with_diagnostics(&diagnostics)
    .unwrap();
    let diagnostic_owner = gate.clone();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let _gateway = Guard(tokio::spawn(async move {
        gate.serve(listener, std::future::pending()).await.unwrap()
    }));
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(3))
        .build()
        .unwrap();
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
            // pi's cache fields; Codex always sends `client_metadata`, which no target takes.
            input["prompt_cache_key"] = json!("pi-session");
            input["prompt_cache_retention"] = json!("24h");
            input["client_metadata"] = json!({"session_id":"synthetic-session"});
            // Without `tool_choice`, the switch rides on Messages' default `auto`.
            input["parallel_tool_calls"] = json!(false);
            if stream && !responses {
                input["stream_options"] = json!({"include_usage":true,"include_obfuscation":false});
            }
            for turn in 0..2 {
                let result = client
                    .post(format!("{base}{path}"))
                    .bearer_auth("synthetic-client-identity-00000001")
                    .header("x-never-forward", "private")
                    .json(&input)
                    .send()
                    .await
                    .unwrap();
                let status = result.status();
                let body: Value = client_body(result, profile, stream).await;
                if status != 200 {
                    diagnostic_owner.flush_diagnostics().await;
                    panic!(
                        "{profile:?} turn {turn}: {body}; {}",
                        std::fs::read_to_string(&diagnostics).unwrap()
                    );
                }
                assert_eq!(body["model"], binding.model);
                assert!(
                    body[if responses { "created_at" } else { "created" }]
                        .as_u64()
                        .is_some()
                );
                assert_eq!(
                    body["usage"][if responses {
                        "input_tokens"
                    } else {
                        "prompt_tokens"
                    }],
                    16
                );
                assert_eq!(body["usage"]["total_tokens"], 23);
                assert!(!body.to_string().contains("synthetic-native-signature"));
                if turn == 0 {
                    let token = if responses {
                        &body["output"][0]["encrypted_content"]
                    } else {
                        &body["choices"][0]["message"]["reasoning_details"][1]["data"]
                    };
                    let (issuer, format, payload) =
                        replay_envelope::open(token.as_str().unwrap()).unwrap();
                    assert_eq!(
                        issuer,
                        replay_envelope::issuer("opencode-go", binding.upstream)
                    );
                    assert_eq!(format,morphiecore::semantic::task::generation::ReplayFormat::AnthropicMessagesThinking);
                    assert_eq!(payload.as_str(), "synthetic-native-signature");
                    if responses {
                        let history = input["input"].as_array_mut().unwrap();
                        history.extend(body["output"].as_array().unwrap().iter().cloned());
                        history.push(json!({"type":"function_call_output","call_id":"call-local","output":"found"}));
                    } else {
                        let history = input["messages"].as_array_mut().unwrap();
                        history.push(body["choices"][0]["message"].clone());
                        history.push(
                            json!({"role":"tool","tool_call_id":"call-local","content":"found"}),
                        );
                    }
                } else if responses {
                    assert_eq!(body["output"][0]["content"][0]["text"], "answer");
                } else {
                    assert_eq!(body["choices"][0]["message"]["content"], "answer");
                }
            }
            if stream {
                continue;
            }
            input["stream"] = json!(false);
            for (prompt, expected) in [
                ("upstream-error", 503),
                ("malformed", 502),
                ("bad-signature", 502),
                ("unknown-usage", 200),
                ("truncated", 200),
                ("refusal", 200),
            ] {
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
                    assert!(body["usage"].is_null());
                    let (status, reason, finish) = match prompt {
                        "truncated" => ("incomplete", json!("max_output_tokens"), "length"),
                        "refusal" => ("incomplete", json!("content_filter"), "content_filter"),
                        _ => ("completed", Value::Null, "stop"),
                    };
                    if responses {
                        assert_eq!(body["status"], status);
                        assert_eq!(body["incomplete_details"]["reason"], reason);
                    } else {
                        assert_eq!(body["choices"][0]["finish_reason"], finish);
                    }
                }
            }
        }
    }
    assert_eq!(seen.lock().unwrap().len(), 26);
    // A Messages client (Claude Code) runs the same tool loop with thinking replay.
    let mut input = json!({"model":binding.model,"max_tokens":64,
        "metadata":{"user_id":"synthetic-user"},
        "system":[{"type":"text","text":"Be brief.","cache_control":{"type":"ephemeral"}}],
        "tools":[{"name":"lookup","strict":false,"cache_control":{"type":"ephemeral"},
            "input_schema":{"type":"object","properties":{"key":{"type":"string"}}}}],
        "tool_choice":{"type":"auto","disable_parallel_tool_use":true},
        "messages":[{"role":"user","content":"lookup"}]});
    for turn in 0..2 {
        let result = client
            .post(format!("{base}/v1/messages"))
            .header("x-api-key", "synthetic-client-identity-00000001")
            .header("anthropic-version", "2023-06-01")
            .json(&input)
            .send()
            .await
            .unwrap();
        let status = result.status();
        let body: Value = result.json().await.unwrap();
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
        assert_eq!(
            body["content"][1],
            json!({"type":"tool_use","id":"call-local","name":"lookup","input":{"key":"A"}})
        );
        let history = input["messages"].as_array_mut().unwrap();
        history.push(json!({"role":"assistant","content":body["content"]}));
        history.push(json!({"role":"user","content":[
            {"type":"tool_result","tool_use_id":"call-local","content":"found"}]}));
    }
    assert_eq!(seen.lock().unwrap().len(), 28);
    for profile in [Profile::Responses, Profile::Chat] {
        let responses = profile == Profile::Responses;
        let path = if responses {
            "/v1/responses"
        } else {
            "/v1/chat/completions"
        };
        for mode in ["hold", "cut", "cancel"] {
            let function = json!({"name":"lookup","parameters":{"type":"object","properties":{"key":{"type":"string"}}},"strict":false});
            let input = if responses {
                let mut tool = function.clone();
                tool["type"] = json!("function");
                json!({"model":binding.model,"max_output_tokens":64,"input":mode,"stream":true,"tools":[tool]})
            } else {
                json!({"model":binding.model,"max_completion_tokens":64,"messages":[{"role":"user","content":mode}],
                    "tools":[{"type":"function","function":function}],"stream":true,
                    "stream_options":{"include_usage":true,"include_obfuscation":false}})
            };
            let count = seen.lock().unwrap().len();
            let mut response = tokio::time::timeout(
                Duration::from_secs(1),
                client
                    .post(format!("{base}{path}"))
                    .bearer_auth("synthetic-client-identity-00000001")
                    .json(&input)
                    .send(),
            )
            .await
            .expect("headers before release")
            .unwrap();
            assert_eq!(response.status(), 200);
            let mut output = vec![];
            tokio::time::timeout(Duration::from_secs(1), async {
                while !String::from_utf8_lossy(&output).contains("early") {
                    output
                        .extend_from_slice(&response.chunk().await.unwrap().expect("live prefix"));
                }
            })
            .await
            .expect("text before release");
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
                    assert!(!output.contains("response.completed") && !output.contains("[DONE]"));
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
    assert_eq!(seen.lock().unwrap().len(), 34);
    // Error classes: Codex reads overflow only from a streamed `response.failed`;
    // a failure after headers ends the stream with its class instead of a cut.
    for profile in [Profile::Responses, Profile::Chat] {
        let responses = profile == Profile::Responses;
        for stream in [false, true] {
            for prompt in ["overflow", "overloaded", "overloaded-early"] {
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
                assert_eq!(wire.contains("partial"), prompt == "overloaded");
                let last: Value = serde_json::from_str(
                    wire.lines()
                        .filter_map(|l| l.strip_prefix("data: "))
                        .next_back()
                        .unwrap(),
                )
                .unwrap();
                if responses {
                    assert_eq!(last["type"], "response.failed");
                    assert_eq!(last["response"]["status"], "failed");
                    assert_eq!(last["response"]["error"]["code"], code);
                } else {
                    assert_eq!(last["error"]["code"], code);
                }
            }
        }
    }
    assert_eq!(seen.lock().unwrap().len(), 52);
    diagnostic_owner.flush_diagnostics().await;
    let records = std::fs::read_to_string(&diagnostics).unwrap();
    assert!(records.contains(r#""request_rules":["messages.omit-sampling.v1"]"#));
    assert!(records.contains(r#""ignored_request_fields":["client_metadata"]"#));
    assert!(!records.contains("0.375"));
}
