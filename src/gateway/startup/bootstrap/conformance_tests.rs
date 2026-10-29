//! Production-constructed lanes with pi-shaped requests and independent native session wires.
//! Dispositions are reviewed limitations, not protocol mapping oracles.
use super::test_support::{Fixture, service};
use axum::{
    Json, Router,
    body::{Body, to_bytes},
    http::{HeaderMap, Request},
    response::IntoResponse,
    routing::post,
};
use futures_util::StreamExt;
use serde_json::{Value, json};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use tower::ServiceExt;

mod wire;
use wire::{Api, Profile, ReasoningShape};

const KEY: &str = "synthetic-gateway-client-token-0001";
const LIMIT: usize = 1 << 20;
const DEADLINE: Duration = Duration::from_secs(5);

struct Server(tokio::task::JoinHandle<()>);
impl Drop for Server {
    fn drop(&mut self) {
        self.0.abort();
    }
}

async fn send(
    boot: &Fixture,
    api: Api,
    body: Value,
    headers: &[(&str, &str)],
) -> (u16, Vec<u8>, bool) {
    let mut request = Request::builder()
        .method("POST")
        .uri(api.path())
        .header("authorization", format!("Bearer {KEY}"))
        .header("content-type", "application/json");
    for &(name, value) in headers {
        request = request.header(name, value);
    }
    let response = tokio::time::timeout(
        DEADLINE,
        boot.gateway
            .router()
            .oneshot(request.body(Body::from(body.to_string())).unwrap()),
    )
    .await
    .unwrap()
    .unwrap();
    let status = response.status().as_u16();
    let (bytes, failed) = tokio::time::timeout(DEADLINE, async {
        let mut frames = response.into_body().into_data_stream();
        let mut bytes = Vec::new();
        while let Some(frame) = frames.next().await {
            match frame {
                Ok(frame) => {
                    assert!(bytes.len() + frame.len() <= LIMIT, "downstream body bound");
                    bytes.extend_from_slice(&frame);
                }
                Err(_) => return (bytes, true),
            }
        }
        (bytes, false)
    })
    .await
    .unwrap();
    (status, bytes, failed)
}

async fn report(boot: &Fixture, ordinal: usize) -> Value {
    // Trace destruction follows body/worker completion. Wait for its record, not a sleep.
    tokio::time::timeout(DEADLINE, async {
        loop {
            boot.gateway.flush_diagnostics().await;
            let records: Vec<Value> =
                std::fs::read_to_string(boot.directory().join("events.jsonl"))
                    .unwrap()
                    .lines()
                    .map(|l| serde_json::from_str(l).unwrap())
                    .filter(|v: &Value| v["event"] == "request")
                    .collect();
            if records.len() == ordinal {
                return records.last().unwrap().clone();
            }
            assert!(records.len() < ordinal);
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("completed request diagnostic")
}

fn disposition(record: &Value) -> String {
    let generation = &record["generation"];
    if generation["result"] == "succeeded" {
        return "accepted".into();
    }
    if let Some(code) = generation["failure"]["code"].as_str() {
        return code.into();
    }
    if let Some(reasons) = generation["planning_failure"]["reasons"].as_array() {
        return format!("planning:{}", serde_json::to_string(reasons).unwrap());
    }
    if let Some(attempt) = generation["attempts"].as_array().and_then(|a| a.last()) {
        for key in ["failure", "handling_failure"] {
            if let Some(code) = attempt[key]["code"].as_str() {
                return code.into();
            }
        }
    }
    generation["failure_code"]
        .as_str()
        .unwrap_or_else(|| panic!("missing failure: {record}"))
        .into()
}

// The initially probed bindings cover standard, Bailian, DeepSeek and native Messages.
type Binding<'a> = (&'a str, &'a str, &'a str, &'a str, Api);
const BINDINGS: &[Binding<'static>] = &[
    (
        "glm-5.3",
        "dashscope-glm-chat",
        "aliyun-dashscope-cn",
        "aliyun-dashscope-cn-api-key",
        Api::Chat,
    ),
    (
        "deepseek-flash",
        "deepseek-chat",
        "deepseek",
        "deepseek-api-key",
        Api::Chat,
    ),
    (
        "gpt-6-luna",
        "openrouter-responses",
        "openrouter",
        "openrouter-api-key",
        Api::Responses,
    ),
    (
        "qwen3.8-flash",
        "dashscope-flash-responses",
        "aliyun-dashscope-cn",
        "aliyun-dashscope-cn-api-key",
        Api::Responses,
    ),
    (
        "claude-haiku-5-5",
        "opencode-go-haiku-messages",
        "opencode-go",
        "opencode-go-api-key",
        Api::Messages,
    ),
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Control {
    Default,
    Stored,
    WithoutCache,
    LegacyVersion,
    ThinkingBeta,
    Uncapped,
    SummaryHistory,
    OpaqueHistory,
    ReadableHistory,
    AdaptiveHistory,
    Incomplete { stored: bool },
    ReportedFacts { refusal: bool, stored: bool },
}
impl Control {
    fn reasoning(self) -> Option<ReasoningShape> {
        match self {
            Self::SummaryHistory => Some(ReasoningShape::Summary),
            Self::OpaqueHistory => Some(ReasoningShape::Opaque),
            Self::ReadableHistory => Some(ReasoningShape::Readable),
            Self::AdaptiveHistory => Some(ReasoningShape::Signed),
            _ => None,
        }
    }
}

async fn lane(
    binding: &Binding<'_>,
    client: Api,
    profile: Profile,
    stream: bool,
    control: Control,
) -> Vec<String> {
    observe(binding, client, profile, stream, control).await.0
}

async fn transports(
    binding: &Binding<'_>,
    client: Api,
    profile: Profile,
    control: Control,
) -> Vec<String> {
    let json = observe(binding, client, profile, false, control).await;
    let sse = observe(binding, client, profile, true, control).await;
    assert_eq!(
        json, sse,
        "{}/{client:?}/{profile:?}: JSON/SSE agreement",
        binding.1
    );
    json.0
}

async fn observe(
    binding: &Binding<'_>,
    client: Api,
    profile: Profile,
    stream: bool,
    control: Control,
) -> (Vec<String>, Vec<Value>) {
    let &(model, binding, provider, pool, upstream) = binding;
    let subscription = crate::topology::catalog::SUBSCRIPTION_BINDINGS
        .iter()
        .find(|entry| entry.endpoint_id().as_str() == binding);
    let oauth = subscription.map(|entry| entry.profile);
    let bailian = crate::topology::catalog::API_KEY_BINDINGS
        .iter()
        .any(|entry| {
            entry.model == model
                && entry.dialect == crate::protocol::openai::Dialect::Bailian
                && (entry.provider)().id.as_str() == provider
        });
    let parameters = (client == Api::Responses
        && profile != Profile::Text
        && upstream != Api::Messages)
        .then(|| {
            wire::request(client, profile, model, stream)["tools"]
                .as_array()
                .unwrap()
                .iter()
                .map(|tool| tool["parameters"].clone())
                .collect::<Vec<_>>()
        });
    let calls = Arc::new(Mutex::new(Vec::<Value>::new()));
    let captured = calls.clone();
    let router = Router::new().route(
        upstream.path(),
        post(move |headers: HeaderMap, Json(body): Json<Value>| {
            let calls = captured.clone();
            let parameters = parameters.clone();
            async move {
                assert_eq!(
                    headers[if upstream == Api::Messages {
                        "x-api-key"
                    } else {
                        "authorization"
                    }],
                    if upstream == Api::Messages {
                        "synthetic-upstream-key"
                    } else if oauth.is_some() {
                        "Bearer synthetic-oauth-access"
                    } else {
                        "Bearer synthetic-upstream-key"
                    }
                );
                if oauth == Some("openai") {
                    assert_eq!(body["stream"], true, "SIWC always streams upstream");
                    assert_eq!(body["store"], false, "upstream stays stateless");
                }
                if control == Control::SummaryHistory {
                    assert_eq!(body["reasoning"], json!({"effort":"low"}));
                    assert!(body.get("include").is_none());
                }
                if let Some(parameters) = parameters {
                    let tools = body["tools"].as_array().unwrap();
                    assert_eq!(tools.len(), parameters.len());
                    for (tool, parameters) in tools.iter().zip(parameters) {
                        let tool = if upstream == Api::Chat {
                            &tool["function"]
                        } else {
                            tool
                        };
                        assert_eq!(tool["parameters"], parameters);
                        if upstream == Api::Chat && !bailian {
                            assert_eq!(tool["strict"], false);
                        } else {
                            assert!(tool.get("strict").is_none());
                        }
                    }
                }
                let turn = {
                    let mut calls = calls.lock().unwrap();
                    let turn = calls.len();
                    assert!(turn < 2, "no hidden retries");
                    calls.push(body.clone());
                    turn
                };
                let bytes = wire::upstream(
                    upstream,
                    profile,
                    turn,
                    body["stream"] == true,
                    body["model"].as_str().unwrap(),
                    control,
                );
                let content_type = if body["stream"] == true {
                    "text/event-stream"
                } else {
                    "application/json"
                };
                ([("content-type", content_type)], bytes).into_response()
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}/v1", listener.local_addr().unwrap());
    let _server = Server(tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    }));
    let mut config = service();
    config["diagnostics"] = json!("events.jsonl");
    config["concurrency"] = json!(1);
    config["exchange_timeout_ms"] = json!(3000);
    config["delta_coalescing"] = Value::Null;
    let mut models = json!({"models":{model:{"max_attempts":1,"candidates":[{
        "binding":binding,"base_url":origin,"attempt_timeout_ms":2000
    }]}}});
    if control == Control::ReadableHistory {
        models["models"][model]["chat_profile"] = json!("readable_reasoning");
    }
    let boot = if let Some(profile) = oauth {
        Fixture::with_oauth(config, models, profile, pool)
    } else {
        Fixture::new(
            config,
            models,
            &[(provider, pool, "synthetic-upstream-key")],
        )
    }
    .unwrap();
    let mut request = wire::request(client, profile, model, stream);
    if control == Control::Uncapped {
        assert_eq!(client, Api::Responses);
        request.as_object_mut().unwrap().remove("max_output_tokens");
    }
    if control == Control::SummaryHistory {
        request["reasoning"] = json!({"effort":"low","summary":"auto"});
        request["include"] = json!(["reasoning.encrypted_content"]);
    }
    if control == Control::AdaptiveHistory {
        assert_eq!(client, Api::Messages);
        request["thinking"] = json!({"type":"adaptive","display":"summarized"});
        request["output_config"] = json!({"effort":"medium"});
    }
    let stored = matches!(
        control,
        Control::Stored
            | Control::SummaryHistory
            | Control::OpaqueHistory
            | Control::Incomplete { stored: true }
            | Control::ReportedFacts { stored: true, .. }
    );
    if stored {
        assert_eq!(client, Api::Responses);
        request["store"] = json!(true);
    }
    let mut outcomes = Vec::new();
    let mut views = Vec::new();
    for turn in 0..2 {
        if control == Control::WithoutCache {
            assert_eq!(client, Api::Messages);
            wire::without_cache(&mut request);
        }
        let before = calls.lock().unwrap().len();
        let raw_headers = wire::raw_messages_headers();
        let headers = [
            (
                "anthropic-version",
                if control == Control::LegacyVersion {
                    raw_headers.0.as_str()
                } else {
                    "2026-10-21"
                },
            ),
            (
                "anthropic-beta",
                if control == Control::ThinkingBeta {
                    raw_headers.1.as_str()
                } else {
                    ""
                },
            ),
        ];
        let (status, bytes, failed) = send(&boot, client, request.clone(), &headers).await;
        let record = report(&boot, turn + 1).await;
        let outcome = disposition(&record);
        let after = calls.lock().unwrap().len();
        assert!(after == before || after == before + 1);
        outcomes.push(format!("{outcome}@{}", after - before));
        if outcome != "accepted" {
            wire::assert_failure(client, stream, status, &bytes, failed);
            if matches!(control, Control::LegacyVersion | Control::ThinkingBeta) {
                assert_eq!(status, 400);
                assert_eq!(
                    serde_json::from_slice::<Value>(&bytes).unwrap()["error"]["type"],
                    "invalid_request_error"
                );
            }
            break;
        }
        assert_eq!(status, 200);
        assert_eq!(after, before + 1);
        assert_eq!(record["generation"]["attempts"][0]["source_eof"], true);
        assert_eq!(
            record["generation"]["attempts"][0]["delivery_complete"],
            true
        );
        assert!(!failed, "successful diagnostic with aborted client body");
        if turn == 1 {
            wire::assert_history(
                upstream,
                profile,
                &calls.lock().unwrap()[1],
                control.reasoning(),
            );
            if matches!(control, Control::Incomplete { .. }) {
                let calls = calls.lock().unwrap();
                let item = calls[1]["input"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|i| i["type"] == "reasoning")
                    .unwrap();
                assert_eq!(item["status"], "incomplete");
            }
        }
        let value = wire::delivered(client, &bytes, stream);
        wire::assert_delivery(client, profile, turn, &value, control.reasoning());
        views.push(wire::delivery_view(client, &value));
        if let Control::ReportedFacts { refusal, .. } = control {
            if client == Api::Messages {
                assert_eq!(value["usage"]["service_tier"], "standard");
                assert_eq!(
                    value["stop_reason"],
                    if refusal { "refusal" } else { "end_turn" }
                );
                if refusal {
                    assert_eq!(value["stop_details"]["category"], "cyber");
                }
            } else {
                assert!(value["service_tier"].is_null());
                assert!(value.get("stop_details").is_none());
                if client == Api::Chat {
                    assert_eq!(
                        value["choices"][0]["finish_reason"],
                        if refusal { "content_filter" } else { "stop" }
                    );
                } else {
                    assert_eq!(
                        value["status"],
                        if refusal { "incomplete" } else { "completed" }
                    );
                    if refusal {
                        assert_eq!(value["incomplete_details"]["reason"], "content_filter");
                    }
                }
            }
        }
        if turn == 0 && matches!(control, Control::Incomplete { .. }) {
            assert_eq!(value["status"], "incomplete");
            assert_eq!(value["incomplete_details"]["reason"], "max_output_tokens");
            let item = value["output"]
                .as_array()
                .unwrap()
                .iter()
                .find(|i| i["type"] == "reasoning")
                .unwrap();
            assert_eq!(item["status"], "incomplete");
        }
        if stored {
            let response = tokio::time::timeout(
                DEADLINE,
                boot.gateway.router().oneshot(
                    Request::builder()
                        .uri(format!("/v1/responses/{}", value["id"].as_str().unwrap()))
                        .header("authorization", format!("Bearer {KEY}"))
                        .body(Body::empty())
                        .unwrap(),
                ),
            )
            .await
            .unwrap()
            .unwrap();
            assert_eq!(response.status(), 200);
            let bytes = tokio::time::timeout(DEADLINE, to_bytes(response.into_body(), LIMIT))
                .await
                .unwrap()
                .unwrap();
            let mut retained: Value = serde_json::from_slice(&bytes).unwrap();
            let mut delivered = value.clone();
            retained.as_object_mut().unwrap().remove("morphiecore");
            delivered.as_object_mut().unwrap().remove("morphiecore");
            assert_eq!(retained, delivered, "retained/delivered agreement");
        }
        wire::append(client, profile, turn, &mut request, value);
    }
    boot.gateway.shutdown();
    (outcomes, views)
}

#[tokio::test]
async fn native_reports_project_without_changing_content_or_terminal_outcome() {
    let binding = BINDINGS
        .iter()
        .find(|binding| binding.4 == Api::Messages)
        .unwrap();
    for client in [Api::Chat, Api::Responses, Api::Messages] {
        for refusal in [false, true] {
            for stream in [false, true] {
                for stored in [false, true] {
                    if stored && client != Api::Responses {
                        continue;
                    }
                    let actual = lane(
                        binding,
                        client,
                        Profile::Text,
                        stream,
                        Control::ReportedFacts { refusal, stored },
                    )
                    .await;
                    assert_eq!(
                        actual, ACCEPT,
                        "reports {client:?}/{refusal}/{stream}/{stored}"
                    );
                }
            }
        }
    }
}

#[tokio::test]
async fn catalog_basic_production_lanes() {
    use crate::topology::{GenerationProtocol, catalog::API_KEY_BINDINGS};
    let mut covered = std::collections::BTreeSet::new();
    for entry in API_KEY_BINDINGS {
        let provider = (entry.provider)();
        let credential = entry.credential();
        for protocol in entry.protocols {
            let endpoint = entry.endpoint(*protocol).unwrap();
            let upstream = match protocol {
                GenerationProtocol::OpenAiChat => Api::Chat,
                GenerationProtocol::OpenAiResponses => Api::Responses,
                _ => unreachable!(),
            };
            let binding = (
                entry.model,
                endpoint.id.as_str(),
                provider.id.as_str(),
                credential.as_str(),
                upstream,
            );
            covered.insert(endpoint.id.as_str().to_owned());
            for client in [Api::Chat, Api::Responses, Api::Messages] {
                for profile in [Profile::Text, Profile::Tools] {
                    let actual = transports(&binding, client, profile, Control::Default).await;
                    assert_eq!(
                        actual,
                        basic_expected(binding.1, client, profile),
                        "{}/{client:?}/{profile:?}",
                        binding.1
                    );
                }
            }
        }
    }
    let reviewed = BASIC_EXPECTATIONS
        .iter()
        .flat_map(|(ids, _)| ids.iter().map(|id| (*id).to_owned()))
        .collect();
    assert_eq!(
        covered, reviewed,
        "reviewed dispositions must match catalog membership"
    );
}

#[tokio::test]
async fn catalog_subscription_production_lanes() {
    use crate::topology::catalog::SUBSCRIPTION_BINDINGS;
    let mut subscriptions = Vec::new();
    for entry in SUBSCRIPTION_BINDINGS {
        let provider = (entry.provider)();
        let endpoint = entry.endpoint_id();
        let credential = entry.credential();
        let binding = (
            entry.model,
            endpoint.as_str(),
            provider.id.as_str(),
            credential.as_str(),
            Api::Responses,
        );
        subscriptions.push(endpoint.as_str().to_owned());
        for client in [Api::Chat, Api::Responses, Api::Messages] {
            for profile in [Profile::Text, Profile::Tools, Profile::Reasoning] {
                // Common protocol controls, not each subscription's emitted reasoning shape.
                let actual = transports(&binding, client, profile, Control::Default).await;
                let expected = if entry.profile == "openai" && entry.model != "gpt-6-luna" {
                    &["admission.max_output_tokens@0"][..]
                } else if client == Api::Messages {
                    &["admission.explicit_cache@0"][..]
                } else if entry.profile == "openai" {
                    &["planning:[\"codec.unsupported.unavailable\"]@0"][..]
                } else if profile == Profile::Reasoning && client == Api::Chat {
                    &["representation.replay_format@1"][..]
                } else {
                    ACCEPT
                };
                assert_eq!(actual, expected, "{}/{client:?}/{profile:?}", binding.1);
            }
        }
        if entry.profile == "openai" {
            assert_eq!(
                transports(&binding, Api::Responses, Profile::Text, Control::Uncapped).await,
                ACCEPT,
                "explicitly uncapped subscription control: {}",
                binding.1
            );
        }
    }
    assert_eq!(
        subscriptions,
        [
            "openai-responses",
            "openai-astra-responses",
            "openai-luna-responses",
            "grok-responses"
        ]
    );
}

#[tokio::test]
async fn native_reasoning_history_contracts_are_distinct() {
    for (id, profile, control) in [
        (
            "dashscope-flash-responses",
            Profile::Text,
            Control::SummaryHistory,
        ),
        (
            "deepseek-chat",
            Profile::Reasoning,
            Control::ReadableHistory,
        ),
        (
            "opencode-go-haiku-messages",
            Profile::Reasoning,
            Control::AdaptiveHistory,
        ),
    ] {
        let binding = BINDINGS.iter().find(|b| b.1 == id).unwrap();
        assert_eq!(
            transports(binding, binding.4, profile, control).await,
            ACCEPT,
            "{id}"
        );
    }
    let grok = (
        "grok-4.7",
        "grok-responses",
        "grok",
        "grok-oauth",
        Api::Responses,
    );
    // Grok may return an opaque reasoning owner even without a reasoning request.
    assert_eq!(
        transports(&grok, Api::Responses, Profile::Text, Control::OpaqueHistory).await,
        ACCEPT
    );
}

#[tokio::test]
async fn agent_default_production_lanes() {
    for binding in BINDINGS {
        for client in [Api::Chat, Api::Responses, Api::Messages] {
            for profile in [Profile::Text, Profile::Tools, Profile::Reasoning] {
                // All API-key text/tool lanes are covered by the catalog gate.
                if profile != Profile::Reasoning && binding.4 != Api::Messages {
                    continue;
                }
                for stream in [false, true] {
                    let actual = lane(binding, client, profile, stream, Control::Default).await;
                    assert_eq!(
                        actual,
                        expected(binding.1, client, profile),
                        "{}/{client:?}/{profile:?}/{stream}",
                        binding.1
                    );
                }
            }
        }
    }
}

#[tokio::test]
async fn stored_production_responses_agree_with_delivered_history() {
    for binding in BINDINGS {
        for profile in [Profile::Text, Profile::Tools, Profile::Reasoning] {
            for stream in [false, true] {
                let actual = lane(binding, Api::Responses, profile, stream, Control::Stored).await;
                assert_eq!(
                    actual,
                    expected(binding.1, Api::Responses, profile),
                    "stored {}/{profile:?}/{stream}",
                    binding.1
                );
            }
        }
    }
}

#[tokio::test]
async fn messages_delivery_controls_reach_post_dispatch_boundaries() {
    // Explicitly not agent-default requests: isolate delivery from the known
    // owner-local-cache admission blocker, without changing target contracts.
    for binding in BINDINGS.iter().filter(|binding| binding.4 != Api::Messages) {
        for stream in [false, true] {
            let actual = lane(
                binding,
                Api::Messages,
                Profile::Text,
                stream,
                Control::WithoutCache,
            )
            .await;
            let expected = &["accepted@1", "accepted@1"];
            assert_eq!(actual, expected, "delivery {}/{stream}", binding.1);
        }
    }
}

#[tokio::test]
async fn installed_messages_headers_reject_before_dispatch() {
    let binding = BINDINGS.iter().find(|b| b.4 == Api::Messages).unwrap();
    for control in [Control::LegacyVersion, Control::ThinkingBeta] {
        assert_eq!(
            lane(binding, Api::Messages, Profile::Text, true, control).await,
            ["gateway.invalid_request@0"]
        );
    }
}

#[tokio::test]
async fn incomplete_responses_reasoning_keeps_its_delivery_and_replay_status() {
    let binding = BINDINGS
        .iter()
        .find(|b| b.1 == "openrouter-responses")
        .unwrap();
    for stored in [false, true] {
        for stream in [false, true] {
            let actual = lane(
                binding,
                Api::Responses,
                Profile::Reasoning,
                stream,
                Control::Incomplete { stored },
            )
            .await;
            assert_eq!(actual, ["accepted@1", "accepted@1"]);
        }
    }
}

// Explicitly reviewed basic dispositions; columns are Chat text/tools and Responses text/tools.
// These groups describe observed admission/delivery, not interchangeable reasoning contracts.
const ACCEPT: &[&str] = &["accepted@1", "accepted@1"];
const BASIC_EXPECTATIONS: &[(&[&str], [&[&str]; 4])] = &[
    (
        &[
            "xiaomi-ultraspeed-responses",
            "opencode-go-muse-responses",
            "deepseek-responses",
            "xiaomi-responses",
            "xiaomi-flash-responses",
            "openrouter-responses",
            "nvidia-responses",
            "zhipu-responses",
            "zhipu-flash-responses",
            "openrouter-glm-5.3-flash-responses",
        ],
        [ACCEPT, ACCEPT, ACCEPT, ACCEPT],
    ),
    (
        &[
            "xiaomi-ultraspeed-chat",
            "opencode-go-mimo-pro-chat",
            "opencode-go-mimo-flash-chat",
            "openrouter-audio-chat",
            "xiaomi-chat",
            "xiaomi-flash-chat",
            "openrouter-chat",
            "nvidia-chat",
            "modelbest-minicpm5-1b-chat",
            "modelbest-minicpm5-2b-chat",
            "modelbest-minicpm-v-4.7-1b-chat",
            "modelbest-minicpm-v-4.7-35b-a3b-chat",
            "opencode-go-chat",
            "openrouter-glm-5.3-flash-chat",
        ],
        [ACCEPT, ACCEPT, ACCEPT, ACCEPT],
    ),
    (
        &["deepseek-chat", "zhipu-chat", "zhipu-flash-chat"],
        [
            ACCEPT,
            &["planning:[\"admission.client_representation\"]@0"],
            ACCEPT,
            ACCEPT,
        ],
    ),
    (
        &[
            "dashscope-flash-responses",
            "tokenplan-max-responses",
            "aliyun-dashscope-cn-responses",
            "aliyun-tokenplan-cn-responses",
        ],
        [
            ACCEPT,
            &[
                "accepted@1",
                "planning:[\"codec.unsupported.unavailable\"]@0",
            ],
            &[
                "accepted@1",
                "planning:[\"codec.unsupported.unavailable\"]@0",
            ],
            &[
                "accepted@1",
                "planning:[\"codec.unsupported.unavailable\"]@0",
            ],
        ],
    ),
    (
        &["tokenplan-max-chat", "aliyun-tokenplan-cn-chat"],
        [
            &["planning:[\"admission.client_representation\"]@0"],
            &["planning:[\"admission.client_representation\"]@0"],
            ACCEPT,
            ACCEPT,
        ],
    ),
    (
        &["dashscope-flash-chat", "aliyun-dashscope-cn-chat"],
        [ACCEPT, ACCEPT, ACCEPT, ACCEPT],
    ),
    (
        &[
            "dashscope-kimi-chat",
            "dashscope-glm-chat",
            "dashscope-glm-prime-chat",
            "tokenplan-glm-chat",
        ],
        [ACCEPT, ACCEPT, ACCEPT, ACCEPT],
    ),
    (
        &["openrouter-haiku-chat"],
        [
            ACCEPT,
            ACCEPT,
            ACCEPT,
            &["admission.conditional_tool_strictness@0"],
        ],
    ),
];

fn basic_expected(binding: &str, client: Api, profile: Profile) -> &'static [&'static str] {
    let columns = BASIC_EXPECTATIONS
        .iter()
        .find_map(|(ids, columns)| ids.contains(&binding).then_some(columns))
        .unwrap_or_else(|| panic!("unreviewed catalog binding: {binding}"));
    let index = match (client, profile) {
        (Api::Chat, Profile::Text) => 0,
        (Api::Chat, Profile::Tools) => 1,
        (Api::Responses, Profile::Text) => 2,
        (Api::Responses, Profile::Tools) => 3,
        (Api::Messages, Profile::Text | Profile::Tools) => return &["admission.explicit_cache@0"],
        _ => panic!("basic gate cannot assert a reasoning contract"),
    };
    columns[index]
}

fn expected(binding: &str, client: Api, profile: Profile) -> &'static [&'static str] {
    use Api::*;
    use Profile::*;
    if profile != Reasoning && binding != "opencode-go-haiku-messages" {
        return basic_expected(binding, client, profile);
    }
    match (binding, client, profile) {
        (_, Messages, _) if binding != "opencode-go-haiku-messages" => {
            &["admission.explicit_cache@0"]
        }
        ("dashscope-glm-chat", Chat | Responses, Reasoning) => {
            &["planning:[\"codec.unsupported.reasoning\"]@0"]
        }
        ("deepseek-chat", Chat, Reasoning) => &["planning:[\"admission.client_representation\"]@0"],
        ("deepseek-chat", Responses, Reasoning) => &["planning:[\"representation.reasoning\"]@0"],
        ("openrouter-responses", Responses, Reasoning) => ACCEPT,
        ("openrouter-responses" | "dashscope-flash-responses", Chat, Reasoning) => {
            &["representation.replay_format@1"]
        }
        ("dashscope-flash-responses", Responses, Reasoning) => &[
            "accepted@1",
            "planning:[\"codec.unsupported.unavailable\"]@0",
        ],
        ("opencode-go-haiku-messages", Chat | Messages, Text | Tools)
        | ("opencode-go-haiku-messages", Responses, Text) => ACCEPT,
        ("opencode-go-haiku-messages", Chat, Reasoning) => {
            &["planning:[\"codec.unsupported.unavailable\"]@0"]
        }
        ("opencode-go-haiku-messages", Responses, Tools | Reasoning) => {
            &["admission.conditional_tool_strictness@0"]
        }
        ("opencode-go-haiku-messages", Messages, Reasoning) => {
            &["planning:[\"codec.unsupported.reasoning\"]@0"]
        }
        _ => panic!("unreviewed lane: {binding}/{client:?}/{profile:?}"),
    }
}
