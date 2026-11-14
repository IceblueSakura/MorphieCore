//! Production-constructed lanes with pi-shaped requests and independent native session wires.
//! Public delivery and failure diagnostics use independent upstream observations.
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

fn disposition(
    exporter: &opentelemetry_sdk::logs::InMemoryLogExporter,
    status: u16,
    failed: bool,
) -> String {
    use opentelemetry::logs::AnyValue;
    for log in exporter.get_emitted_logs().unwrap() {
        if log.record.event_name() != Some("gateway.failure") {
            continue;
        }
        if let Some((_, AnyValue::String(code))) = log
            .record
            .attributes_iter()
            .find(|(key, _)| key.as_str() == "error.type")
        {
            let code = code.as_str();
            return if code.starts_with("admission.") {
                format!("planning:{}", serde_json::to_string(&[code]).unwrap())
            } else {
                code.to_owned()
            };
        }
    }
    assert_eq!(status, 200, "failed request must have a failure diagnostic");
    assert!(!failed, "aborted body must have a failure diagnostic");
    "accepted".into()
}

// The initially probed bindings cover standard, Bailian, DeepSeek and native Messages.
#[tokio::test]
async fn unrepresentable_named_choice_never_dispatches_to_bailian() {
    let calls = Arc::new(Mutex::new(0usize));
    let observed = calls.clone();
    let router = Router::new().route(
        "/v1/chat/completions",
        post(move || {
            let calls = observed.clone();
            async move {
                *calls.lock().unwrap() += 1;
                (axum::http::StatusCode::BAD_REQUEST, "unexpected dispatch")
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}/v1", listener.local_addr().unwrap());
    let _server = Server(tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    }));
    let config = service();
    let mut boot = Fixture::with_accounts(
        config,
        json!({"models":{"qwen3.8-flash":{"candidates":[{"binding":"dashscope-flash-chat"}]}}}),
        &[("aliyun-dashscope-cn", "synthetic", "synthetic-upstream-key")],
        json!({"version":1,"providers":{"aliyun-dashscope-cn":{"accounts":[{
            "name":"synthetic","enabled":true,"credential":{"alias":"synthetic"},"base_url":origin
        }]}}}),
    )
    .unwrap();
    let exporter = boot.record_failures();
    let body = json!({"model":"qwen3.8-flash","max_tokens":64,
        "thinking":{"type":"disabled"},"messages":[{"role":"user","content":"go"}],
        "tools":[{"name":"read","description":"Read","input_schema":{"type":"object"}}],
        "tool_choice":{"type":"tool","name":"read"}});
    let (status, _, failed) = send(
        &boot,
        Api::Messages,
        body,
        &[(
            "anthropic-version",
            crate::protocol::anthropic::messages::API_VERSION,
        )],
    )
    .await;
    assert_eq!(status, 400);
    assert!(!failed);
    assert_eq!(*calls.lock().unwrap(), 0);
    assert_eq!(
        disposition(&exporter, status, failed),
        "planning:[\"admission.tool_choice\"]"
    );
}

type Binding<'a> = (&'a str, &'a str, &'a str, &'a str, Api);
#[tokio::test]
async fn siwc_quota_event_keeps_its_failure_class_before_response_created() {
    for stream in [false, true] {
        let calls = Arc::new(Mutex::new(0usize));
        let observed = calls.clone();
        let router = Router::new().route(
            "/v1/responses",
            post(move || {
                let calls = observed.clone();
                async move {
                    *calls.lock().unwrap() += 1;
                    ([("content-type", "text/event-stream")],
                        "event: error\ndata: {\"type\":\"error\",\"sequence_number\":0,\"code\":\"subscription_sharing_usage_limit_exceeded\",\"message\":\"synthetic quota exhausted\",\"param\":null}\n\n")
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let _server = Server(tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        }));
        let mut boot = Fixture::with_oauth(
            service(),
            json!({"models":{"gpt-6.1-sol":{"candidates":[{"binding":"chatgpt-responses"}]}}}),
            "chatgpt",
            "synthetic",
        )
        .unwrap();
        boot.loopback_authority(&origin);
        let exporter = boot.record_failures();
        let (status, bytes, failed) = send(
            &boot,
            Api::Responses,
            json!({"model":"gpt-6.1-sol","input":"go","stream":stream}),
            &[],
        )
        .await;
        assert_eq!(
            status,
            if stream { 200 } else { 429 },
            "{}",
            disposition(&exporter, status, failed)
        );
        assert!(!failed);
        let body = String::from_utf8(bytes).unwrap();
        assert!(body.contains("quota") || body.contains("rate_limit_exceeded"));
        assert_eq!(*calls.lock().unwrap(), 1);
    }
}

const BINDINGS: &[Binding<'static>] = &[
    (
        "glm-5.3",
        "dashscope-glm-chat",
        "aliyun-dashscope-cn",
        "aliyun-dashscope-cn-api-key",
        Api::Chat,
    ),
    (
        "deepseek-v4.1-flash-1017",
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
    BareModel,
    JsonOnlyReports,
    Stored,
    WithoutCache,
    LegacyVersion,
    ThinkingBeta,
    ResolvedHeaders,
    Uncapped,
    SummaryHistory,
    OpaqueHistory { stored: bool },
    AdaptiveHistory,
    Incomplete { stored: bool },
    ReportedFacts { refusal: bool, stored: bool },
}
impl Control {
    fn reasoning(self) -> Option<ReasoningShape> {
        match self {
            Self::SummaryHistory => Some(ReasoningShape::Summary),
            Self::OpaqueHistory { .. } => Some(ReasoningShape::Opaque),
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
    let mut json = observe(binding, client, profile, false, control).await;
    let sse = observe(binding, client, profile, true, control).await;
    if client == Api::Chat {
        for (complete, streamed) in json.1.iter_mut().zip(&sse.1) {
            if let Some(metadata) = complete["reports"]
                .as_object_mut()
                .unwrap()
                .remove("metadata")
            {
                assert_eq!(binding.4, Api::Responses);
                assert_eq!(metadata, serde_json::json!({"synthetic":"echo"}));
                assert!(
                    streamed["reports"].get("metadata").is_none(),
                    "Chat chunks have no metadata carrier"
                );
            }
        }
    }
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
    let (outcomes, values, _) =
        observe_intake(binding, client, profile, stream, control, false).await;
    (outcomes, values)
}

type Observation = (Vec<String>, Vec<Value>, Vec<Value>);

async fn observe_intake(
    binding: &Binding<'_>,
    client: Api,
    profile: Profile,
    stream: bool,
    control: Control,
    force_stream: bool,
) -> Observation {
    let &(model, binding, provider, pool, upstream) = binding;
    let reasoning = control.reasoning().or_else(|| {
        (upstream == Api::Messages && profile == Profile::Reasoning)
            .then_some(ReasoningShape::Signed)
    });
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
    let parameters = (client == Api::Responses && profile != Profile::Text).then(|| {
        wire::request(client, profile, model, stream)["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|tool| tool["parameters"].clone())
            .collect::<Vec<_>>()
    });
    let calls = Arc::new(Mutex::new(Vec::<Value>::new()));
    let captured = calls.clone();
    let beta_api = provider == "deepseek";
    let handler = post(move |headers: HeaderMap, Json(body): Json<Value>| {
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
            if force_stream {
                assert_eq!(body["stream"], true, "experiment must reach SSE intake");
                if upstream == Api::Chat {
                    assert_eq!(body["stream_options"]["include_usage"], true);
                }
            }
            if upstream == Api::Messages
                && client == Api::Messages
                && profile == Profile::Reasoning
                && control == Control::Default
            {
                assert_eq!(body["thinking"]["type"], "adaptive");
                assert_eq!(body["output_config"]["effort"], "low");
                assert_eq!(
                    body["messages"][1],
                    json!({"role":"system","content":[],"output_config":{"effort":"medium"}})
                );
                assert!(body.get("temperature").is_none());
            }
            if control == Control::SummaryHistory {
                assert_eq!(body["reasoning"], json!({"effort":"low"}));
                assert!(body.get("include").is_none());
            }
            if let Some(parameters) = parameters {
                let tools = if oauth == Some("openai") {
                    body["input"][0]["tools"].as_array().unwrap()
                } else {
                    body["tools"].as_array().unwrap()
                };
                assert_eq!(tools.len(), parameters.len());
                for (tool, parameters) in tools.iter().zip(parameters) {
                    let tool = if upstream == Api::Chat {
                        &tool["function"]
                    } else {
                        tool
                    };
                    assert_eq!(
                        tool[if upstream == Api::Messages {
                            "input_schema"
                        } else {
                            "parameters"
                        }],
                        parameters
                    );
                    if upstream == Api::Messages || upstream == Api::Chat && !bailian {
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
    });
    let api_root = if beta_api { "/beta" } else { "/v1" };
    let path = if beta_api {
        if upstream == Api::Responses {
            "/beta/responses"
        } else {
            "/beta/chat/completions"
        }
    } else {
        upstream.path()
    };
    let router = Router::new().route(path, handler);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}{api_root}", listener.local_addr().unwrap());
    let _server = Server(tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    }));
    let mut config = service();
    config["resources"] = json!({"concurrency":1});
    config["delivery"] = json!({"exchange_timeout_ms":3000,"delta_coalescing":null});
    let models = json!({"models":{model:{"candidates":[{
        "binding":binding
    }]}}});
    let mut boot = if oauth.is_some() {
        Fixture::with_oauth(config, models, provider, pool)
    } else {
        Fixture::with_accounts(
            config,
            models,
            &[(provider, pool, "synthetic-upstream-key")],
            json!({"version":1,"providers":{provider:{"accounts":[{
                "name":pool,"enabled":true,"credential":{"alias":pool},"base_url":origin
            }]}}}),
        )
    }
    .unwrap();
    if oauth.is_some() {
        boot.loopback_authority(origin.strip_suffix("/v1").unwrap());
    }
    if force_stream {
        assert!(!stream, "experiment preserves JSON client delivery");
        boot.force_upstream_stream();
    }
    let exporter = boot.record_failures();
    let mut request = wire::request(client, profile, model, stream);
    if control == Control::BareModel {
        assert_eq!(client, Api::Messages);
        request = wire::request(client, profile, "synthetic-model", stream);
        request["model"] = json!(model);
    }
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
            | Control::OpaqueHistory { stored: true }
            | Control::Incomplete { stored: true }
            | Control::ReportedFacts { stored: true, .. }
    );
    if stored {
        assert_eq!(client, Api::Responses);
        request["store"] = json!(true);
    }
    let mut outcomes = Vec::new();
    let mut views = Vec::new();
    let mut reports = Vec::new();
    for turn in 0..2 {
        if control == Control::WithoutCache {
            assert_eq!(client, Api::Messages);
            wire::without_cache(&mut request);
        }
        let before = calls.lock().unwrap().len();
        let raw_headers = wire::raw_messages_headers();
        let resolved_headers = wire::resolved_messages_headers(profile);
        let mut headers = vec![
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
        if control == Control::ResolvedHeaders {
            headers = resolved_headers
                .as_object()
                .unwrap()
                .iter()
                .filter_map(|(name, value)| value.as_str().map(|value| (name.as_str(), value)))
                .collect();
        }
        exporter.reset();
        let (status, bytes, failed) = send(&boot, client, request.clone(), &headers).await;
        let outcome = disposition(&exporter, status, failed);
        let after = calls.lock().unwrap().len();
        assert!(after == before || after == before + 1);
        outcomes.push(format!("{outcome}@{}", after - before));
        if outcome == "codec.invalid.invalid_value" {
            panic!(
                "synthetic fault {binding}/{client:?}/{profile:?}/stream={stream}/turn={turn}: {outcome}\nsynthetic request: {request}"
            );
        }
        if outcome != "accepted" {
            eprintln!(
                "pending {binding}/{client:?}/{profile:?}/stream={stream}: {outcome}@{} -> accepted",
                after - before
            );
            wire::assert_failure(client, stream, status, &bytes, failed);
            if matches!(
                control,
                Control::LegacyVersion | Control::ThinkingBeta | Control::ResolvedHeaders
            ) {
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
        assert!(!failed, "successful diagnostic with aborted client body");
        if turn == 1 {
            wire::assert_history(
                upstream,
                client,
                profile,
                &calls.lock().unwrap()[1],
                reasoning,
                bailian,
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
        reports.push(json!({"delivery":wire::intake_view(client, &value)}));
        wire::assert_delivery(client, upstream, profile, turn, &value, reasoning);
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
    if control == Control::Default {
        let descriptor: Value =
            serde_json::from_slice(&boot.gateway.state.text_models.models[model]).unwrap();
        let protocol = match client {
            Api::Chat => "chat_completions",
            Api::Responses => "responses",
            Api::Messages => "messages",
        };
        let interface = descriptor["data"]["interfaces"]
            .as_array()
            .unwrap()
            .iter()
            .find(|interface| interface["protocol"] == protocol)
            .unwrap();
        let profile = match profile {
            Profile::Text => "text",
            Profile::Tools => "tools",
            Profile::Reasoning => "reasoning",
        };
        let mode = if stream { "sse" } else { "json" };
        let defaults = &interface["generation"]["agent_defaults"];
        assert_eq!(defaults["scope"], "request_preflight");
        assert_eq!(defaults["history_loop"], "not_asserted");
        assert_eq!(
            matches!(
                defaults["profiles"][profile][mode]["state"].as_str(),
                Some("faithful" | "approved_adaptation")
            ),
            !calls.lock().unwrap().is_empty(),
            "{binding}/{protocol}/{profile}/{mode}: published default admission must reach dispatch"
        );
        let capabilities: Value =
            serde_json::from_slice(&boot.gateway.state.capabilities.0).unwrap();
        let capability = capabilities["data"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["model"] == model && entry["protocol"] == protocol)
            .unwrap();
        assert_eq!(&capability["agent_defaults"], defaults);
    }
    boot.gateway.shutdown();
    (outcomes, views, reports)
}

async fn grouped_responses_history(managed: bool) {
    let model = "qwen3.8-flash";
    let output = json!([
        {"type":"function_call","id":"fc_a","call_id":"a","name":"lookup","arguments":"{\"key\":\"A\"}","status":"completed"},
        {"type":"function_call","id":"fc_b","call_id":"b","name":"lookup","arguments":"{\"key\":\"B\"}","status":"completed"}
    ]);
    let results = json!([
        {"type":"function_call_output","call_id":"a","output":"value-A"},
        {"type":"function_call_output","call_id":"b","output":"value-B"}
    ]);
    for stream in [false, true] {
        let calls = Arc::new(Mutex::new(Vec::<Value>::new()));
        let observed = calls.clone();
        let source = output.clone();
        let router = Router::new().route("/v1/responses", post(move |Json(body): Json<Value>| {
            let observed = observed.clone();
            let source = source.clone();
            async move {
                assert_eq!(body["model"], model);
                assert_eq!(body["store"], false);
                assert!(body.get("previous_response_id").is_none());
                let turn = {
                    let mut calls = observed.lock().unwrap();
                    let turn = calls.len();
                    assert!(turn < 3, "no hidden retries");
                    calls.push(body.clone());
                    turn
                };
                let response = json!({"id":format!("source-{turn}"),"object":"response",
                    "model":model,"created_at":1,"status":"completed",
                    "output":if turn == 0 { source } else { json!([]) }});
                if body["stream"] != true {
                    return Json(response).into_response();
                }
                // Independent event oracle, with real argument deltas rather
                // than a terminal snapshot supplying missing stream content.
                let mut opening = response.clone();
                opening["status"] = json!("in_progress");
                opening["output"] = json!([]);
                let mut frames = vec![json!({"type":"response.created","response":opening})];
                for (index, item) in response["output"].as_array().unwrap().iter().enumerate() {
                    let mut added = item.clone();
                    added["status"] = json!("in_progress");
                    added["arguments"] = json!("");
                    frames.extend([
                        json!({"type":"response.output_item.added","output_index":index,"item":added}),
                        json!({"type":"response.function_call_arguments.delta","output_index":index,
                            "item_id":item["id"],"delta":item["arguments"]}),
                        json!({"type":"response.function_call_arguments.done","output_index":index,
                            "item_id":item["id"],"arguments":item["arguments"]}),
                        json!({"type":"response.output_item.done","output_index":index,"item":item}),
                    ]);
                }
                frames.push(json!({"type":"response.completed","response":response}));
                let bytes = frames.into_iter().enumerate().map(|(sequence, mut frame)| {
                    frame["sequence_number"] = json!(sequence);
                    format!("event: {}\ndata: {frame}\n\n",frame["type"].as_str().unwrap())
                }).collect::<String>();
                ([("content-type","text/event-stream")],bytes).into_response()
            }
        }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}/v1", listener.local_addr().unwrap());
        let _server = Server(tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        }));
        let boot = Fixture::with_accounts(
            service(),
            json!({"models":{model:{
            "candidates":[{"binding":"dashscope-flash-responses"}]}}}),
            &[(
                "aliyun-dashscope-cn",
                "aliyun-dashscope-cn-api-key",
                "synthetic-upstream-key",
            )],
            json!({"version":1,"providers":{"aliyun-dashscope-cn":{"accounts":[{
                "name":"aliyun-dashscope-cn-api-key","enabled":true,
                "credential":{"alias":"aliyun-dashscope-cn-api-key"},"base_url":origin
            }]}}}),
        )
        .unwrap();
        let mut request = json!({"model":model,"input":"go","max_output_tokens":64,"stream":stream,"store":managed});
        request["tools"] = json!([{"type":"function","name":"lookup","description":"Look up a key.",
            "parameters":{"type":"object","properties":{"key":{"type":"string"}}},"strict":false}]);
        if stream {
            request["stream_options"] = json!({"include_obfuscation":false});
        }
        let (status, bytes, failed) = send(&boot, Api::Responses, request.clone(), &[]).await;
        assert_eq!(status, 200);
        assert!(!failed);
        let first = wire::delivered(Api::Responses, &bytes, stream);
        assert_eq!(first["output"], output);
        let expected = json!([
            {"type":"message","role":"user","content":[{"type":"input_text","text":"go"}]},
            {"type":"function_call","id":"fc_a","call_id":"a","name":"lookup","arguments":"{\"key\":\"A\"}"},
            {"type":"function_call","id":"fc_b","call_id":"b","name":"lookup","arguments":"{\"key\":\"B\"}"},
            results[0],results[1]
        ]);
        request["store"] = json!(false);
        if managed {
            request["previous_response_id"] = first["id"].clone();
            request["input"] = results.clone();
            let (status, bytes, failed) = send(&boot, Api::Responses, request.clone(), &[]).await;
            assert_eq!(status, 200);
            assert!(!failed);
            assert_eq!(
                wire::delivered(Api::Responses, &bytes, stream)["status"],
                "completed"
            );
        }
        request
            .as_object_mut()
            .unwrap()
            .remove("previous_response_id");
        request["input"] = expected.clone();
        let (status, bytes, failed) = send(&boot, Api::Responses, request, &[]).await;
        assert_eq!(status, 200);
        assert!(!failed);
        assert_eq!(
            wire::delivered(Api::Responses, &bytes, stream)["status"],
            "completed"
        );
        let captured = calls.lock().unwrap();
        assert_eq!(captured.len(), if managed { 3 } else { 2 });
        assert_eq!(captured[1]["input"], expected, "grouped call-ID history");
        if managed {
            assert_eq!(
                captured[2], captured[1],
                "stateless and managed upstream requests"
            );
        }
        drop(captured);
        boot.gateway.shutdown();
    }
}

#[tokio::test]
async fn grouped_stateless_responses_history_preserves_call_id_association() {
    grouped_responses_history(false).await;
}

#[tokio::test]
#[ignore = "experimental stateful API; awaiting explicit activation"]
async fn experimental_grouped_responses_history_agrees_between_managed_and_stateless_continuation()
{
    grouped_responses_history(true).await;
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
                let actual = lane(
                    binding,
                    client,
                    Profile::Text,
                    stream,
                    Control::ReportedFacts {
                        refusal,
                        stored: false,
                    },
                )
                .await;
                assert_eq!(actual, ACCEPT, "reports {client:?}/{refusal}/{stream}");
            }
        }
    }
}

#[tokio::test]
async fn representative_dialects_preserve_text_and_tool_workflows() {
    use crate::topology::{GenerationProtocol, catalog::API_KEY_BINDINGS};
    // Exercise wire dialects and distinct tool/history policies, not catalog membership.
    for prefix in [
        "openai-terra",
        "xiaomi",
        "deepseek",
        "dashscope-flash",
        "openrouter",
        "openrouter-ember",
        "nvidia",
        "modelbest-minicpm5-1b",
        "modelbest-minicpm-v-4.7-1b",
        "opencode-go",
        "opencode-go-minimax",
        "zhipu",
    ] {
        let entry = API_KEY_BINDINGS
            .iter()
            .find(|entry| entry.endpoint_prefix == prefix)
            .unwrap();
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
}

#[tokio::test]
async fn chat_json_only_facts_block_unconditional_streaming_intake() {
    let binding = BINDINGS
        .iter()
        .find(|binding| binding.1 == "deepseek-chat")
        .unwrap();
    let mut json = observe_intake(
        binding,
        Api::Chat,
        Profile::Text,
        false,
        Control::JsonOnlyReports,
        false,
    )
    .await;
    let streaming = observe_intake(
        binding,
        Api::Chat,
        Profile::Text,
        false,
        Control::JsonOnlyReports,
        true,
    )
    .await;
    assert_eq!(json.0, ACCEPT);
    assert_eq!(streaming.0, ACCEPT);
    for (full, stream) in json.1.iter_mut().zip(&streaming.1) {
        assert_eq!(
            full["reports"]["metadata"],
            json!({"synthetic":"json-only"})
        );
        assert_eq!(
            full["message"]["annotations"],
            json!([{
                "type":"url_citation","url_citation":{
                    "start_index":0,"end_index":1,"title":"Synthetic source",
                    "url":"https://example.invalid/source"
                }
            }])
        );
        assert!(stream["reports"].get("metadata").is_none());
        assert!(stream["message"].get("annotations").is_none());
        full["reports"].as_object_mut().unwrap().remove("metadata");
        full["message"]
            .as_object_mut()
            .unwrap()
            .remove("annotations");
    }
    for report in &mut json.2 {
        let full = &mut report["delivery"];
        full.as_object_mut().unwrap().remove("metadata");
        full["choices"][0]["message"]
            .as_object_mut()
            .unwrap()
            .remove("annotations");
    }
    assert_eq!(json, streaming, "only the two named facts may differ");
}

#[tokio::test]
async fn subscription_dialects_preserve_text_tools_and_reasoning() {
    use crate::topology::catalog::SUBSCRIPTION_BINDINGS;
    for identity in ["chatgpt", "grok"] {
        let entry = SUBSCRIPTION_BINDINGS
            .iter()
            .find(|entry| entry.identity == identity)
            .unwrap();
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
        for client in [Api::Chat, Api::Responses, Api::Messages] {
            for profile in [Profile::Text, Profile::Tools, Profile::Reasoning] {
                // Common protocol controls, not each subscription's emitted reasoning shape.
                let actual = transports(&binding, client, profile, Control::Default).await;
                assert_eq!(actual, ACCEPT, "{}/{client:?}/{profile:?}", binding.1);
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
        transports(
            &grok,
            Api::Responses,
            Profile::Text,
            Control::OpaqueHistory { stored: false }
        )
        .await,
        ACCEPT
    );
}

#[tokio::test]
async fn agent_default_production_lanes() {
    for binding in BINDINGS {
        for client in [Api::Chat, Api::Responses, Api::Messages] {
            for profile in [Profile::Text, Profile::Tools, Profile::Reasoning] {
                // Text/tool policies are covered by the representative dialect gate.
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
#[ignore = "experimental stateful API; awaiting explicit activation"]
async fn experimental_stored_production_responses_agree_with_delivered_history() {
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
async fn messages_delivery_controls_complete_without_cache_markers() {
    // Isolate report projection from owner-local cache projection.
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
            assert_eq!(actual, ACCEPT, "delivery {}/{stream}", binding.1);
        }
    }
}

#[tokio::test]
async fn installed_messages_headers_complete_without_opt_outs() {
    let binding = BINDINGS.iter().find(|b| b.4 == Api::Messages).unwrap();
    for control in [
        Control::LegacyVersion,
        Control::ThinkingBeta,
        Control::ResolvedHeaders,
    ] {
        for profile in [Profile::Text, Profile::Tools, Profile::Reasoning] {
            assert_eq!(
                lane(binding, Api::Messages, profile, true, control).await,
                ACCEPT
            );
        }
    }
}

#[tokio::test]
async fn bare_messages_manual_thinking_converts_to_adaptive() {
    let binding = BINDINGS.iter().find(|b| b.4 == Api::Messages).unwrap();
    for stream in [false, true] {
        assert_eq!(
            lane(
                binding,
                Api::Messages,
                Profile::Reasoning,
                stream,
                Control::BareModel
            )
            .await,
            ACCEPT
        );
    }
}

#[tokio::test]
async fn incomplete_responses_reasoning_keeps_its_delivery_and_replay_status() {
    let binding = BINDINGS
        .iter()
        .find(|b| b.1 == "openrouter-responses")
        .unwrap();
    for stream in [false, true] {
        let actual = lane(
            binding,
            Api::Responses,
            Profile::Reasoning,
            stream,
            Control::Incomplete { stored: false },
        )
        .await;
        assert_eq!(actual, ["accepted@1", "accepted@1"]);
    }
}

const ACCEPT: &[&str] = &["accepted@1", "accepted@1"];

fn basic_expected(binding: &str, client: Api, profile: Profile) -> &'static [&'static str] {
    match (binding, client, profile) {
        ("deepseek-chat" | "zhipu-chat", Api::Chat, Profile::Tools) => {
            &["planning:[\"admission.reasoning_history_required\"]@0"]
        }
        _ => ACCEPT,
    }
}

fn expected(binding: &str, client: Api, profile: Profile) -> &'static [&'static str] {
    use Api::*;
    use Profile::*;
    if profile != Reasoning && binding != "opencode-go-haiku-messages" {
        return basic_expected(binding, client, profile);
    }
    match (binding, client, profile) {
        ("dashscope-glm-chat", Chat | Responses, Reasoning) => ACCEPT,
        ("dashscope-glm-chat" | "deepseek-chat", Messages, Reasoning) => ACCEPT,
        ("openrouter-responses", Messages, Reasoning) => ACCEPT,
        ("dashscope-flash-responses", Messages, Reasoning) => ACCEPT,
        ("deepseek-chat", Chat, Reasoning) => {
            &["planning:[\"admission.reasoning_history_required\"]@0"]
        }
        ("deepseek-chat", Responses, Reasoning) => ACCEPT,
        ("openrouter-responses", Responses, Reasoning) => ACCEPT,
        ("openrouter-responses" | "dashscope-flash-responses", Chat, Reasoning) => ACCEPT,
        ("dashscope-flash-responses", Responses, Reasoning) => ACCEPT,
        ("opencode-go-haiku-messages", Chat | Messages, Text | Tools)
        | ("opencode-go-haiku-messages", Responses, Text | Tools) => ACCEPT,
        ("opencode-go-haiku-messages", Chat, Reasoning) => ACCEPT,
        ("opencode-go-haiku-messages", Responses, Reasoning) => ACCEPT,
        ("opencode-go-haiku-messages", Messages, Reasoning) => ACCEPT,
        _ => panic!("unreviewed lane: {binding}/{client:?}/{profile:?}"),
    }
}
