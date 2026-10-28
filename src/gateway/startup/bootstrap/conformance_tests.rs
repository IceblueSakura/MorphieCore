//! Production-constructed lanes with pi-shaped requests and independent native session wires.
//! Dispositions are reviewed limitations, not protocol mapping oracles.
use super::test_support::{Fixture, service};
use axum::{
    Json, Router,
    body::{Body, to_bytes},
    http::Request,
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
use wire::{Api, Profile};

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
type Binding = (&'static str, &'static str, &'static str, &'static str, Api);
const BINDINGS: &[Binding] = &[
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
    Incomplete { stored: bool },
}

async fn lane(
    binding: &Binding,
    client: Api,
    profile: Profile,
    stream: bool,
    control: Control,
) -> Vec<String> {
    let &(model, binding, provider, pool, upstream) = binding;
    let calls = Arc::new(Mutex::new(Vec::<Value>::new()));
    let captured = calls.clone();
    let router = Router::new().route(
        upstream.path(),
        post(move |Json(body): Json<Value>| {
            let calls = captured.clone();
            async move {
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
                    matches!(control, Control::Incomplete { .. }) && turn == 0,
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
    let boot = Fixture::new(
        config,
        json!({"models":{model:{"max_attempts":1,"candidates":[{
            "binding":binding,"base_url":origin,"attempt_timeout_ms":2000
        }]}}}),
        &[(provider, pool, "synthetic-upstream-key")],
    )
    .unwrap();
    let mut request = wire::request(client, profile, model, stream);
    let stored = matches!(
        control,
        Control::Stored | Control::Incomplete { stored: true }
    );
    if stored {
        assert_eq!(client, Api::Responses);
        request["store"] = json!(true);
    }
    let mut outcomes = Vec::new();
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
            wire::assert_history(upstream, profile, &calls.lock().unwrap()[1]);
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
        wire::assert_delivery(client, profile, turn, &value);
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
    outcomes
}

#[tokio::test]
async fn agent_default_production_lanes() {
    for binding in BINDINGS {
        for client in [Api::Chat, Api::Responses, Api::Messages] {
            for profile in [Profile::Text, Profile::Tools, Profile::Reasoning] {
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
    for binding in &BINDINGS[..4] {
        for stream in [false, true] {
            let actual = lane(
                binding,
                Api::Messages,
                Profile::Text,
                stream,
                Control::WithoutCache,
            )
            .await;
            // Empty probability reports now pass; the independently scheduled
            // unreported-completion-cause restriction is the remaining boundary.
            let expected = &["codec.unsupported.terminal@1"];
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

fn expected(binding: &str, client: Api, profile: Profile) -> &'static [&'static str] {
    use Api::*;
    use Profile::*;
    const ACCEPT: &[&str] = &["accepted@1", "accepted@1"];
    // Failure code plus dispatch count pins both the cause and pre/post-dispatch boundary.
    match (binding, client, profile) {
        (
            "dashscope-glm-chat"
            | "deepseek-chat"
            | "openrouter-responses"
            | "dashscope-flash-responses",
            Messages,
            _,
        ) => &["admission.explicit_cache@0"],
        ("dashscope-glm-chat", Chat, Text | Tools) => ACCEPT,
        ("dashscope-glm-chat", Chat, Reasoning) => {
            &["planning:[\"codec.unsupported.reasoning\"]@0"]
        }
        ("dashscope-glm-chat", Responses, Text) => ACCEPT,
        ("dashscope-glm-chat", Responses, Tools) => {
            &["planning:[\"codec.unsupported.unavailable\"]@0"]
        }
        ("dashscope-glm-chat", Responses, Reasoning) => {
            &["planning:[\"codec.unsupported.reasoning\"]@0"]
        }
        ("deepseek-chat", Chat, Text | Tools) | ("deepseek-chat", Responses, Text) => ACCEPT,
        ("deepseek-chat", Chat, Reasoning) => &["representation.reasoning@1"],
        ("deepseek-chat", Responses, Tools) => &["planning:[\"representation.strict_default\"]@0"],
        ("deepseek-chat", Responses, Reasoning) => &["planning:[\"representation.reasoning\"]@0"],
        ("openrouter-responses", Chat, Text | Tools) | ("openrouter-responses", Responses, _) => {
            ACCEPT
        }
        ("openrouter-responses" | "dashscope-flash-responses", Chat, Reasoning) => {
            &["representation.reasoning@1"]
        }
        ("dashscope-flash-responses", Chat, Text) => ACCEPT,
        ("dashscope-flash-responses", Chat, Tools)
        | ("dashscope-flash-responses", Responses, Text) => &[
            "accepted@1",
            "planning:[\"codec.unsupported.unavailable\"]@0",
        ],
        ("dashscope-flash-responses", Responses, Tools) => {
            &["planning:[\"codec.unsupported.unavailable\"]@0"]
        }
        ("dashscope-flash-responses", Responses, Reasoning) => {
            &["planning:[\"codec.unsupported.reasoning\"]@0"]
        }
        ("opencode-go-haiku-messages", Chat | Messages, Text | Tools)
        | ("opencode-go-haiku-messages", Responses, Text) => ACCEPT,
        ("opencode-go-haiku-messages", Chat, Reasoning) => {
            &["planning:[\"codec.unsupported.unavailable\"]@0"]
        }
        ("opencode-go-haiku-messages", Responses, Tools | Reasoning) => {
            &["admission.tool_schema_normalization@0"]
        }
        ("opencode-go-haiku-messages", Messages, Reasoning) => {
            &["planning:[\"codec.unsupported.reasoning\"]@0"]
        }
        _ => panic!("unreviewed lane: {binding}/{client:?}/{profile:?}"),
    }
}
