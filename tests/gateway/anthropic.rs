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
    seen.lock().unwrap().push(body.clone());
    let prompt = body["messages"][0]["content"][0]["text"].as_str().unwrap();
    if matches!(prompt, "hold" | "cut" | "cancel") {
        assert_eq!(body["stream"], true);
        return controlled_response(prompt, control);
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
            "malformed" => response["usage"]["input_tokens"] = json!("invalid"),
            "bad-signature" => {
                response["content"] =
                    json!([{"type":"thinking","thinking":"private","signature":7}])
            }
            _ => panic!("unexpected synthetic prompt"),
        }
        return native_response(response, body["stream"] == true);
    }
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
                ("upstream-error", 502),
                ("malformed", 502),
                ("bad-signature", 502),
                ("unknown-usage", 200),
                ("truncated", 200),
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
                    count + if prompt == "upstream-error" { 2 } else { 1 }
                );
                assert!(!body.to_string().contains("synthetic-private"));
                if expected == 200 {
                    assert!(body["usage"].is_null());
                    if responses {
                        assert_eq!(
                            body["status"],
                            if prompt == "truncated" {
                                "incomplete"
                            } else {
                                "completed"
                            }
                        );
                    } else {
                        assert_eq!(
                            body["choices"][0]["finish_reason"],
                            if prompt == "truncated" {
                                "length"
                            } else {
                                "stop"
                            }
                        );
                    }
                }
            }
        }
    }
    assert_eq!(seen.lock().unwrap().len(), 20);
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
    assert_eq!(seen.lock().unwrap().len(), 26);
    diagnostic_owner.flush_diagnostics().await;
    let records = std::fs::read_to_string(&diagnostics).unwrap();
    assert!(records.contains(r#""request_rules":["messages.omit-sampling.v1"]"#));
    assert!(!records.contains("0.375"));
}
