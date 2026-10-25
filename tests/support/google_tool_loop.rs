//! One selected Google candidate; independent native tool/error/final-history oracles.
#[path = "google_tool_loop/interactions.rs"]
mod interactions;
use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, Uri},
    response::Response,
    routing::post,
};
use morphiecore::{
    gateway::{Entry, Gateway, Limits, TaskEntries},
    protocol::openai::Profile,
    provider::{SecretMaterial, TrustedOrigin},
    topology::{self, catalog::GOOGLE_BINDINGS},
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};
use tokio::net::TcpListener;

pub const KEY: &str = "synthetic-google-loop-client-000001";
pub const ERROR: &str = "missing file";
pub const RESULT: &str = "cedar-71";
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Api {
    GenerateContent,
    Interactions,
}
pub struct Fixture {
    pub base: String,
    pub seen: Arc<Mutex<Vec<Value>>>,
    pub gateway: Gateway,
    tasks: Vec<tokio::task::JoinHandle<()>>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        for task in &self.tasks {
            task.abort();
        }
    }
}
#[derive(Clone)]
struct Upstream {
    profile: Profile,
    messages: bool,
    seen: Arc<Mutex<Vec<Value>>>,
}
fn expected_history(turn: usize, messages: bool) -> Value {
    let mut history = vec![json!({"role":"user","parts":[{"text":"Read."}]})];
    if turn >= 2 {
        let result = if messages {
            json!({"error":ERROR})
        } else {
            json!({"output":ERROR})
        };
        history.extend([
            json!({"role":"model","parts":[{"functionCall":{"id":"c1","name":"read",
                "args":{"path":"missing"}},"thoughtSignature":"U0lHLTE="}]}),
            json!({"role":"user","parts":[{"functionResponse":{"id":"c1","name":"read",
                "response":result}}]}),
        ]);
    }
    if turn >= 3 {
        history.extend([
            json!({"role":"model","parts":[{"functionCall":{"id":"c2","name":"read",
                "args":{"path":"found"}},"thoughtSignature":"U0lHLTI="}]}),
            json!({"role":"user","parts":[{"functionResponse":{"id":"c2","name":"read",
                "response":{"output":RESULT}}}]}),
        ]);
    }
    if turn >= 4 {
        history.extend([
            json!({"role":"model","parts":[{"text":RESULT,"thoughtSignature":"U0lHLTM="}]}),
            json!({"role":"user","parts":[{"text":"Continue."}]}),
        ]);
    }
    json!(history)
}
async fn answer(
    State(state): State<Upstream>,
    uri: Uri,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    assert_eq!(
        uri.path(),
        "/v1beta/models/gemini-3.8-flash:streamGenerateContent"
    );
    assert_eq!(uri.query(), Some("alt=sse"));
    assert_eq!(headers["x-goog-api-key"], "synthetic-google-loop-upstream");
    assert!(headers.get("authorization").is_none());
    let turn = match body["contents"].as_array().unwrap().len() {
        1 => 1,
        3 => 2,
        5 => 3,
        7 => 4,
        _ => panic!("unexpected history length"),
    };
    let mut thinking = json!({"thinkingLevel":"LOW"});
    if state.profile == Profile::Responses {
        thinking["includeThoughts"] = json!(true);
    }
    assert_eq!(
        body,
        json!({
            "contents":expected_history(turn, state.messages),
            "tools":[{"functionDeclarations":[{"name":"read","parametersJsonSchema":{
                "type":"object","properties":{"path":{"type":"string"}},"required":["path"]}}]}],
            "generationConfig":{"maxOutputTokens":256,"thinkingConfig":thinking},
        })
    );
    let mut seen = state.seen.lock().unwrap();
    assert!(seen.len() < 8, "no retries or excess dispatch");
    assert_eq!(turn, seen.len() % 4 + 1);
    seen.push(body);
    drop(seen);
    let part = match turn {
        1 => json!({"functionCall":{"id":"c1","name":"read","args":{"path":"missing"}},
            "thoughtSignature":"U0lHLTE="}),
        2 => json!({"functionCall":{"id":"c2","name":"read","args":{"path":"found"}},
            "thoughtSignature":"U0lHLTI="}),
        3 => json!({"text":RESULT,"thoughtSignature":"U0lHLTM="}),
        _ => json!({"text":"continued"}),
    };
    let frame = json!({"responseId":format!("native-{turn}"),"modelVersion":"gemini-3.8-flash",
        "candidates":[{"index":0,"content":{"role":"model","parts":[part]},"finishReason":"STOP"}],
        "usageMetadata":{"promptTokenCount":10,"candidatesTokenCount":3,
            "thoughtsTokenCount":2,"totalTokenCount":15}});
    let bytes = format!("data: {frame}\r\n\r\n").into_bytes();
    Response::builder()
        .header("content-type", "text/event-stream")
        .body(axum::body::Body::from_stream(futures_util::stream::iter(
            bytes
                .chunks(13)
                .map(|chunk| Ok::<_, std::io::Error>(bytes::Bytes::copy_from_slice(chunk)))
                .collect::<Vec<_>>(),
        )))
        .unwrap()
}
pub async fn start(profile: Profile, api: Api) -> Fixture {
    start_with(profile, api, false).await
}
pub async fn start_messages(api: Api) -> Fixture {
    start_with(Profile::Chat, api, true).await
}
async fn start_with(profile: Profile, api: Api, messages: bool) -> Fixture {
    let binding = &GOOGLE_BINDINGS[0];
    let seen = Arc::new(Mutex::new(Vec::new()));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin =
        TrustedOrigin::parse(&format!("http://{}", listener.local_addr().unwrap())).unwrap();
    let mut endpoint = match api {
        Api::GenerateContent => binding.endpoint(),
        Api::Interactions => binding.interactions_endpoint(),
    };
    endpoint.target.origin = origin.clone();
    let mut provider = binding.provider();
    provider.origin = origin;
    let mut route = binding.route();
    // The other Google protocol cannot mask a failed conversion.
    route.endpoints = vec![endpoint.id.clone()];
    route.policy.max_attempts = 1;
    let app = match api {
        Api::GenerateContent => Router::new().route(endpoint.target.path.as_str(), post(answer)),
        Api::Interactions => {
            Router::new().route(endpoint.target.path.as_str(), post(interactions::answer))
        }
    }
    .with_state(Upstream {
        profile,
        messages,
        seen: seen.clone(),
    });
    let upstream = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
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
        TaskEntries {
            generation: vec![Entry {
                chat_profile: Default::default(),
                model: binding.model.into(),
                protocol: profile,
                endpoint: endpoint.id,
            }],
            ..Default::default()
        },
        BTreeMap::from([(
            endpoint.credential,
            Arc::new(SecretMaterial::new("synthetic-google-loop-upstream").unwrap()),
        )]),
        std::collections::BTreeMap::from([(
            "fixture-agent".into(),
            SecretMaterial::new(KEY).unwrap(),
        )]),
        Limits::default(),
        None,
    )
    .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}/v1", listener.local_addr().unwrap());
    let owner = gateway.clone();
    let server = tokio::spawn(async move {
        owner.serve(listener, std::future::pending()).await.unwrap();
    });
    Fixture {
        base,
        seen,
        gateway,
        tasks: vec![server, upstream],
    }
}
