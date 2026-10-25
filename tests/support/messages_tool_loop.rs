//! Selected native Messages route, with independent five-turn upstream wire expectations.
use crate::messages_wire::native_response;
use axum::{Json, Router, extract::State, http::HeaderMap, response::Response, routing::post};
use morphiecore::{
    gateway::{Entry, Gateway, Limits, TaskEntries},
    protocol::openai::Profile,
    provider::{SecretMaterial, TrustedOrigin},
    topology::{self, catalog::MESSAGES_BINDINGS},
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};
use tokio::net::TcpListener;

pub const KEY: &str = "synthetic-messages-client-token-00000001";
pub const BETA: &str = "mid-conversation-output-config-2026-07-01,thinking-binding-controls-2026-08-01,inline-tools-2026-09-15";
pub struct Fixture {
    pub base: String,
    pub seen: Arc<Mutex<Vec<Value>>>,
    pub recovery: Option<Arc<crate::credential_recovery::Recovery>>,
    jobs: Vec<tokio::task::JoinHandle<()>>,
    _store: Option<tempfile::TempDir>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        for job in &self.jobs {
            job.abort();
        }
    }
}

pub fn input(stream: bool) -> Value {
    json!({"model":"claude-haiku-5-5","max_tokens":2048,"stream":stream,
        "thinking":{"type":"adaptive","display":"summarized","block_binding":{"prefix_mismatch_behavior":"drop_block"}},
        "output_config":{"effort":"high"},
        "system":[{"type":"text","text":"Synthetic Messages tool loop.","cache_control":{"type":"ephemeral","ttl":"1h"}}],
        "tools":[
            {"name":"lookup","strict":true,"eager_input_streaming":true,
             "input_schema":{"type":"object","properties":{"key":{"type":"string"}},"required":["key"],"additionalProperties":false},
             "cache_control":{"type":"ephemeral","ttl":"1h"}},
            {"name":"__pi_deferred_placeholder__","input_schema":{"type":"object"},"defer_loading":true}
        ],
        "messages":[{"role":"user","content":[{"type":"text","text":"lookup","cache_control":{"type":"ephemeral","ttl":"1h"}}]},
            {"role":"system","content":[],"output_config":{"effort":"high"}}]})
}

pub fn snapshot(turn: usize) -> Value {
    let content = match turn {
        1 => json!([
            {"type":"thinking","thinking":"plan lookup","signature":"native-lookup-signature"},
            {"type":"redacted_thinking","data":"native-redacted"},
            {"type":"tool_use","id":"call-lookup","name":"lookup","input":{"key":"A"}},
            {"type":"tool_use","id":"call-second","name":"lookup","input":{"key":"B"}}
        ]),
        3 => json!([
            {"type":"thinking","thinking":"plan inspect","signature":"native-inspect-signature"},
            {"type":"tool_use","id":"call-inspect","name":"inspect","input":{}}
        ]),
        2 | 4 | 5 => {
            json!([{"type":"text","text":if turn==2 {"A"}else if turn==4 {"B"}else{"A B"}}])
        }
        _ => panic!("upstream turn"),
    };
    json!({"id":format!("msg-tools-{turn}"),"type":"message","role":"assistant","model":"claude-haiku-5-5",
        "content":content,"stop_reason":if turn==1 || turn==3 {"tool_use"}else{"end_turn"},"stop_sequence":null,
        "usage":{"input_tokens":11,"cache_read_input_tokens":13,"cache_creation_input_tokens":17,
            "cache_creation":{"ephemeral_5m_input_tokens":0,"ephemeral_1h_input_tokens":17},"output_tokens":7}})
}

#[derive(Clone)]
struct StateData {
    seen: Arc<Mutex<Vec<Value>>>,
    recovery: Option<Arc<crate::credential_recovery::Recovery>>,
}

async fn answer(
    State(StateData { seen, recovery }): State<StateData>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    assert_eq!(headers["anthropic-version"], "2026-10-21");
    if recovery.is_none() {
        assert_eq!(headers["x-api-key"], "synthetic-messages-upstream");
    }
    assert!(headers.contains_key("x-opencode-session") && !headers.contains_key("authorization"));
    // Dispatch failures do not advance the semantic history.
    let turn = body["messages"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|message| message["role"] == "user")
        .count();
    assert!((1..=5).contains(&turn));
    let mut expected = json!([
        {"role":"user","content":[{"type":"text","text":"lookup"}]},
        {"role":"system","content":[],"output_config":{"effort":"high"}}
    ]);
    let messages = expected.as_array_mut().unwrap();
    if turn >= 2 {
        messages.extend([
            json!({"role":"assistant","content":[
                {"type":"thinking","thinking":"plan lookup","signature":"native-lookup-signature"},
                {"type":"redacted_thinking","data":"native-redacted"},
                {"type":"tool_use","id":"call-lookup","name":"lookup","input":{"key":"A"}},
                {"type":"tool_use","id":"call-second","name":"lookup","input":{"key":"B"}}
            ]}),
            json!({"role":"user","content":[
                {"type":"tool_result","tool_use_id":"call-second","content":"second-result","is_error":true},
                {"type":"tool_result","tool_use_id":"call-lookup","content":"A","is_error":false}]}),
            json!({"role":"system","content":[],"output_config":{"effort":"high"}}),
        ]);
    }
    if turn >= 3 {
        messages.extend([
            json!({"role":"assistant","content":[{"type":"text","text":"A"}]}),
            json!({"role":"user","content":[{"type":"text","text":"inspect"}]}),
            json!({"role":"system","content":[
                {"type":"text","text":"Inspect mode."},
                {"type":"tool_removal","tool":{"type":"tool_reference","name":"lookup"}},
                {"type":"tool_addition","tool":{"type":"tool_definition","definition":{
                    "name":"inspect","input_schema":{"type":"object"},"eager_input_streaming":true}}}
            ],"output_config":{"effort":"low"}}),
        ]);
    }
    if turn >= 4 {
        messages.extend([
            json!({"role":"assistant","content":[
                {"type":"thinking","thinking":"plan inspect","signature":"native-inspect-signature"},
                {"type":"tool_use","id":"call-inspect","name":"inspect","input":{}}
            ]}),
            json!({"role":"user","content":[{"type":"tool_result","tool_use_id":"call-inspect","content":"B","is_error":true}]}),
            json!({"role":"system","content":[],"output_config":{"effort":"low"}}),
        ]);
    }
    if turn == 5 {
        messages.extend([
            json!({"role":"assistant","content":[{"type":"text","text":"B"}]}),
            json!({"role":"user","content":[{"type":"text","text":"repeat"}]}),
            json!({"role":"system","content":[],"output_config":{"effort":"low"}}),
        ]);
    }
    messages
        .iter_mut()
        .rev()
        .find(|m| m["role"] == "user")
        .unwrap()["content"][0]["cache_control"] = json!({"type":"ephemeral","ttl":"1h"});
    assert_eq!(
        body["messages"], expected,
        "native Messages history turn {turn}"
    );
    // Independent of the client fixture: a changed stimulus must not redefine the oracle.
    let initial = json!({"model":"claude-haiku-5-5","max_tokens":2048,
        "thinking":{"type":"adaptive","display":"summarized",
            "block_binding":{"prefix_mismatch_behavior":"drop_block"}},
        "output_config":{"effort":"high"},
        "system":[{"type":"text","text":"Synthetic Messages tool loop.",
            "cache_control":{"type":"ephemeral","ttl":"1h"}}],
        "tools":[{"name":"lookup","strict":true,"eager_input_streaming":true,
            "input_schema":{"type":"object","properties":{"key":{"type":"string"}},
                "required":["key"],"additionalProperties":false},
            "cache_control":{"type":"ephemeral","ttl":"1h"}},
            {"name":"__pi_deferred_placeholder__","input_schema":{"type":"object"},"defer_loading":true}]
    });
    for key in [
        "model",
        "max_tokens",
        "thinking",
        "output_config",
        "tools",
        "system",
    ] {
        assert_eq!(body[key], initial[key], "native Messages {key}");
    }
    assert!(body.get("cache_control").is_none() && body.get("temperature").is_none());
    let expected_beta = if turn < 3 {
        "mid-conversation-output-config-2026-07-01,thinking-binding-controls-2026-08-01"
    } else {
        BETA
    };
    assert_eq!(headers["anthropic-beta"], expected_beta);
    seen.lock().unwrap().push(body.clone());
    if let Some(recovery) = &recovery
        && let Some(failure) =
            recovery.observe(turn, headers["x-api-key"].to_str().unwrap(), &headers)
    {
        return failure;
    }
    native_response(snapshot(turn), body["stream"] == true)
}

pub async fn start() -> Fixture {
    start_with_recovery(false).await
}

pub async fn start_recovery() -> Fixture {
    start_with_recovery(true).await
}

async fn start_with_recovery(recover: bool) -> Fixture {
    let seen = Arc::new(Mutex::new(vec![]));
    let recovery = recover.then(|| Arc::new(crate::credential_recovery::Recovery::default()));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin =
        TrustedOrigin::parse(&format!("http://{}", listener.local_addr().unwrap())).unwrap();
    let app = Router::new()
        .route("/zen/go/v1/messages", post(answer))
        .with_state(StateData {
            seen: seen.clone(),
            recovery: recovery.clone(),
        });
    let upstream = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let binding = &MESSAGES_BINDINGS[0];
    let mut provider = binding.provider();
    provider.origin = origin.clone();
    let mut endpoint = binding.endpoint();
    endpoint.target.origin = origin;
    let mut public = binding.public_model();
    public.contract = binding.canonical_model().contract;
    let (credentials, store) = if recover {
        let (credentials, store) =
            crate::credential_recovery::pool(provider.id.as_str(), endpoint.credential.clone());
        (credentials, Some(store))
    } else {
        (
            BTreeMap::from([(
                binding.credential(),
                Arc::new(SecretMaterial::new("synthetic-messages-upstream").unwrap()),
            )])
            .into(),
            None,
        )
    };
    let mut route = binding.route();
    if recover {
        route.policy.max_attempts = 2;
    }
    let topology = topology::compile(
        vec![provider],
        vec![endpoint],
        vec![route],
        vec![public],
        vec![binding.canonical_model()],
    )
    .unwrap()
    .with_model_metadata([(binding.canonical_model().id, binding.metadata())])
    .unwrap();
    let gateway = Gateway::new(
        topology,
        TaskEntries {
            generation: vec![Entry {
                chat_profile: Default::default(),
                model: binding.model.into(),
                protocol: Profile::Chat,
                endpoint: binding.endpoint().id,
            }],
            ..Default::default()
        },
        credentials,
        std::collections::BTreeMap::from([(
            "fixture-agent".into(),
            SecretMaterial::new(KEY).unwrap(),
        )]),
        Limits::default(),
        None,
    )
    .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        gateway
            .serve(listener, std::future::pending())
            .await
            .unwrap()
    });
    Fixture {
        base,
        seen,
        recovery,
        jobs: vec![upstream, server],
        _store: store,
    }
}
