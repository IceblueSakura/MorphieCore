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

async fn answer(
    State(seen): State<Arc<Mutex<Vec<Value>>>>,
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
        return Json(response).into_response();
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
    Json(json!({"id":"msg-local","type":"message","role":"assistant","model":"claude-haiku-5-5",
        "content": if turn { json!([{"type":"text","text":"answer"}]) } else { json!([
            {"type":"thinking","thinking":"plan","signature":"synthetic-native-signature"},
            {"type":"tool_use","id":"call-local","name":"lookup","input":{"key":"A"}}
        ]) },
        "stop_reason":if turn {"end_turn"}else{"tool_use"},"stop_sequence":null,
        "usage":{"input_tokens":11,"cache_read_input_tokens":3,"cache_creation_input_tokens":2,"output_tokens":7},
        "future_report":{"ignored":true}
    })).into_response()
}

#[tokio::test]
async fn json_clients_replay_native_thinking_and_tools_without_a_private_input_api() {
    let seen = Arc::new(Mutex::new(vec![]));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let app = Router::new()
        .route("/zen/go/v1/messages", post(answer))
        .with_state(seen.clone());
    let _upstream = Guard(tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap()
    }));
    let binding = &MESSAGES_BINDINGS[0];
    let mut provider = binding.provider();
    provider.origin = TrustedOrigin::parse(&origin).unwrap();
    let mut endpoint = binding.endpoint();
    endpoint.target.origin = provider.origin.clone();
    let endpoint_id = endpoint.id.clone();
    let model = endpoint.canonical_model.clone();
    let route = topology::Route {
        id: topology::RouteId::new("native").unwrap(),
        task: topology::TaskKind::Generation,
        endpoints: vec![endpoint_id.clone()],
        policy: Default::default(),
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
        vec![endpoint],
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
            .map(|protocol| Entry {
                model: binding.model.into(),
                protocol,
                endpoint: endpoint_id.clone(),
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
                "previous_response_id":null,"include":["reasoning.encrypted_content"],
                "input":[{"role":"user","content":"lookup"}],"tools":[tool]})
        } else {
            json!({"model":binding.model,"max_completion_tokens":64,"messages":[{"role":"user","content":"lookup"}],
                "tools":[{"type":"function","function":function}]})
        };
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
            let body: Value = result.json().await.unwrap();
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
                    history
                        .push(json!({"role":"tool","tool_call_id":"call-local","content":"found"}));
                }
            } else if responses {
                assert_eq!(body["output"][0]["content"][0]["text"], "answer");
            } else {
                assert_eq!(body["choices"][0]["message"]["content"], "answer");
            }
        }
        input["stream"] = json!(true);
        let count = seen.lock().unwrap().len();
        let rejected = client
            .post(format!("{base}{path}"))
            .bearer_auth("synthetic-client-identity-00000001")
            .json(&input)
            .send()
            .await
            .unwrap();
        assert_eq!(rejected.status(), 400);
        assert_eq!(seen.lock().unwrap().len(), count);
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
            assert_eq!(seen.lock().unwrap().len(), count + 1);
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
    assert_eq!(seen.lock().unwrap().len(), 14);
}
