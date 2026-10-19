//! Selected R1 bindings against independent synthetic wire oracles.
use crate::r1_response_wire as responses;
use axum::{
    Json, Router, body::Body, extract::State, http::HeaderMap, response::Response, routing::post,
};
use morphiecore::{
    gateway::{Entry, Gateway, Limits, TaskEntries},
    protocol::openai::Profile,
    provider::{SecretMaterial, TrustedOrigin},
    topology::{self, catalog},
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};
use tokio::net::TcpListener;

pub const CLIENT_KEY: &str = "synthetic-r1-client-identity-000001";
pub const RESULT: &str = "{\"error\":\"synthetic lookup unavailable\",\"label\":\"cedar-71\"}";
pub struct Fixture {
    pub base: String,
    pub seen: Arc<Mutex<Vec<Value>>>,
    pub gateway: Gateway,
    #[allow(dead_code)] // The gateway target owns lifecycle-control assertions.
    pub control: Arc<Control>,
    tasks: Vec<tokio::task::JoinHandle<()>>,
}
#[derive(Default)]
pub struct Control {
    pub release: tokio::sync::Notify,
    pub dropped: tokio::sync::Notify,
}
struct DropSignal(Arc<Control>);
impl Drop for DropSignal {
    fn drop(&mut self) {
        self.0.dropped.notify_one();
    }
}
type StateData = (Arc<Mutex<Vec<Value>>>, Arc<Control>);
impl Drop for Fixture {
    fn drop(&mut self) {
        for task in &self.tasks {
            task.abort();
        }
    }
}
pub fn snapshot(chat: bool, turn: usize) -> Value {
    if chat {
        return json!({"id":format!("chat-{turn}"),"object":"chat.completion","created":1,
            "model":"qwen3.8-flash","choices":[{"index":0,
                "message":if turn == 1 { json!({"role":"assistant","content":null,
                    "reasoning_content":"Plan 🧪","tool_calls":[{"id":"call-r1","type":"function",
                    "function":{"name":"lookup","arguments":"{ \"key\": \"A\" }"}}]}) }
                else {json!({"role":"assistant","reasoning_content":"Check","content":"cedar-71"})},
                "finish_reason":if turn == 1 {"tool_calls"}else{"stop"}}],
            "usage":{"prompt_tokens":11,"completion_tokens":7,"total_tokens":18,
                "completion_tokens_details":{"reasoning_tokens":2}}});
    }
    json!({"id":format!("response-{turn}"),"object":"response","created_at":1,
        "completed_at":2,"model":"openai/gpt-6-luna","status":"completed","error":null,
        "incomplete_details":null,"output":if turn == 1 {json!([
            {"id":"reason","type":"reasoning","status":"completed",
                "summary":[{"type":"summary_text","text":"Plan 🧪"}],
                "content":[{"type":"reasoning_text","text":"Check"}],
                "encrypted_content":"synthetic-final-token"},
            {"id":"comment","type":"message","role":"assistant","phase":"commentary",
                "status":"completed","content":[{"type":"output_text","text":"checking","annotations":[],"logprobs":[]}]},
            {"id":"call-item","type":"function_call","status":"completed","call_id":"call-r1",
                "name":"lookup","arguments":"{ \"key\": \"A\" }"}
        ])}else{json!([
            {"id":format!("answer-{turn}"),"type":"message","role":"assistant","phase":"final_answer",
                "status":"completed","content":[{"type":"output_text","text":"cedar-71","annotations":[],"logprobs":[]}]}
        ])},"usage":{"input_tokens":11,"output_tokens":7,"total_tokens":18,
            "input_tokens_details":{"cached_tokens":3},"output_tokens_details":{"reasoning_tokens":2}}})
}
pub fn events(chat: bool, turn: usize) -> Vec<Value> {
    if !chat {
        return responses::response_events(snapshot(false, turn));
    }
    let full = snapshot(true, turn);
    let chunk = |delta: Value, finish: Value| {
        json!({"id":full["id"],"object":"chat.completion.chunk",
        "created":1,"model":full["model"],"choices":[{"index":0,"delta":delta,"finish_reason":finish}]})
    };
    let mut events = vec![
        chunk(json!({"role":"assistant"}), Value::Null),
        chunk(
            json!({"reasoning_content":if turn==1{"Plan 🧪"}else{"Check"}}),
            Value::Null,
        ),
    ];
    if turn == 1 {
        events.push(chunk(
            json!({"tool_calls":[{"index":0,"id":"call-r1","type":"function",
            "function":{"name":"lookup","arguments":"{ \"key\":"}}]}),
            Value::Null,
        ));
        events.push(chunk(
            json!({"tool_calls":[{"index":0,"function":{"arguments":" \"A\" }"}}]}),
            Value::Null,
        ));
    } else {
        events.push(chunk(json!({"content":"cedar-71"}), Value::Null));
    }
    events.push(chunk(
        json!({}),
        json!(if turn == 1 { "tool_calls" } else { "stop" }),
    ));
    events.push(
        json!({"id":full["id"],"object":"chat.completion.chunk","created":1,
        "model":full["model"],"choices":[],"usage":full["usage"]}),
    );
    events
}
async fn answer(
    State((seen, control)): State<StateData>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    assert!(matches!(
        headers["authorization"].to_str().unwrap(),
        "Bearer synthetic-r1-upstream" | "Bearer synthetic-r1-secondary"
    ));
    let chat = body.get("messages").is_some();
    assert_eq!(
        body["model"],
        if chat {
            "qwen3.8-flash"
        } else {
            "openai/gpt-6-luna"
        }
    );
    assert_eq!(
        body[if chat {
            "max_completion_tokens"
        } else {
            "max_output_tokens"
        }],
        256
    );
    if chat {
        assert_eq!(body["reasoning_effort"], "low");
    } else {
        assert_eq!(body["reasoning"]["effort"], "low");
    }
    for field in [
        "temperature",
        "top_p",
        "parallel_tool_calls",
        "top_logprobs",
    ] {
        assert!(body.get(field).is_none(), "unexpected {field}");
    }
    let tool = &body["tools"][0];
    let tool = if chat { &tool["function"] } else { tool };
    assert_eq!(tool["name"], "lookup");
    if chat {
        assert!(tool.get("strict").is_none_or(|v| v == false));
    } else {
        assert_eq!(tool["strict"], false);
        assert_eq!(body["provider"], json!({"require_parameters":true}));
    }
    assert_eq!(
        tool["parameters"],
        json!({"type":"object",
        "properties":{"key":{"type":"string"}},"required":["key"]})
    );
    let history = body[if chat { "messages" } else { "input" }]
        .as_array()
        .unwrap();
    let turn = if history.iter().any(|v| {
        v["role"] == "user" && (v["content"] == "next" || v["content"][0]["text"] == "next")
    }) {
        3
    } else if history
        .iter()
        .any(|v| v["role"] == "tool" || v["type"] == "function_call_output")
    {
        2
    } else {
        1
    };
    if !chat {
        assert_eq!(body["store"], false);
        assert!(body.get("previous_response_id").is_none());
    }
    if turn >= 2 {
        if chat {
            let assistant = history.iter().find(|v| v["role"] == "assistant").unwrap();
            assert_eq!(assistant["reasoning_content"], "Plan 🧪");
            assert_eq!(assistant["tool_calls"][0]["id"], "call-r1");
            assert_eq!(
                assistant["tool_calls"][0]["function"]["arguments"],
                "{ \"key\": \"A\" }"
            );
            assert!(history.iter().any(|v| v["role"] == "tool"
                && v["tool_call_id"] == "call-r1"
                && v["content"] == RESULT));
        } else {
            assert!(history.contains(&json!({"id":"reason","type":"reasoning",
                "summary":[{"type":"summary_text","text":"Plan 🧪"}],
                "content":[{"type":"reasoning_text","text":"Check"}],"encrypted_content":"synthetic-final-token"})));
            assert!(history.iter().any(|v| v["type"] == "function_call"
                && v["call_id"] == "call-r1"
                && v["arguments"] == "{ \"key\": \"A\" }"));
            assert!(
                history
                    .iter()
                    .any(|v| v["phase"] == "commentary" && v["content"][0]["text"] == "checking")
            );
            assert!(history.contains(
                &json!({"type":"function_call_output","call_id":"call-r1","output":RESULT})
            ));
        }
    }
    if turn == 3 {
        assert!(history.iter().any(|v| v["role"] == "assistant"
            && if chat {
                v["content"] == "cedar-71" && v["reasoning_content"] == "Check"
            } else {
                v["phase"] == "final_answer" && v["content"][0]["text"] == "cedar-71"
            }));
    }
    seen.lock().unwrap().push(body.clone());
    let stream = body["stream"] == true;
    let bytes = if stream {
        let mut bytes = Vec::new();
        for event in events(chat, turn) {
            let mode = history[0]["content"]
                .as_str()
                .or_else(|| history[0]["content"][0]["text"].as_str())
                .unwrap();
            if matches!(mode, "cut" | "cancel")
                && (event["type"] == "response.completed"
                    || event["choices"][0]["finish_reason"].is_string())
            {
                break;
            }
            bytes.extend_from_slice(format!("data: {event}\n\n").as_bytes());
        }
        if chat && !matches!(history[0]["content"].as_str(), Some("cut" | "cancel")) {
            bytes.extend_from_slice(b"data: [DONE]\n\n");
        }
        bytes
    } else {
        serde_json::to_vec(&snapshot(chat, turn)).unwrap()
    };
    let mode = history[0]["content"]
        .as_str()
        .or_else(|| history[0]["content"][0]["text"].as_str())
        .unwrap();
    if matches!(mode, "hold" | "cut" | "cancel") {
        let cut = mode == "cut";
        let stream = futures_util::stream::unfold(
            (0, DropSignal(control), bytes, cut),
            |(phase, guard, bytes, cut)| async move {
                match phase {
                    0 => Some((
                        Ok::<_, std::io::Error>(bytes::Bytes::copy_from_slice(&bytes)),
                        (1, guard, bytes, cut),
                    )),
                    1 => {
                        guard.0.release.notified().await;
                        if cut {
                            Some((
                                Err(std::io::Error::other("synthetic disconnect")),
                                (2, guard, bytes, cut),
                            ))
                        } else {
                            None
                        }
                    }
                    _ => None,
                }
            },
        );
        return Response::builder()
            .header("content-type", "text/event-stream")
            .body(Body::from_stream(stream))
            .unwrap();
    }
    Response::builder()
        .header(
            "content-type",
            if stream {
                "text/event-stream"
            } else {
                "application/json"
            },
        )
        .body(Body::from_stream(futures_util::stream::iter(
            bytes
                .chunks(13)
                .map(|b| Ok::<_, std::io::Error>(bytes::Bytes::copy_from_slice(b)))
                .collect::<Vec<_>>(),
        )))
        .unwrap()
}
pub async fn start() -> Fixture {
    start_with_fallback(false).await
}
pub async fn start_with_fallback(fallback: bool) -> Fixture {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let control = Arc::new(Control::default());
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin =
        TrustedOrigin::parse(&format!("http://{}", listener.local_addr().unwrap())).unwrap();
    let app = Router::new()
        .route("/compatible-mode/v1/chat/completions", post(answer))
        .route("/api/v1/responses", post(answer))
        .with_state((seen.clone(), control.clone()));
    let upstream = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let catalog = catalog::default_topology().unwrap();
    let mut providers = Vec::new();
    let mut endpoints = Vec::new();
    let mut routes = Vec::new();
    let mut models = Vec::new();
    let mut canonical = Vec::new();
    let mut entries = Vec::new();
    let mut credentials = BTreeMap::new();
    for (label, id, protocol) in [
        ("qwen3.8-flash", "dashscope-flash-chat", Profile::Chat),
        ("gpt-6-luna", "openrouter-responses", Profile::Responses),
    ] {
        let mut endpoint = catalog
            .endpoint(&topology::EndpointId::new(id).unwrap())
            .unwrap()
            .clone();
        let mut provider = catalog.endpoint_provider(&endpoint).unwrap().clone();
        provider.origin = origin.clone();
        endpoint.target.origin = origin.clone();
        let public = catalog.model(label).unwrap().clone();
        let mut route = catalog.route(&public.route).unwrap().clone();
        route.endpoints = vec![endpoint.id.clone()];
        route.policy.max_attempts = 1;
        if fallback {
            let mut secondary = endpoint.clone();
            secondary.id = topology::EndpointId::new(&format!("{id}-secondary")).unwrap();
            secondary.credential =
                morphiecore::provider::CredentialBindingId::new(&format!("{id}-secondary-key"))
                    .unwrap();
            route.endpoints.push(secondary.id.clone());
            route.policy.max_attempts = 2;
            route.policy.fallback = topology::FallbackPolicy::BeforeCommit;
            entries.push(Entry {
                model: label.into(),
                protocol,
                endpoint: secondary.id.clone(),
            });
            credentials.insert(
                secondary.credential.clone(),
                Arc::new(SecretMaterial::new("synthetic-r1-secondary").unwrap()),
            );
            endpoints.push(secondary);
        }
        canonical.push(topology::CanonicalModel {
            id: public.canonical_model.clone(),
            task: public.task,
            contract: public.contract.clone(),
        });
        entries.push(Entry {
            model: label.into(),
            protocol,
            endpoint: endpoint.id.clone(),
        });
        credentials.insert(
            endpoint.credential.clone(),
            Arc::new(SecretMaterial::new("synthetic-r1-upstream").unwrap()),
        );
        providers.push(provider);
        endpoints.push(endpoint);
        routes.push(route);
        models.push(public);
    }
    let metadata = models
        .iter()
        .map(|model| {
            (
                model.id.clone(),
                topology::ModelMetadata::new(1, "Synthetic R1").unwrap(),
            )
        })
        .collect::<Vec<_>>();
    let topology = topology::compile(providers, endpoints, routes, models, canonical)
        .unwrap()
        .with_model_metadata(metadata)
        .unwrap();
    let gateway = Gateway::new(
        topology,
        TaskEntries {
            generation: entries,
            ..Default::default()
        },
        credentials,
        SecretMaterial::new(CLIENT_KEY).unwrap(),
        Limits::default(),
        None,
    )
    .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}/v1", listener.local_addr().unwrap());
    let owner = gateway.clone();
    let server =
        tokio::spawn(async move { owner.serve(listener, std::future::pending()).await.unwrap() });
    Fixture {
        base,
        seen,
        gateway,
        control,
        tasks: vec![server, upstream],
    }
}
