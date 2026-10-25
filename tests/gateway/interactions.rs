//! Independent stateless tool replay through the Router and an Interactions upstream.
use super::Guard;
use axum::{
    Json, Router,
    extract::State,
    http::HeaderMap,
    response::{IntoResponse, Response},
    routing::post,
};
use morphiecore::{
    gateway::{Entry, Gateway, Limits},
    protocol::openai::Profile,
    provider::{SecretMaterial, TrustedOrigin},
    topology::{self, catalog::GOOGLE_BINDINGS},
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::net::TcpListener;
fn wire(events: Vec<Value>, done: bool) -> String {
    let mut wire = events
        .iter()
        .map(|e| {
            format!(
                "event: {}\ndata: {e}\n\n",
                e["event_type"].as_str().unwrap()
            )
        })
        .collect::<String>();
    if done {
        wire.push_str("event: done\ndata: [DONE]\n\n");
    }
    wire
}
#[derive(Clone, Default)]
struct Fixture {
    seen: Arc<Mutex<Vec<Value>>>,
    dropped: Arc<tokio::sync::Notify>,
}
struct DropSignal(Arc<tokio::sync::Notify>);
impl Drop for DropSignal {
    fn drop(&mut self) {
        self.0.notify_one();
    }
}
async fn answer(
    State(state): State<Fixture>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    assert_eq!(headers["x-goog-api-key"], "synthetic-upstream");
    assert!(headers.get("authorization").is_none());
    assert_eq!(body["store"], false);
    assert_eq!(body["stream"], true);
    assert!(body.get("previous_interaction_id").is_none());
    let hold = body["input"][0]["content"][0]["text"] == "hold";
    let plain = body["input"][0]["content"][0]["text"] == "plain";
    let replay = body["input"]
        .as_array()
        .unwrap()
        .iter()
        .any(|i| i["type"] == "function_result");
    state.seen.lock().unwrap().push(body.clone());
    let mut events = vec![
        json!({"event_type":"interaction.created","interaction":{"object":"interaction","model":"gemini-3.8-flash"}}),
        json!({"event_type":"interaction.status_update","status":"in_progress"}),
    ];
    if hold || replay || plain {
        if replay {
            assert_eq!(
                body["input"][1],
                json!({"type":"thought","summary":[{"type":"text","text":"Plan"}],"signature":"synthetic-thought"})
            );
            assert!(
                body["input"].as_array().unwrap().iter().any(|i| i
                    == &json!({"type":"function_call","id":"c1","name":"read","arguments":{}}))
            );
        }
        events.extend([
            json!({"event_type":"step.start","index":0,"step":{"type":"model_output"}}),
            json!({"event_type":"step.delta","index":0,"delta":{"type":"text","text":"done"}}),
        ]);
        if hold {
            let stream = futures_util::stream::unfold(
                (Some(wire(events, false)), DropSignal(state.dropped)),
                |(first, guard)| async move {
                    match first {
                        Some(s) => Some((
                            Ok::<_, std::io::Error>(bytes::Bytes::from(s)),
                            (None, guard),
                        )),
                        None => std::future::pending().await,
                    }
                },
            );
            return Response::builder()
                .header("content-type", "text/event-stream")
                .body(axum::body::Body::from_stream(stream))
                .unwrap();
        }
        events.push(json!({"event_type":"step.stop","index":0}));
    } else {
        events.extend([
            json!({"event_type":"step.start","index":0,"step":{"type":"thought"}}),
            json!({"event_type":"step.delta","index":0,"delta":{"type":"thought_summary","content":{"type":"text","text":"Plan"}}}),
            json!({"event_type":"step.delta","index":0,"delta":{"type":"thought_signature","signature":"synthetic-thought"}}),
            json!({"event_type":"step.stop","index":0}),
            json!({"event_type":"step.start","index":1,"step":{"type":"function_call","id":"c1","name":"read","arguments":{}}}),
            json!({"event_type":"step.stop","index":1}),
        ]);
    }
    let mut final_resource = json!({
        "status":if replay || plain{"completed"}else{"requires_action"},
        "usage":{"total_input_tokens":10,"total_output_tokens":5,"total_thought_tokens":0,"total_cached_tokens":0,"total_tokens":15}
    });
    if plain {
        final_resource["id"] = json!("i");
    }
    events.push(json!({"event_type":"interaction.completed","interaction":final_resource}));
    ([("content-type", "text/event-stream")], wire(events, true)).into_response()
}
#[tokio::test]
async fn stateless_json_and_sse_replay_signatures_and_cancel_upstream() {
    let state = Fixture::default();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let app = Router::new()
        .route("/v1/interactions", post(answer))
        .with_state(state.clone());
    let _upstream = Guard(tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap()
    }));
    let binding = &GOOGLE_BINDINGS[0];
    let mut provider = binding.provider();
    provider.origin = TrustedOrigin::parse(&origin).unwrap();
    let mut endpoint = binding.interactions_endpoint();
    endpoint.target.origin = provider.origin.clone();
    let mut route = binding.route();
    assert_eq!(
        route.endpoints,
        [binding.endpoint().id, endpoint.id.clone()]
    );
    route.endpoints = vec![endpoint.id.clone()];
    let topology = topology::compile(
        vec![provider],
        vec![endpoint.clone()],
        vec![route],
        vec![binding.public_model()],
        vec![binding.canonical_model()],
    )
    .unwrap()
    .with_model_metadata([(endpoint.canonical_model.clone(), binding.metadata())])
    .unwrap();
    let gateway = Gateway::new(
        topology,
        morphiecore::gateway::TaskEntries {
            generation: [Profile::Chat, Profile::Responses]
                .map(|protocol| Entry {
                    chat_profile: Default::default(),
                    model: binding.model.into(),
                    protocol,
                    endpoint: endpoint.id.clone(),
                })
                .to_vec(),
            ..Default::default()
        },
        BTreeMap::from([(
            binding.credential(),
            Arc::new(SecretMaterial::new("synthetic-upstream").unwrap()),
        )]),
        std::collections::BTreeMap::from([(
            "fixture-agent".into(),
            SecretMaterial::new(super::support::CLIENT_KEY).unwrap(),
        )]),
        Limits::default(),
        None,
    )
    .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let _gateway = Guard(tokio::spawn(async move {
        gateway
            .serve(listener, std::future::pending())
            .await
            .unwrap()
    }));
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();
    let request = json!({"model":binding.model,"max_output_tokens":128,"input":[{"role":"user","content":"Read."}],
        "tools":[{"type":"function","name":"read","strict":false,"parameters":{"type":"object","properties":{}}}]});
    let response = client
        .post(format!("{base}/v1/responses"))
        .bearer_auth(super::support::CLIENT_KEY)
        .json(&request)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let response: Value = response.json().await.unwrap();
    assert!(response["id"].as_str().unwrap().starts_with("resp_"));
    // Downstream sum derives from the selected input and inclusive output;
    // the source's unreported total relationship remains unchanged.
    assert_eq!(response["usage"]["input_tokens"], 10);
    assert_eq!(response["usage"]["output_tokens"], 5);
    assert_eq!(response["usage"]["total_tokens"], 15);
    assert!(!response.to_string().contains("synthetic-thought"));
    let mut next = request.clone();
    let input = next["input"].as_array_mut().unwrap();
    input.extend(response["output"].as_array().unwrap().clone());
    input.push(json!({"type":"function_call_output","call_id":"c1","output":"ok"}));
    next["stream"] = json!(true);
    let response = client
        .post(format!("{base}/v1/responses"))
        .bearer_auth(super::support::CLIENT_KEY)
        .json(&next)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = response.text().await.unwrap();
    assert!(body.contains("response.completed"), "{body}");
    assert!(body.contains("done"));
    assert_eq!(state.seen.lock().unwrap().len(), 2);
    let response = client
        .post(format!("{base}/v1/chat/completions"))
        .bearer_auth(super::support::CLIENT_KEY)
        .json(
            &json!({"model":binding.model,"max_completion_tokens":128,"stream":true,
            "messages":[{"role":"user","content":"plain"}]}),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = response.text().await.unwrap();
    let chunks = body
        .lines()
        .filter_map(|line| line.strip_prefix("data: "))
        .filter(|line| *line != "[DONE]")
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    let client_id = chunks[0]["id"].as_str().unwrap();
    assert_ne!(client_id, "i");
    assert!(chunks.iter().all(|chunk| chunk["id"] == client_id));
    assert!(
        chunks
            .iter()
            .any(|chunk| chunk["choices"][0]["delta"]["content"] == "done")
    );
    assert!(
        chunks
            .iter()
            .any(|chunk| chunk["choices"][0]["finish_reason"] == "stop")
    );
    assert!(body.contains("[DONE]"));
    let mut hold = request;
    hold["input"][0]["content"] = json!("hold");
    hold["stream"] = json!(true);
    let mut response = client
        .post(format!("{base}/v1/responses"))
        .bearer_auth(super::support::CLIENT_KEY)
        .json(&hold)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert!(response.chunk().await.unwrap().is_some());
    drop(response);
    tokio::time::timeout(Duration::from_secs(3), state.dropped.notified())
        .await
        .unwrap();
    assert_eq!(state.seen.lock().unwrap().len(), 4);
}
