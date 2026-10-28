//! Catalog-backed Chat/Responses Router fixture; protocol wire oracles stay separate.
#[path = "openai_tool_loop/chat.rs"]
mod chat;
#[path = "openai_tool_loop/responses.rs"]
mod responses;
use axum::{
    Json, Router, body::Body, extract::State, http::HeaderMap, response::Response, routing::post,
};
use morphiecore::{
    gateway::{ChatProfile, Entry, Gateway, Limits, TaskEntries},
    protocol::openai::GenerationApi,
    provider::{SecretMaterial, TrustedOrigin},
    topology::{self, catalog},
};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};
use tokio::net::TcpListener;

pub const CLIENT_KEY: &str = "synthetic-tool-history-client-0001";
pub const RESULT: &str = "{\"error\":\"synthetic lookup unavailable\",\"label\":\"cedar-71\"}";
pub struct Fixture {
    pub base: String,
    pub seen: Arc<Mutex<Vec<Value>>>,
    pub recovery: Option<Arc<crate::credential_recovery::Recovery>>,
    pub gateway: Gateway,
    #[allow(dead_code)] // The gateway target owns lifecycle-control assertions.
    pub control: Arc<Control>,
    tasks: Vec<tokio::task::JoinHandle<()>>,
    _store: Option<tempfile::TempDir>,
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
#[derive(Clone)]
struct StateData {
    profile: GenerationApi,
    seen: Arc<Mutex<Vec<Value>>>,
    control: Arc<Control>,
    recovery: Option<Arc<crate::credential_recovery::Recovery>>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        for task in &self.tasks {
            task.abort();
        }
    }
}
fn snapshot(profile: GenerationApi, turn: usize, reasoning: bool) -> Value {
    match profile {
        GenerationApi::Chat => chat::snapshot(turn, reasoning),
        GenerationApi::Responses => responses::snapshot(turn),
    }
}
fn events(profile: GenerationApi, turn: usize, reasoning: bool) -> Vec<Value> {
    match profile {
        GenerationApi::Chat => chat::events(turn, reasoning),
        GenerationApi::Responses => responses::events(turn),
    }
}
async fn answer(
    State(StateData {
        profile,
        seen,
        control,
        recovery,
    }): State<StateData>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    let secret = headers["authorization"]
        .to_str()
        .unwrap()
        .strip_prefix("Bearer ")
        .unwrap();
    if recovery.is_none() {
        assert!(matches!(
            secret,
            "synthetic-tool-history-upstream" | "synthetic-tool-history-secondary"
        ));
    }
    for field in [
        "temperature",
        "top_p",
        "parallel_tool_calls",
        "top_logprobs",
    ] {
        assert!(body.get(field).is_none(), "unexpected {field}");
    }
    let turn = match profile {
        GenerationApi::Chat => chat::assert_request(&body),
        GenerationApi::Responses => responses::assert_request(&body),
    };
    let chat = profile == GenerationApi::Chat;
    let history = body[if chat { "messages" } else { "input" }]
        .as_array()
        .unwrap();
    seen.lock().unwrap().push(body.clone());
    if let Some(recovery) = &recovery
        && let Some(failure) = recovery.observe(turn, secret, &headers)
    {
        return failure;
    }
    let reasoning = history[0]["content"] == "reasoning";
    let mode = history[0]["content"]
        .as_str()
        .or_else(|| history[0]["content"][0]["text"].as_str())
        .unwrap();
    let incomplete = !chat && mode == "incomplete" && turn == 1;
    let stream = body["stream"] == true;
    let bytes = if stream {
        let mut bytes = Vec::new();
        for mut event in events(profile, turn, reasoning) {
            if incomplete {
                if event["type"] == "response.output_item.done"
                    && event["item"]["type"] == "reasoning"
                {
                    event["item"]["status"] = serde_json::json!("incomplete");
                }
                if event["type"] == "response.completed" {
                    event["type"] = serde_json::json!("response.incomplete");
                    responses::make_incomplete(&mut event["response"]);
                }
            }
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
        let mut value = snapshot(profile, turn, reasoning);
        if incomplete {
            responses::make_incomplete(&mut value);
        }
        serde_json::to_vec(&value).unwrap()
    };
    if matches!(mode, "hold" | "cut" | "cancel" | "terminal-cut") {
        let cut = matches!(mode, "cut" | "terminal-cut");
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
pub async fn start(profile: GenerationApi, fallback: bool) -> Fixture {
    start_with_recovery(profile, fallback, false, ChatProfile::Standard, false).await
}

pub async fn start_recovery() -> Fixture {
    start_with_recovery(
        GenerationApi::Chat,
        false,
        true,
        ChatProfile::Standard,
        false,
    )
    .await
}

pub async fn start_readable() -> Fixture {
    start_with_recovery(
        GenerationApi::Chat,
        false,
        false,
        ChatProfile::ReadableReasoning,
        true,
    )
    .await
}

#[allow(dead_code)] // Only the Router target exercises pre-dispatch rejection.
pub async fn start_standard_thinking() -> Fixture {
    start_with_recovery(
        GenerationApi::Chat,
        false,
        false,
        ChatProfile::Standard,
        true,
    )
    .await
}

async fn start_with_recovery(
    profile: GenerationApi,
    fallback: bool,
    recover: bool,
    chat_profile: ChatProfile,
    declared_thinking: bool,
) -> Fixture {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let control = Arc::new(Control::default());
    let recovery = recover.then(|| Arc::new(crate::credential_recovery::Recovery::default()));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin =
        TrustedOrigin::parse(&format!("http://{}", listener.local_addr().unwrap())).unwrap();
    let app = Router::new()
        .route("/compatible-mode/v1/chat/completions", post(answer))
        .route("/api/v1/responses", post(answer))
        .with_state(StateData {
            profile,
            seen: seen.clone(),
            control: control.clone(),
            recovery: recovery.clone(),
        });
    let upstream = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let catalog = catalog::default_topology().unwrap();
    let mut endpoints = Vec::new();
    let mut entries = Vec::new();
    let mut credentials = BTreeMap::new();
    let (label, id) = match profile {
        GenerationApi::Chat => ("qwen3.8-flash", "dashscope-flash-chat"),
        GenerationApi::Responses => ("gpt-6-luna", "openrouter-responses"),
    };
    let mut endpoint = catalog
        .endpoint(&topology::EndpointId::new(id).unwrap())
        .unwrap()
        .clone();
    let mut provider = catalog.endpoint_provider(&endpoint).unwrap().clone();
    provider.origin = origin.clone();
    endpoint.target.origin = origin.clone();
    // The generic synthetic fixture can return either shape, independently of
    // the real binding's declared thinking-output contract.
    endpoint
        .representation
        .openai_mut()
        .unwrap()
        .chat_thinking_output = declared_thinking;
    let public = catalog.model(label).unwrap().clone();
    let mut route = catalog.route(&public.route).unwrap().clone();
    route.endpoints = vec![endpoint.id.clone()];
    route.policy.max_attempts = if recover { 2 } else { 1 };
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
            chat_profile,
            model: label.into(),
            protocol: profile.into(),
            endpoint: secondary.id.clone(),
        });
        credentials.insert(
            secondary.credential.clone(),
            Arc::new(SecretMaterial::new("synthetic-tool-history-secondary").unwrap()),
        );
        endpoints.push(secondary);
    }
    let canonical = topology::CanonicalModel {
        id: public.canonical_model.clone(),
        task: public.task,
        contract: public.contract.clone(),
    };
    entries.push(Entry {
        chat_profile,
        model: label.into(),
        protocol: profile.into(),
        endpoint: endpoint.id.clone(),
    });
    credentials.insert(
        endpoint.credential.clone(),
        Arc::new(SecretMaterial::new("synthetic-tool-history-upstream").unwrap()),
    );
    endpoints.push(endpoint);
    let metadata = [(
        public.id.clone(),
        topology::ModelMetadata::new(1, "Synthetic tool history").unwrap(),
    )];
    let (credentials, store) = if recover {
        let (credentials, store) = crate::credential_recovery::pool(
            provider.id.as_str(),
            endpoints.last().unwrap().credential.clone(),
        );
        (credentials, Some(store))
    } else {
        (credentials.into(), None)
    };
    let topology = topology::compile(
        vec![provider],
        endpoints,
        vec![route],
        vec![public],
        vec![canonical],
    )
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
        std::collections::BTreeMap::from([(
            "fixture-agent".into(),
            SecretMaterial::new(CLIENT_KEY).unwrap(),
        )]),
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
        recovery,
        gateway,
        control,
        tasks: vec![server, upstream],
        _store: store,
    }
}
