//! Explicit live acceptance probe for fixed provider bindings.
//!
//! Explicitly gated: refuses to run unless `MORPHIECORE_PROBE=1`.
//!
//!     MORPHIECORE_PROBE=1 cargo run --locked --offline --example live_probe
//!
//! Boundaries honored here:
//! - credentials are read from the private local config at run time and used
//!   in-process only for auth headers; nothing secret is logged or reported;
//! - the matrix is chat/responses × {text, json_object, tool} × {JSON, SSE};
//!   tools continue with actual response history, never reconstructed reasoning;
//! - OpenRouter and additional Providers require explicit `MORPHIECORE_PROBE_MODEL`; optional
//!   PROTOCOL/CASE/DELIVERY filters narrow calls; MAX_TOKENS is bounded at 2048;
//! - one request per call, 120s timeout, 2s spacing, bounded `max_output_tokens`;
//! - model ids are verified through the free Models listing before any paid call;
//! - raw per-call reports go to the gitignored `testdata/runtime/` tree and hold
//!   no keys, auth headers or credential locators.

#[cfg(test)]
#[path = "../tests/support/filesystem.rs"]
mod test_files;

use morphiecore::{
    adapter::{Adapter, Dialect},
    execution::{Attempt, AttemptError, ResponseDelivery, admit, prepare},
    lowering::generation::{GenerationRepresentationContract, ReportedFactPolicy},
    protocol::openai::{
        Profile,
        chat_sse::ChatSseDecoder,
        sse::{Obfuscation, ResponsesSseDecoder, SseLimits},
    },
    provider::{ErrorClass, ProviderDefinition, SecretMaterial},
    semantic::{
        context::StreamOptions,
        task::generation::{ContentPart, Continuation, GenerationResponse, Item, Outcome, Usage},
    },
    topology::{Endpoint, EndpointId, ProtocolProfile, PublicModel, catalog as topology_catalog},
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, fmt::Write as _, time::Instant};
use tokio::time::{Duration, sleep};

const CALL_TIMEOUT: Duration = Duration::from_secs(120);
const CALL_INTERVAL: Duration = Duration::from_secs(2);
const CAPTURE_LIMIT: usize = 2 * 1024 * 1024;

fn downstream_contract(
    upstream: &GenerationRepresentationContract,
    reported_facts: ReportedFactPolicy,
) -> GenerationRepresentationContract {
    // Input controls and reported output facts have independent admission.
    GenerationRepresentationContract {
        replay_origin: upstream.adaptation.scope.clone(),
        reported_facts,
        ..GenerationRepresentationContract::full()
    }
}

fn downstream_adapter(profile: Profile, upstream: &GenerationRepresentationContract) -> Adapter {
    Adapter::new(
        profile,
        match profile {
            Profile::Responses => Dialect::Standard,
            Profile::Chat => Dialect::MorphieCore,
        },
        upstream.adaptation.scope.clone(),
    )
}

const TEXT_PROMPT: &str = "Reply with exactly the word pong.";
const LENGTH_PROMPT: &str = "Write alpha 200 times separated by spaces. Do not summarize.";
const JSON_PROMPT: &str = "Return a JSON object with keys pong (boolean) and note (string).";
const TOOL_PROMPT: &str = "Use the lookup tool to find the value for key \"alpha\", then report it in one short sentence.";
const TOOL_OUTPUT: &str = "{\"value\": 42}";

struct ModelSpec {
    label: &'static str,
    pool: &'static str,
    provider: &'static str,
    models_path: Option<&'static str>,
    chat_endpoint: &'static str,
    responses_endpoint: Option<&'static str>,
}

const MODELS: [ModelSpec; 12] = [
    ModelSpec {
        label: "deepseek-flash",
        pool: "deepseek-api-key",
        provider: "deepseek",
        models_path: Some("/models"),
        chat_endpoint: "deepseek-chat",
        responses_endpoint: Some("deepseek-responses"),
    },
    ModelSpec {
        label: "mimo-v2.6-pro",
        pool: "xiaomi-api-key",
        provider: "xiaomi",
        models_path: Some("/v1/models"),
        chat_endpoint: "xiaomi-chat",
        responses_endpoint: Some("xiaomi-responses"),
    },
    ModelSpec {
        label: "mimo-v2.6-flash",
        pool: "xiaomi-api-key",
        provider: "xiaomi",
        models_path: Some("/v1/models"),
        chat_endpoint: "xiaomi-flash-chat",
        responses_endpoint: Some("xiaomi-flash-responses"),
    },
    ModelSpec {
        label: "gpt-6-luna",
        pool: "openrouter-api-key",
        provider: "openrouter",
        models_path: Some("/api/v1/models"),
        chat_endpoint: "openrouter-chat",
        responses_endpoint: Some("openrouter-responses"),
    },
    ModelSpec {
        label: "nemotron-3-super",
        pool: "nvidia-api-key",
        provider: "nvidia",
        models_path: Some("/v1/models"),
        chat_endpoint: "nvidia-chat",
        responses_endpoint: Some("nvidia-responses"),
    },
    ModelSpec {
        label: "qwen3.8-max",
        pool: "aliyun-dashscope-cn-api-key",
        provider: "aliyun-dashscope-cn",
        models_path: Some("/compatible-mode/v1/models"),
        chat_endpoint: "aliyun-dashscope-cn-chat",
        responses_endpoint: Some("aliyun-dashscope-cn-responses"),
    },
    ModelSpec {
        label: "qwen3.8-flash",
        pool: "aliyun-tokenplan-cn-api-key",
        provider: "aliyun-tokenplan-cn",
        // No Models entry is declared for this plan; never guess a discovery URL.
        models_path: None,
        chat_endpoint: "aliyun-tokenplan-cn-chat",
        responses_endpoint: Some("aliyun-tokenplan-cn-responses"),
    },
    ModelSpec {
        label: "minicpm5-1b",
        pool: "modelbest-api-key",
        provider: "modelbest",
        models_path: Some("/v1/models"),
        chat_endpoint: "modelbest-minicpm5-1b-chat",
        responses_endpoint: None,
    },
    ModelSpec {
        label: "minicpm5-2b",
        pool: "modelbest-api-key",
        provider: "modelbest",
        models_path: Some("/v1/models"),
        chat_endpoint: "modelbest-minicpm5-2b-chat",
        responses_endpoint: None,
    },
    ModelSpec {
        label: "minicpm-v-4.6",
        pool: "modelbest-api-key",
        provider: "modelbest",
        models_path: Some("/v1/models"),
        chat_endpoint: "modelbest-minicpm-v-4.6-chat",
        responses_endpoint: None,
    },
    ModelSpec {
        label: "glm-5.3",
        pool: "zhipu-api-key",
        provider: "zhipu",
        models_path: Some("/api/paas/v4/models"),
        chat_endpoint: "zhipu-chat",
        responses_endpoint: Some("zhipu-responses"),
    },
    ModelSpec {
        label: "glm-5.3-flash",
        pool: "zhipu-api-key",
        provider: "zhipu",
        models_path: Some("/api/paas/v4/models"),
        chat_endpoint: "zhipu-flash-chat",
        responses_endpoint: Some("zhipu-flash-responses"),
    },
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Case {
    Text,
    JsonObject,
    Tool,
    Length,
    Schema,
    Image,
}

impl Case {
    fn name(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::JsonObject => "json_object",
            Self::Tool => "tool",
            Self::Length => "length",
            Self::Schema => "schema",
            Self::Image => "image",
        }
    }

    fn cap(self) -> u64 {
        match self {
            Self::Text => 64,
            Self::Length => 8,
            Self::JsonObject | Self::Tool => 192,
            Self::Schema => 2048,
            Self::Image => 512,
        }
    }
}

const CASES: [Case; 3] = [Case::Text, Case::JsonObject, Case::Tool];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Delivery {
    Json,
    Stream,
}

impl Delivery {
    fn name(self) -> &'static str {
        match self {
            Self::Json => "json",
            Self::Stream => "sse",
        }
    }
}

#[derive(Clone, Debug, serde::Serialize)]
struct CallReport {
    model: String,
    provider: String,
    protocol: String,
    case: String,
    delivery: String,
    round: u8,
    stage: String,
    ok: bool,
    http_status: Option<u16>,
    error_class: Option<String>,
    outcome: Option<String>,
    usage: Option<Value>,
    normalizations: Vec<String>,
    text_chars: Option<usize>,
    excerpt: Option<String>,
    tool_call: Option<String>,
    error: Option<String>,
    delivered_bytes: usize,
    latency_ms: u64,
}

#[derive(Clone, Debug)]
struct ToolCallInfo {
    call_id: String,
    name: String,
    arguments: morphiecore::semantic::task::generation::ToolArguments,
    history: Vec<Value>,
}

struct Credentials {
    pools: BTreeMap<String, String>,
}

/// Explicit owned JSON documents; no environment key, legacy import or fallback.
fn load_credentials(path: &str) -> Result<Credentials, String> {
    let load = || -> Result<Credentials, morphiecore::credential::CredentialError> {
        if !std::path::Path::new(path).is_dir() {
            return Err(morphiecore::credential::CredentialError::Storage);
        }
        let manager = morphiecore::credential::CredentialManager::new(path, vec![])?;
        let mut pools = BTreeMap::new();
        for (provider, binding, mut status) in manager.pools()? {
            if !topology_catalog::API_KEY_BINDINGS
                .iter()
                .any(|b| b.credential == binding && (b.provider)().id.as_str() == provider)
            {
                continue;
            }
            status.config.max_attempts = 1;
            status.config.fallback = false;
            let pool = manager.bind_pool(&provider, &status.config)?;
            if let Some(morphiecore::credential::PoolMember::ApiKey(key)) = pool.members.first() {
                pools.insert(binding, key.borrow()?.expose().to_owned());
            }
        }
        if pools.is_empty() {
            return Err(morphiecore::credential::CredentialError::KeyUnavailable);
        }
        Ok(Credentials { pools })
    };
    load().map_err(|_| "invalid credential configuration".into())
}

fn scenario_request(
    protocol: Profile,
    label: &str,
    case: Case,
    delivery: Delivery,
    round: u8,
    call: Option<&ToolCallInfo>,
) -> Value {
    let streaming = delivery == Delivery::Stream;
    let mut value = match protocol {
        Profile::Chat => {
            let mut messages = vec![json!({"role":"user","content": match case {
                Case::Text => TEXT_PROMPT,
                Case::JsonObject => JSON_PROMPT,
                Case::Tool => TOOL_PROMPT,
                Case::Length => LENGTH_PROMPT,
                Case::Schema | Case::Image => unreachable!("Responses-only probe"),
            }})];
            if round == 2 {
                let call = call.expect("round two carries a tool call");
                messages.extend(call.history.clone());
                messages
                    .push(json!({"role":"tool","tool_call_id":call.call_id,"content":TOOL_OUTPUT}));
            }
            let mut value =
                json!({"model":label,"messages":messages,"max_completion_tokens":case.cap()});
            if case == Case::JsonObject {
                value["response_format"] = json!({"type":"json_object"});
            }
            if case == Case::Tool {
                value["tools"] = json!([chat_tool()]);
                value["tool_choice"] =
                    json!(if matches!(label, "deepseek-flash" | "nemotron-3-super") {
                        "auto"
                    } else if round == 1 {
                        "required"
                    } else {
                        "none"
                    });
            }
            value
        }
        Profile::Responses => {
            let prompt = match case {
                Case::Text => TEXT_PROMPT,
                Case::JsonObject => JSON_PROMPT,
                Case::Tool => TOOL_PROMPT,
                Case::Length => LENGTH_PROMPT,
                Case::Schema => {
                    "Return JSON with z_answer equal to 7 and a_label equal to synthetic."
                }
                Case::Image => {
                    "Classify the dominant color of each image in order using only these basic labels: blue, green, black, red, white, yellow. Do not use shade names. Reply with exactly two lowercase labels separated by a comma, no spaces or other text."
                }
            };
            let mut input =
                vec![json!({"role":"user","content":[{"type":"input_text","text":prompt}]})];
            if case == Case::Image {
                // Exact synthetic 192x192 RGB fixtures from probe_support.images.color_png.
                // The repeated Base64 segment avoids duplicating a long opaque literal.
                let red = format!(
                    "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAMAAAADACAIAAADdvvtQAAACW0lEQVR4nO3OQQkAQBDEsPFv+s7DfkqhEAHZ25Iz{}fhC1DxwqeGLAr8TfAAAAAElFTkSuQmCC",
                    "fhA1".repeat(190)
                );
                let blue = format!(
                    "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAMAAAADACAIAAADdvvtQAAACW0lEQVR4nO3OQQkAQBDEsPFv+k7EPkqhEAHZ9pID{}fhCxDytZeGJa8Jr2AAAAAElFTkSuQmCC",
                    "fhA1".repeat(190)
                );
                input[0]["content"] = json!([
                    {"type":"input_text","text":prompt},
                    {"type":"input_image","image_url":red},
                    {"type":"input_text","text":"Now the second image:"},
                    {"type":"input_image","image_url":blue}
                ]);
            }
            if round == 2 {
                let call = call.expect("round two carries a tool call");
                input.extend(call.history.clone());
                input.push(json!({"type":"function_call_output","call_id":call.call_id,"output":TOOL_OUTPUT}));
            }
            let mut value = json!({"model":label,"input":input,"max_output_tokens":case.cap()});
            if case == Case::JsonObject {
                value["text"] = json!({"format":{"type":"json_object"}});
            }
            if case == Case::Schema {
                value["text"] = json!({"format":{
                    "type":"json_schema","name":"ordered_answer","strict":true,
                    "schema":{"type":"object","properties":{
                        "z_answer":{"type":"integer"},
                        "a_label":{"type":"string","enum":["synthetic"]}},
                        "required":["z_answer","a_label"],"additionalProperties":false}
                }});
            }
            if case == Case::Tool {
                value["tools"] = json!([responses_tool()]);
                value["tool_choice"] = json!(if label == "deepseek-flash" {
                    "auto"
                } else if round == 1 {
                    "required"
                } else {
                    "none"
                });
            }
            value
        }
    };
    if matches!(case, Case::Schema | Case::Image) {
        // Match the SDK probe's explicit delivery flag for the same fixed input.
        value["stream"] = json!(streaming);
    }
    if streaming {
        value["stream"] = json!(true);
        if protocol == Profile::Chat {
            value["stream_options"] = json!({"include_usage":true,"include_obfuscation":false});
        }
    }
    value
}

fn chat_tool() -> Value {
    json!({"type":"function","function":{
        "name":"lookup",
        "description":"Look up the stored value for a key.",
        "parameters":{"type":"object","properties":{"key":{"type":"string"}},"required":["key"],"additionalProperties":false},
        "strict":false
    }})
}

fn responses_tool() -> Value {
    json!({"type":"function",
        "name":"lookup",
        "description":"Look up the stored value for a key.",
        "parameters":{"type":"object","properties":{"key":{"type":"string"}},"required":["key"],"additionalProperties":false},
        "strict":false
    })
}

fn chat_stream_options() -> StreamOptions {
    StreamOptions {
        include_usage: morphiecore::semantic::value::Presence::Value(true),
        include_obfuscation: morphiecore::semantic::value::Presence::Value(false),
    }
}

fn outcome_label(response: &GenerationResponse) -> String {
    match response.outcome() {
        Outcome::Completed => match response.continuation() {
            Continuation::Unreported => "completed:stop".into(),
            Continuation::ToolResults(_) => "completed:tool_calls".into(),
        },
        Outcome::Incomplete => "incomplete".into(),
        Outcome::Failed => "failed".into(),
        Outcome::Cancelled => "cancelled".into(),
    }
}

fn usage_value(usage: &Usage) -> Value {
    json!({
        "input_tokens": usage.input_tokens,
        "output_tokens": usage.output_tokens,
        "total_tokens": usage.total_tokens,
        "reasoning_tokens": usage.reasoning_tokens,
        "cached_input_tokens": usage.cached_input_tokens,
        "input_cache_write_tokens": usage.input_cache_write_tokens,
        "input_text_tokens": usage.input_text_tokens,
        "output_text_tokens": usage.output_text_tokens,
        "accepted_prediction_tokens": usage.accepted_prediction_tokens,
        "rejected_prediction_tokens": usage.rejected_prediction_tokens,
    })
}

fn error_class(error: &AttemptError) -> Option<ErrorClass> {
    match error {
        AttemptError::Status { class, .. } | AttemptError::Transport(class) => Some(*class),
        _ => None,
    }
}

struct CallOutcome {
    report: CallReport,
    tool_call: Option<ToolCallInfo>,
}

struct CallContext<'a> {
    out_dir: &'a str,
    label: &'a str,
    provider_name: &'a str,
    provider_def: &'a ProviderDefinition,
    secret: &'a SecretMaterial,
    endpoint: &'a Endpoint,
    public: &'a PublicModel,
    downstream: Profile,
    delivery: Delivery,
    case: Case,
}

fn write_capture(path: &str, body: &[u8], secret: &SecretMaterial) {
    if std::env::var("MORPHIECORE_PROBE_CAPTURE").as_deref() == Ok("1")
        && let Some(clean) = capture_bytes(body, secret)
    {
        let _ = std::fs::write(path, clean);
    }
}
fn capture_bytes(body: &[u8], secret: &SecretMaterial) -> Option<Vec<u8>> {
    if body.len() > CAPTURE_LIMIT
        || body
            .windows(secret.expose().len())
            .any(|w| w == secret.expose().as_bytes())
    {
        return None;
    }
    // Explicit synthetic forensic captures, not reusable wire or replay oracles.
    fn scrub(value: &mut Value) {
        match value {
            Value::Object(map) => {
                if map.get("error").is_some_and(|value| !value.is_null()) {
                    map.clear();
                    return;
                }
                let encrypted =
                    map.get("type").and_then(Value::as_str) == Some("reasoning.encrypted");
                map.retain(|key, _| {
                    let key = key.to_ascii_lowercase();
                    !matches!(
                        key.as_str(),
                        "encrypted_content"
                            | "authorization"
                            | "api_key"
                            | "access_token"
                            | "refresh_token"
                    ) && !key.contains("signature")
                        && !(encrypted && key == "data")
                });
                for value in map.values_mut() {
                    scrub(value);
                }
            }
            Value::Array(items) => {
                for value in items {
                    scrub(value);
                }
            }
            _ => {}
        }
    }
    let mut clean = Vec::new();
    if let Ok(mut value) = serde_json::from_slice::<Value>(body) {
        scrub(&mut value);
        if let Ok(bytes) = serde_json::to_vec(&value) {
            clean = bytes;
        }
    } else if let Ok(text) = std::str::from_utf8(body) {
        for line in text.lines().filter_map(|line| line.strip_prefix("data:")) {
            if line.trim() == "[DONE]" {
                clean.extend_from_slice(b"data: [DONE]\n\n");
                continue;
            }
            let Ok(mut value) = serde_json::from_str::<Value>(line.trim()) else {
                return None;
            };
            scrub(&mut value);
            let Ok(bytes) = serde_json::to_vec(&value) else {
                return None;
            };
            clean.extend_from_slice(b"data: ");
            clean.extend(bytes);
            clean.extend_from_slice(b"\n\n");
        }
    }
    (!clean.is_empty() && clean.len() <= CAPTURE_LIMIT).then_some(clean)
}

#[cfg(test)]
mod capture_tests {
    use super::*;
    #[test]
    fn explicit_capture_omits_opaque_and_refuses_credential_echo() {
        let secret = SecretMaterial::new("synthetic-private-secret-value").unwrap();
        assert!(capture_bytes(br#"{"error":"synthetic-private-secret-value"}"#, &secret).is_none());
        let body = br#"{"error":null,"output":[{"type":"reasoning","encrypted_content":"opaque-one"}],"reasoning_details":[{"type":"reasoning.encrypted","data":"opaque-two"}],"thinkingSignature":"opaque-three","text":"synthetic"}"#;
        let clean = String::from_utf8(capture_bytes(body, &secret).unwrap()).unwrap();
        assert!(!clean.contains("opaque-"));
        assert!(clean.contains("synthetic"));
        assert!(capture_bytes(b"not JSON or SSE", &secret).is_none());
    }
}

#[path = "support/probe_control.rs"]
mod probe_control;

async fn run_call(
    client: &reqwest::Client,
    ctx: &CallContext<'_>,
    request_json: Value,
    round: u8,
) -> CallOutcome {
    let protocol = if ctx.downstream == Profile::Chat {
        "chat"
    } else {
        "responses"
    };
    let scenario = format!(
        "native:{}:{}:{}:{}:{}",
        ctx.label,
        protocol,
        ctx.case.name(),
        ctx.delivery.name(),
        round
    );
    let cap = request_json
        .get("max_completion_tokens")
        .or_else(|| request_json.get("max_output_tokens"))
        .and_then(Value::as_u64)
        .expect("fixed request has a cap");
    probe_control::call("register", json!({"cases":[[ctx.label, scenario, cap]]})).await;
    let id = probe_control::call(
        "reserve",
        json!({"model":ctx.label,"scenario":scenario,"tokens":cap}),
    )
    .await;
    let outcome = run_call_inner(client, ctx, request_json, round, &id).await;
    let report = &outcome.report;
    let stage = match report.stage.as_str() {
        "request-decode" | "admission" => "admission",
        "prepare" => "prepare",
        "response-head" => "response_head",
        "transport" => "connect",
        "intake" => "intake",
        "terminal" => "terminal",
        "render" => "projection",
        "consumed" => "complete",
        _ => "oracle",
    };
    let state = if report.ok {
        "passed"
    } else if stage == "oracle" {
        "oracle_failed"
    } else {
        "failed"
    };
    probe_control::call(
        "finish",
        json!({"attempt":id,"state":state,"metrics":{
            "upstream_status":report.http_status,"stage":stage,"content_ok":report.ok,
            "elapsed_ms":report.latency_ms,"handed_off_bytes":report.delivered_bytes
        }}),
    )
    .await;
    outcome
}

async fn run_call_inner(
    client: &reqwest::Client,
    ctx: &CallContext<'_>,
    request_json: Value,
    round: u8,
    attempt_id: &str,
) -> CallOutcome {
    let started = Instant::now();
    let base = CallReport {
        model: ctx.label.into(),
        provider: ctx.provider_name.into(),
        protocol: match ctx.downstream {
            Profile::Chat => "chat",
            Profile::Responses => "responses",
        }
        .into(),
        case: ctx.case.name().into(),
        delivery: ctx.delivery.name().into(),
        round,
        stage: "start".into(),
        ok: false,
        http_status: None,
        error_class: None,
        outcome: None,
        usage: None,
        normalizations: vec![],
        text_chars: None,
        excerpt: None,
        tool_call: None,
        error: None,
        delivered_bytes: 0,
        latency_ms: 0,
    };
    let fail = |stage: &str,
                error_class: Option<String>,
                error: Option<String>,
                report: &mut CallReport| {
        report.stage = stage.into();
        report.error_class = error_class;
        report.error = error.map(|_| "private details suppressed".into());
        report.latency_ms = started.elapsed().as_millis() as u64;
    };

    // 1. Downstream request bytes must decode through the real admission codecs.
    let bytes = serde_json::to_vec(&request_json).expect("request serializes");
    let family = ctx.downstream;
    let client_adapter = downstream_adapter(family, &ctx.endpoint.representation);
    let decoded = match client_adapter.decode_request(&bytes) {
        Ok(request) => request,
        Err(error) => {
            let mut report = base;
            fail("request-decode", None, Some(error.to_string()), &mut report);
            return CallOutcome {
                report,
                tool_call: None,
            };
        }
    };

    // 2. Entry admission against the public contract, then candidate preparation.
    if let Err(error) = admit(ctx.public, &decoded) {
        let mut report = base;
        fail(
            "admission",
            error_class(&error).map(|c| format!("{c:?}")),
            Some(format!("{error}")),
            &mut report,
        );
        return CallOutcome {
            report,
            tool_call: None,
        };
    }
    let upstream = prepare(ctx.endpoint, ctx.provider_def, ctx.secret, &decoded);
    let upstream = match upstream {
        Ok(upstream) => upstream,
        Err(error) => {
            let mut report = base;
            fail(
                "prepare",
                error_class(&error).map(|c| format!("{c:?}")),
                Some(format!("{error}")),
                &mut report,
            );
            return CallOutcome {
                report,
                tool_call: None,
            };
        }
    };

    // Raw capture for wire diagnosis: bodies only, never headers or credentials.
    let raw_stem = format!(
        "{}/raw/{}-{}-{}-{}-r{}",
        ctx.out_dir,
        ctx.label,
        match ctx.downstream {
            Profile::Chat => "chat",
            Profile::Responses => "responses",
        },
        ctx.case.name(),
        ctx.delivery.name(),
        round
    );
    let _ = std::fs::create_dir_all(format!("{}/raw", ctx.out_dir));
    write_capture(&format!("{raw_stem}.req.json"), &upstream.body, ctx.secret);

    // 3. Transport attempt. The request carries no target, credential or profile.
    let mut call = client.post(format!("{}{}", upstream.origin, upstream.path));
    for (name, value) in &upstream.safe_headers {
        call = call.header(name, value);
    }
    call = call.header(&upstream.auth_header.0, &upstream.auth_header.1);
    probe_control::call("dispatched", json!({"attempt":attempt_id})).await;
    let mut response = match call.body(upstream.body).send().await {
        Ok(response) => response,
        Err(error) => {
            let mut report = base;
            let class = if error.is_timeout() {
                ErrorClass::Timeout
            } else {
                ErrorClass::Upstream
            };
            fail(
                "transport",
                Some(format!("{class:?}")),
                Some(format!("{error}")),
                &mut report,
            );
            return CallOutcome {
                report,
                tool_call: None,
            };
        }
    };
    let mut report = base;
    report.http_status = Some(response.status().as_u16());
    let content_type = response
        .headers()
        .get("content-type")
        .map(|v| v.to_str().unwrap_or_default().to_string())
        .unwrap_or_default();
    let mut attempt = Attempt::new(
        ctx.endpoint.adapter(),
        ctx.endpoint.execution.response_body_limit,
        SseLimits::default(),
    );
    if let Err(error) = attempt.begin(report.http_status.unwrap_or(0), &content_type) {
        let mut body = Vec::new();
        while let Ok(Some(chunk)) = response.chunk().await {
            if body.len() + chunk.len() > CAPTURE_LIMIT {
                break;
            }
            body.extend_from_slice(&chunk);
        }
        write_capture(&format!("{raw_stem}.resp.txt"), &body, ctx.secret);
        fail(
            "response-head",
            error_class(&error).map(|c| format!("{c:?}")),
            Some(format!("{error}")),
            &mut report,
        );
        return CallOutcome {
            report,
            tool_call: None,
        };
    }
    let mut raw_body: Vec<u8> = vec![];
    let mut delivery = ResponseDelivery::new(
        client_adapter.clone(),
        downstream_contract(&ctx.endpoint.representation, ctx.public.reported_facts),
        ctx.label,
        SseLimits::default(),
        chat_stream_options(),
        Obfuscation::Disabled,
    );
    // This diagnostic consumer retains bounded output; the execution chain does not.
    let mut rendered_stream = Vec::new();
    let mut intake_error: Option<AttemptError> = None;
    while let Some(chunk) = match response.chunk().await {
        Ok(chunk) => chunk,
        Err(error) => {
            let class = if error.is_timeout() {
                ErrorClass::Timeout
            } else {
                ErrorClass::Upstream
            };
            write_capture(&format!("{raw_stem}.resp.txt"), &raw_body, ctx.secret);
            fail(
                "transport",
                Some(format!("{class:?}")),
                Some(format!("{error}")),
                &mut report,
            );
            return CallOutcome {
                report,
                tool_call: None,
            };
        }
    } {
        if raw_body.len().saturating_add(chunk.len()) > CAPTURE_LIMIT {
            intake_error = Some(AttemptError::Limit);
            break;
        }
        raw_body.extend_from_slice(&chunk);
        if intake_error.is_none() {
            let mut rest = &chunk[..];
            while !rest.is_empty() {
                match attempt.push(rest) {
                    Ok((used, events)) if used > 0 => {
                        rest = &rest[used..];
                        if matches!(ctx.delivery, Delivery::Stream) {
                            match delivery.encode_events(&attempt, &events) {
                                Ok(frames) => {
                                    for frame in frames {
                                        if rendered_stream.len() + frame.len() > CAPTURE_LIMIT {
                                            intake_error = Some(AttemptError::Limit);
                                            break;
                                        }
                                        rendered_stream.extend_from_slice(&frame);
                                    }
                                }
                                Err(error) => {
                                    intake_error = Some(error);
                                    break;
                                }
                            }
                        }
                    }
                    Ok(_) => break,
                    Err(error) => {
                        intake_error = Some(error);
                        break;
                    }
                }
            }
        }
    }

    if raw_body
        .windows(ctx.secret.expose().len())
        .any(|w| w == ctx.secret.expose().as_bytes())
    {
        fail("sensitive-body", None, None, &mut report);
        return CallOutcome {
            report,
            tool_call: None,
        };
    }
    write_capture(&format!("{raw_stem}.resp.txt"), &raw_body, ctx.secret);
    if let Some(error) = intake_error {
        fail(
            "intake",
            error_class(&error).map(|c| format!("{c:?}")),
            Some(format!("{error}")),
            &mut report,
        );
        return CallOutcome {
            report,
            tool_call: None,
        };
    }

    // 4. Semantic terminal validation, then downstream delivery.
    let finished = match attempt.finish() {
        Ok(finished) => finished,
        Err(error) => {
            fail(
                "terminal",
                error_class(&error).map(|c| format!("{c:?}")),
                Some(format!("{error}")),
                &mut report,
            );
            return CallOutcome {
                report,
                tool_call: None,
            };
        }
    };
    let (text, mut tool_call) = extract(&finished.semantic);
    if let Some(call) = &mut tool_call
        && let Ok(projection) = client_adapter.encode_response(
            finished,
            &downstream_contract(&ctx.endpoint.representation, ctx.public.reported_facts),
        )
    {
        call.history = if family == Profile::Chat {
            vec![projection["choices"][0]["message"].clone()]
        } else {
            projection["output"].as_array().cloned().unwrap_or_default()
        };
    }
    let scenario_ok = finished.semantic.outcome()
        == if ctx.case == Case::Length {
            Outcome::Incomplete
        } else {
            Outcome::Completed
        }
        && matches!(
            finished.semantic.continuation(),
            Continuation::ToolResults(_)
        ) == (ctx.case == Case::Tool && round == 1)
        && match ctx.case {
            Case::Length => tool_call.is_none(),
            Case::Image => text.trim() == "red,blue",
            Case::Schema => serde_json::from_str::<Value>(&text).ok().is_some_and(|v| {
                v.as_object()
                    .is_some_and(|o| o.keys().map(String::as_str).eq(["z_answer", "a_label"]))
                    && v["z_answer"].as_u64() == Some(7)
                    && v["a_label"] == "synthetic"
            }),
            Case::Text => text.trim() == "pong",
            Case::JsonObject => serde_json::from_str::<Value>(&text)
                .ok()
                .is_some_and(|v| v["pong"].is_boolean() && v["note"].is_string()),
            Case::Tool if round == 1 => {
                tool_call.as_ref().is_some_and(|c| {
                    c.name == "lookup"
                        && !c.history.is_empty()
                        && c.arguments
                            .as_raw()
                            .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
                            .is_some_and(|v| v["key"] == "alpha")
                }) && finished
                    .semantic
                    .items()
                    .iter()
                    .filter(|(_, i)| matches!(i, Item::ToolCall(_)))
                    .count()
                    == 1
            }
            Case::Tool => tool_call.is_none() && text.contains("42"),
        };
    report.outcome = Some(outcome_label(&finished.semantic));
    report.usage = finished.semantic.usage().map(|u| usage_value(&u));
    report.normalizations = finished
        .fidelity
        .normalizations()
        .iter()
        .map(|rule| format!("{rule:?}"))
        .collect();
    report.text_chars = Some(text.chars().count());
    // Payload text and issuer identifiers remain in memory, never in reports.

    let delivered = match ctx.delivery {
        Delivery::Json => delivery.encode_json(&attempt),
        Delivery::Stream => delivery.finish_stream(&attempt).and_then(|frames| {
            for frame in frames {
                if rendered_stream.len() + frame.len() > CAPTURE_LIMIT {
                    return Err(AttemptError::Limit);
                }
                rendered_stream.extend_from_slice(&frame);
            }
            Ok(rendered_stream)
        }),
    };
    let delivered = match delivered {
        Ok(delivered) => delivered,
        Err(error) => {
            fail(
                "render",
                error_class(&error).map(|c| format!("{c:?}")),
                Some(format!("{error}")),
                &mut report,
            );
            return CallOutcome { report, tool_call };
        }
    };
    if delivered.len() > CAPTURE_LIMIT {
        fail("delivery-limit", None, None, &mut report);
        return CallOutcome { report, tool_call };
    }
    report.delivered_bytes = delivered.len();

    // 5. Client consumption of the delivered bytes through the same profile codec.
    let consumed = match ctx.delivery {
        Delivery::Json => client_adapter.decode_response(&delivered).is_ok(),
        Delivery::Stream => match ctx.downstream {
            Profile::Chat => consume_chat_stream(&delivered).is_ok(),
            Profile::Responses => consume_responses_stream(&delivered).is_ok(),
        },
    };
    if !consumed {
        fail(
            "consumption",
            Some("codec".into()),
            Some("delivered bytes failed client decode".into()),
            &mut report,
        );
        return CallOutcome { report, tool_call };
    }
    if let Err(error) = delivery.commit().and_then(|()| delivery.complete(&attempt)) {
        fail("delivery", None, Some(error.to_string()), &mut report);
        return CallOutcome { report, tool_call };
    }
    if !scenario_ok {
        fail(
            "scenario",
            None,
            Some("decoded response did not meet the scenario oracle".into()),
            &mut report,
        );
        return CallOutcome { report, tool_call };
    }
    report.stage = "consumed".into();
    report.ok = true;
    report.latency_ms = started.elapsed().as_millis() as u64;
    CallOutcome { report, tool_call }
}

fn extract(
    response: &morphiecore::semantic::task::generation::GenerationResponse,
) -> (String, Option<ToolCallInfo>) {
    let mut text = String::new();
    let mut call = None;
    for (_, item) in response.items() {
        match item {
            Item::Message(message) => {
                for part in &message.parts {
                    if let ContentPart::Text(content) = &part.content {
                        text.push_str(content.as_str());
                    }
                }
            }
            Item::ToolCall(tool) if call.is_none() => {
                call = Some(ToolCallInfo {
                    call_id: tool.call_id.as_str().into(),
                    name: tool.name.as_str().into(),
                    arguments: tool.arguments.clone(),
                    history: vec![],
                });
            }
            _ => {}
        }
    }
    (text, call)
}

fn consume_chat_stream(delivered: &[u8]) -> Result<(), String> {
    let mut decoder = ChatSseDecoder::with_decoder(
        200,
        "text/event-stream; charset=utf-8",
        SseLimits::default(),
        Adapter::new(Profile::Chat, Dialect::MorphieCore, None).event_decoder(),
    )
    .map_err(|e| e.to_string())?;
    feed(
        &mut |chunk| {
            decoder
                .consume(chunk)
                .map(|(used, _)| used)
                .map_err(|e| e.to_string())
        },
        delivered,
    )?;
    decoder.finish().map_err(|e| e.to_string())
}

fn consume_responses_stream(delivered: &[u8]) -> Result<(), String> {
    let mut decoder = ResponsesSseDecoder::new(
        200,
        "text/event-stream; charset=utf-8",
        SseLimits::default(),
        None,
    )
    .map_err(|e| e.to_string())?;
    feed(
        &mut |chunk| {
            decoder
                .consume(chunk)
                .map(|(used, _)| used)
                .map_err(|e| e.to_string())
        },
        delivered,
    )?;
    decoder.finish().map_err(|e| e.to_string())?;
    decoder.materialize().map(|_| ()).map_err(|e| e.to_string())
}

fn feed(
    consume: &mut dyn FnMut(&[u8]) -> Result<usize, String>,
    delivered: &[u8],
) -> Result<(), String> {
    let mut rest = delivered;
    while !rest.is_empty() {
        let used = consume(rest)?;
        if used == 0 {
            break;
        }
        rest = &rest[used..];
    }
    Ok(())
}

async fn model_listing(
    client: &reqwest::Client,
    origin: &str,
    path: &str,
    secret: &SecretMaterial,
) -> Result<Vec<String>, String> {
    let response = client
        .get(format!("{origin}{path}"))
        .header("authorization", format!("Bearer {}", secret.expose()))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let status = response.status().as_u16();
    if status != 200 {
        return Err(format!("HTTP {status}"));
    }
    let mut response = response;
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "model listing transport".to_string())?
    {
        if body.len() + chunk.len() > CAPTURE_LIMIT {
            return Err("model listing limit".into());
        }
        body.extend_from_slice(&chunk);
    }
    let value: Value = serde_json::from_slice(&body).map_err(|e| e.to_string())?;
    Ok(value["data"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item["id"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default())
}

fn report_dir() -> String {
    let run = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_millis();
    let date = std::process::Command::new("date")
        .arg("+%F")
        .output()
        .ok()
        .and_then(|out| String::from_utf8(out.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "undated".into());
    format!("testdata/runtime/probe-{date}-{run}-{}", std::process::id())
}

fn selection(name: &str, allowed: &[&str]) -> Option<String> {
    std::env::var(name).ok().inspect(|value| {
        if !allowed.contains(&value.as_str()) {
            eprintln!("invalid probe selection");
            std::process::exit(2);
        }
    })
}

/// Replay only the fixed synthetic matrix captures, without credentials or I/O to providers.
fn replay_capture(dir: &str) -> Result<(), String> {
    let topology = topology_catalog::default_topology().map_err(|e| e.to_string())?;
    let public = topology.model("gpt-6-luna").expect("fixed public binding");
    let protocol_only = selection("MORPHIECORE_PROBE_PROTOCOL", &["chat", "responses"]);
    let delivery_only = selection("MORPHIECORE_PROBE_DELIVERY", &["json", "sse"]);
    let case_only = selection(
        "MORPHIECORE_PROBE_CASE",
        &["text", "json_object", "tool", "schema", "image"],
    );
    let cases = match case_only.as_deref() {
        Some("schema") => &[Case::Schema][..],
        Some("image") => &[Case::Image][..],
        _ => &CASES[..],
    };
    let mut failures = 0;
    for (name, profile) in [("chat", Profile::Chat), ("responses", Profile::Responses)] {
        if protocol_only
            .as_deref()
            .is_some_and(|selected| selected != name)
        {
            continue;
        }
        let endpoint = topology
            .endpoint(&EndpointId::new(&format!("openrouter-{name}")).unwrap())
            .unwrap();
        for &case in cases {
            if case_only
                .as_deref()
                .is_some_and(|selected| selected != case.name())
            {
                continue;
            }
            for delivery in [Delivery::Json, Delivery::Stream] {
                if delivery_only
                    .as_deref()
                    .is_some_and(|selected| selected != delivery.name())
                {
                    continue;
                }
                let path = format!(
                    "{dir}/raw/gpt-6-luna-{name}-{}-{}-r1.resp.txt",
                    case.name(),
                    delivery.name()
                );
                let result = (|| -> Result<(), String> {
                    use std::io::Read;
                    let mut bytes = vec![];
                    std::fs::File::open(&path)
                        .map_err(|_| "missing capture")?
                        .take(CAPTURE_LIMIT as u64 + 1)
                        .read_to_end(&mut bytes)
                        .map_err(|_| "capture read")?;
                    if bytes.len() > CAPTURE_LIMIT {
                        return Err("capture limit".into());
                    }
                    let client = downstream_adapter(profile, &endpoint.representation);
                    let mut attempt =
                        Attempt::new(endpoint.adapter(), CAPTURE_LIMIT, SseLimits::default());
                    attempt
                        .begin(
                            200,
                            if delivery == Delivery::Json {
                                "application/json"
                            } else {
                                "text/event-stream"
                            },
                        )
                        .map_err(|e| e.to_string())?;
                    let mut output = ResponseDelivery::new(
                        client.clone(),
                        downstream_contract(&endpoint.representation, public.reported_facts),
                        "gpt-6-luna",
                        SseLimits::default(),
                        chat_stream_options(),
                        Obfuscation::Disabled,
                    );
                    let mut rest = bytes.as_slice();
                    let mut wire = vec![];
                    while !rest.is_empty() {
                        let (used, events) = attempt.push(rest).map_err(|e| e.to_string())?;
                        if used == 0 {
                            return Err("no progress".into());
                        }
                        rest = &rest[used..];
                        if delivery == Delivery::Stream {
                            wire.extend(
                                output
                                    .encode_events(&attempt, &events)
                                    .map_err(|e| e.to_string())?
                                    .into_iter()
                                    .flatten(),
                            );
                        }
                        if wire.len() > CAPTURE_LIMIT {
                            return Err("delivery limit".into());
                        }
                    }
                    attempt.finish().map_err(|e| e.to_string())?;
                    if delivery == Delivery::Json {
                        wire = output.encode_json(&attempt).map_err(|e| e.to_string())?;
                        client.decode_response(&wire).map_err(|e| e.to_string())?;
                    } else {
                        wire.extend(
                            output
                                .finish_stream(&attempt)
                                .map_err(|e| e.to_string())?
                                .into_iter()
                                .flatten(),
                        );
                        if profile == Profile::Chat {
                            consume_chat_stream(&wire)?;
                        } else {
                            consume_responses_stream(&wire)?;
                        }
                    }
                    Ok(())
                })();
                if result.is_err() {
                    failures += 1;
                }
                println!(
                    "replay {name} {} {}: {}",
                    case.name(),
                    delivery.name(),
                    result.err().unwrap_or_else(|| "consumed".into())
                );
            }
        }
    }
    if failures > 0 {
        Err(format!("{failures} replay failures"))
    } else {
        Ok(())
    }
}

#[tokio::main]
async fn main() {
    if let Ok(dir) = std::env::var("MORPHIECORE_PROBE_REPLAY_DIR") {
        if let Err(error) = replay_capture(&dir) {
            eprintln!("{error}");
            std::process::exit(1);
        }
        return;
    }
    if std::env::var("MORPHIECORE_PROBE").as_deref() != Ok("1") {
        eprintln!("live probe is an explicit paid gate; set MORPHIECORE_PROBE=1 to run");
        std::process::exit(2);
    }
    let only = selection(
        "MORPHIECORE_PROBE_MODEL",
        &[
            "deepseek-flash",
            "mimo-v2.6-pro",
            "mimo-v2.6-flash",
            "gpt-6-luna",
            "nemotron-3-super",
            "qwen3.8-max",
            "qwen3.8-flash",
            "glm-5.3",
            "glm-5.3-flash",
            "minicpm5-1b",
            "minicpm5-2b",
            "minicpm-v-4.6",
        ],
    );
    let only = Some(only.unwrap_or_else(|| "nemotron-3-super".into()));
    probe_control::call("check", json!({"model":only.as_deref().unwrap()})).await;
    let protocol_only = selection("MORPHIECORE_PROBE_PROTOCOL", &["chat", "responses"]);
    let case_only = selection(
        "MORPHIECORE_PROBE_CASE",
        &["text", "json_object", "tool", "length", "schema", "image"],
    );
    let delivery_only = selection("MORPHIECORE_PROBE_DELIVERY", &["json", "sse"]);
    if matches!(case_only.as_deref(), Some("schema" | "image"))
        && (protocol_only.as_deref() != Some("responses")
            || case_only.as_deref() == Some("image") && only.as_deref() != Some("gpt-6-luna"))
    {
        eprintln!("schema/image probes require an explicit supported Responses target");
        std::process::exit(2);
    }
    let cap = std::env::var("MORPHIECORE_PROBE_MAX_TOKENS").ok().map(|s| {
        s.parse::<u64>()
            .ok()
            .filter(|n| (1..=2048).contains(n))
            .unwrap_or_else(|| {
                eprintln!("probe token cap must be 1..=2048");
                std::process::exit(2)
            })
    });
    if std::env::var("MORPHIECORE_PROBE_LIST_MODELS").as_deref() == Ok("1")
        && MODELS
            .iter()
            .any(|spec| only.as_deref() == Some(spec.label) && spec.models_path.is_none())
    {
        eprintln!("model directory is not declared for this target; no request sent");
        std::process::exit(2);
    }
    let credentials_path =
        std::env::var("MORPHIECORE_PROBE_CREDENTIALS_DIR").unwrap_or_else(|_| {
            eprintln!("explicit MORPHIECORE_PROBE_CREDENTIALS_DIR is required");
            std::process::exit(2)
        });
    let credentials = match load_credentials(&credentials_path) {
        Ok(credentials) => credentials,
        Err(error) => {
            eprintln!("credential setup failed: {error}");
            std::process::exit(1);
        }
    };
    let secret_for = |pool: &str| -> SecretMaterial {
        let raw = credentials
            .pools
            .get(pool)
            .unwrap_or_else(|| panic!("missing credential pool {pool}"));
        SecretMaterial::new(raw).expect("validated at load")
    };

    let topology = topology_catalog::default_topology().expect("fixed topology compiles");
    // Egress goes through the environment's outbound proxy when one is set:
    // this is trusted tooling, not business request routing. Loopback tests
    // build their own `no_proxy` clients and are unaffected.
    let mut client_builder = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(30))
        .timeout(CALL_TIMEOUT)
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never());
    for variable in [
        "https_proxy",
        "HTTPS_PROXY",
        "http_proxy",
        "HTTP_PROXY",
        "all_proxy",
        "ALL_PROXY",
    ] {
        if let Ok(url) = std::env::var(variable) {
            match reqwest::Proxy::all(&url) {
                Ok(proxy) => {
                    client_builder = client_builder.proxy(proxy);
                    break;
                }
                Err(_) => continue,
            }
        }
    }
    let client = client_builder.build().expect("http client");
    let stamp = report_dir();
    let out_dir = format!(
        "{}/{}",
        std::env::var("MORPHIECORE_PROBE_RUN").expect("validated run"),
        std::path::Path::new(&stamp)
            .file_name()
            .unwrap()
            .to_string_lossy()
    );
    std::fs::create_dir_all(&out_dir).expect("report directory");

    let mut reports: Vec<CallReport> = vec![];
    let mut precheck = String::new();
    let mut eligible: Vec<&ModelSpec> = vec![];

    // Free model-listing precheck: never spend a paid call on a wrong model id.
    for spec in &MODELS {
        if only.as_ref().is_some_and(|label| label != spec.label)
            || only.is_none() && !matches!(spec.provider, "deepseek" | "xiaomi")
        {
            continue;
        }
        if std::env::var("MORPHIECORE_PROBE_LIST_MODELS").as_deref() != Ok("1") {
            eligible.push(spec);
            continue;
        }
        let models_path = spec
            .models_path
            .expect("directory selection checked before credentials");
        let directory_id = probe_control::call(
            "reserve",
            json!({"model":spec.label,"scenario":format!("native:{}:models",spec.label),"tokens":1}),
        ).await;
        probe_control::call("dispatched", json!({"attempt":directory_id})).await;
        let definition = topology.provider(spec.provider).expect("fixed provider");
        let secret = secret_for(spec.pool);
        match model_listing(&client, definition.origin.as_str(), models_path, &secret).await {
            Ok(ids) => {
                probe_control::call(
                    "finish",
                    json!({"attempt":directory_id,"state":"passed","metrics":{}}),
                )
                .await;
                let endpoint = topology
                    .endpoint(&EndpointId::new(spec.chat_endpoint).expect("id"))
                    .expect("endpoint");
                let found = ids.iter().any(|id| id == &endpoint.upstream_model);
                let _ = writeln!(
                    precheck,
                    "- {}: exact upstream model {}",
                    spec.label,
                    if found { "found" } else { "MISSING" }
                );
                if found {
                    eligible.push(spec);
                }
            }
            Err(_) => {
                probe_control::call(
                    "finish",
                    json!({"attempt":directory_id,"state":"failed","metrics":{"failure":"http"}}),
                )
                .await;
                let error = "private details suppressed";
                let _ = writeln!(
                    precheck,
                    "- {} (`{}`): listing failed: {} — paid calls skipped",
                    spec.label, models_path, error
                );
            }
        }
    }

    // Filters only narrow this fixed matrix; no automatic retries or extra models.
    for spec in &eligible {
        if only.as_ref().is_some_and(|label| label != spec.label) {
            continue;
        }
        let definition = topology.provider(spec.provider).expect("fixed provider");
        let secret = secret_for(spec.pool);
        let public = topology.model(spec.label).expect("public model binding");
        for protocol in [
            ProtocolProfile::OpenAiChat,
            ProtocolProfile::OpenAiResponses,
        ] {
            let protocol_name = if protocol == ProtocolProfile::OpenAiChat {
                "chat"
            } else {
                "responses"
            };
            if protocol_only.as_deref().is_some_and(|s| s != protocol_name) {
                continue;
            }
            let (endpoint_id, family) = match protocol {
                ProtocolProfile::OpenAiChat => (spec.chat_endpoint, Profile::Chat),
                ProtocolProfile::OpenAiResponses => match spec.responses_endpoint {
                    Some(id) => (id, Profile::Responses),
                    None => continue,
                },
                ProtocolProfile::AnthropicMessages => continue,
            };
            let endpoint = topology
                .endpoint(&EndpointId::new(endpoint_id).expect("endpoint id"))
                .expect("compiled endpoint");
            // The diagnostic truncation case is opt-in, never added to default matrices.
            let cases = match case_only.as_deref() {
                Some("length") => &[Case::Length][..],
                Some("schema") => &[Case::Schema][..],
                Some("image") => &[Case::Image][..],
                _ => &CASES[..],
            };
            for &case in cases {
                if case == Case::Length && protocol != ProtocolProfile::OpenAiChat {
                    continue;
                }
                if case_only.as_deref().is_some_and(|s| s != case.name()) {
                    continue;
                }
                for delivery in [Delivery::Json, Delivery::Stream] {
                    if delivery_only
                        .as_deref()
                        .is_some_and(|s| s != delivery.name())
                    {
                        continue;
                    }
                    let ctx = CallContext {
                        out_dir: &out_dir,
                        label: spec.label,
                        provider_name: spec.provider,
                        provider_def: definition,
                        secret: &secret,
                        endpoint,
                        public,
                        downstream: family,
                        delivery,
                        case,
                    };
                    let mut request = scenario_request(family, spec.label, case, delivery, 1, None);
                    if let Some(cap) = cap.filter(|_| case != Case::Length) {
                        request[if protocol == ProtocolProfile::OpenAiChat {
                            "max_completion_tokens"
                        } else {
                            "max_output_tokens"
                        }] = json!(cap);
                    }
                    let first = run_call(&client, &ctx, request, 1).await;
                    let mut tool_call = first.tool_call.clone();
                    let first_ok = first.report.ok;
                    println!(
                        "{} {} {} r1: {}",
                        protocol_name,
                        case.name(),
                        delivery.name(),
                        first.report.stage
                    );
                    reports.push(first.report);
                    if first_ok
                        && case == Case::Tool
                        && let Some(call) = tool_call.take()
                    {
                        sleep(CALL_INTERVAL).await;
                        let mut request =
                            scenario_request(family, spec.label, case, delivery, 2, Some(&call));
                        if let Some(cap) = cap {
                            request[if protocol == ProtocolProfile::OpenAiChat {
                                "max_completion_tokens"
                            } else {
                                "max_output_tokens"
                            }] = json!(cap);
                        }
                        let second = run_call(&client, &ctx, request, 2).await;
                        reports.push(second.report);
                    }
                    sleep(CALL_INTERVAL).await;
                }
            }
        }
    }

    // Sanitized report: matrix facts only, no keys, auth headers or locators.
    let jsonl = reports
        .iter()
        .map(|report| serde_json::to_string(report).expect("serializable"))
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(format!("{out_dir}/calls.jsonl"), format!("{jsonl}\n")).expect("write calls");

    let mut summary = String::new();
    let _ = writeln!(summary, "# Live probe summary\n");
    let _ = writeln!(summary, "## Model precheck\n{precheck}");
    let _ = writeln!(
        summary,
        "\n## Matrix ({} calls, {}/{} scenarios consumed)\n",
        reports.len(),
        reports.iter().filter(|r| r.ok).count(),
        reports.len()
    );
    let _ = writeln!(
        summary,
        "| model | protocol | case | delivery | round | stage | ok | http | class | outcome | text | usage | error | ms |"
    );
    let _ = writeln!(
        summary,
        "|---|---|---|---|---|---|---|---|---|---|---|---|---|---|"
    );
    for report in &reports {
        let error = report
            .error
            .as_deref()
            .map(|e| e.replace('|', "/").chars().take(80).collect::<String>())
            .unwrap_or_else(|| "-".into());
        let _ = writeln!(
            summary,
            "| {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |",
            report.model,
            report.protocol,
            report.case,
            report.delivery,
            report.round,
            report.stage,
            if report.ok { "yes" } else { "no" },
            report
                .http_status
                .map(|status| status.to_string())
                .unwrap_or_else(|| "-".into()),
            report.error_class.as_deref().unwrap_or("-"),
            report.outcome.as_deref().unwrap_or("-"),
            report
                .text_chars
                .map(|n| format!("{n} chars"))
                .unwrap_or_else(|| "-".into()),
            report
                .usage
                .as_ref()
                .map(|u| format!("in {} / out {}", u["input_tokens"], u["output_tokens"]))
                .unwrap_or_else(|| "-".into()),
            error,
            report.latency_ms,
        );
    }
    std::fs::write(format!("{out_dir}/summary.md"), &summary).expect("write summary");
    println!("{summary}");
    println!("reports written to {out_dir}/ (gitignored)");
    if reports.is_empty() || reports.iter().any(|r| !r.ok) {
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn downstream_reports_do_not_inherit_upstream_request_control_rejections() {
        let mut upstream = GenerationRepresentationContract::full();
        upstream.semantics.logprobs = false;
        upstream.adaptation.scope =
            Some(morphiecore::semantic::value::ReplayOrigin::new("synthetic-scope").unwrap());
        let client = downstream_adapter(Profile::Responses, &upstream);
        assert_eq!(
            client.adaptation,
            Adapter::new(
                Profile::Responses,
                Dialect::Standard,
                upstream.adaptation.scope.clone()
            )
            .adaptation
        );
        let target = downstream_contract(&upstream, ReportedFactPolicy::Faithful);
        assert_eq!(target.replay_origin, upstream.adaptation.scope);
        assert_eq!(
            downstream_contract(&upstream, ReportedFactPolicy::StrictComplete).reported_facts,
            ReportedFactPolicy::StrictComplete
        );
        let requested = client.decode_request(br#"{"model":"synthetic","input":"hi","include":["message.output_text.logprobs"],"top_logprobs":1}"#).unwrap();
        assert!(
            client
                .encode_request(&requested, "synthetic", &upstream)
                .is_err()
        );
        let body = json!({"id":"response-local","object":"response","model":"synthetic",
        "created_at":1,"status":"completed","usage":null,"output":[
            {"id":"message-local","type":"message","role":"assistant","status":"completed",
             "content":[{"type":"output_text","text":"synthetic","annotations":[],"logprobs":[]}]}
        ]});
        let mut attempt = Attempt::new(client.clone(), CAPTURE_LIMIT, SseLimits::default());
        attempt.begin(200, "application/json").unwrap();
        attempt.push(&serde_json::to_vec(&body).unwrap()).unwrap();
        attempt.finish().unwrap();
        assert!(
            client
                .encode_response(attempt.response().unwrap(), &upstream)
                .is_err()
        );
        let mut delivery = ResponseDelivery::new(
            client.clone(),
            target.clone(),
            "public",
            SseLimits::default(),
            chat_stream_options(),
            Obfuscation::Disabled,
        );
        let bytes = delivery.encode_json(&attempt).unwrap();
        let result: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(result["model"], "public");
        assert_eq!(result["output"], body["output"]);
        client.decode_response(&bytes).unwrap();
        delivery.commit().unwrap();
        delivery.complete(&attempt).unwrap();
        assert!(!upstream.semantics.logprobs);

        let mut initial = body.clone();
        initial["status"] = json!("in_progress");
        initial["output"] = json!([]);
        let item = &body["output"][0];
        let part = &item["content"][0];
        let events = [
            json!({"type":"response.created","response":initial}),
            json!({"type":"response.output_item.added","output_index":0,
                "item":{"id":"message-local","type":"message","role":"assistant","status":"in_progress","content":[]}}),
            json!({"type":"response.content_part.added","output_index":0,"item_id":"message-local","content_index":0,
                "part":{"type":"output_text","text":"","annotations":[],"logprobs":[]}}),
            json!({"type":"response.output_text.delta","output_index":0,"item_id":"message-local","content_index":0,
                "delta":"synthetic","logprobs":[]}),
            json!({"type":"response.output_text.done","output_index":0,"item_id":"message-local","content_index":0,
                "text":"synthetic","logprobs":[]}),
            json!({"type":"response.content_part.done","output_index":0,"item_id":"message-local","content_index":0,"part":part}),
            json!({"type":"response.output_item.done","output_index":0,"item":item}),
            json!({"type":"response.completed","response":body}),
        ];
        let mut attempt = Attempt::new(client.clone(), CAPTURE_LIMIT, SseLimits::default());
        attempt.begin(200, "text/event-stream").unwrap();
        let mut delivery = ResponseDelivery::new(
            client,
            target,
            "public",
            SseLimits::default(),
            chat_stream_options(),
            Obfuscation::Disabled,
        );
        let mut rendered = Vec::new();
        for (index, mut event) in events.into_iter().enumerate() {
            event["sequence_number"] = json!(index);
            let frame =
                morphiecore::protocol::openai::sse::encode_frame(&event, CAPTURE_LIMIT).unwrap();
            let (used, semantic) = attempt.push(&frame).unwrap();
            assert_eq!(used, frame.len());
            for frame in delivery.encode_events(&attempt, &semantic).unwrap() {
                rendered.extend_from_slice(&frame);
            }
        }
        attempt.finish().unwrap();
        for frame in delivery.finish_stream(&attempt).unwrap() {
            rendered.extend_from_slice(&frame);
        }
        let mut consumer =
            ResponsesSseDecoder::new(200, "text/event-stream", SseLimits::default(), None).unwrap();
        feed(
            &mut |chunk| {
                consumer
                    .consume(chunk)
                    .map(|(used, _)| used)
                    .map_err(|_| "synthetic stream".into())
            },
            &rendered,
        )
        .unwrap();
        consumer.finish().unwrap();
        let decoded = consumer.materialize().unwrap();
        assert_eq!(decoded.semantic, attempt.response().unwrap().semantic);
        assert_eq!(decoded.metadata.model, "public");
        delivery.commit().unwrap();
        delivery.complete(&attempt).unwrap();
    }

    #[test]
    fn schema_and_image_forensics_are_opt_in_and_match_fixed_synthetic_inputs() {
        use base64::Engine;
        use sha2::Digest;
        assert!(!CASES.contains(&Case::Schema));
        assert!(!CASES.contains(&Case::Image));
        let schema = scenario_request(
            Profile::Responses,
            "m",
            Case::Schema,
            Delivery::Json,
            1,
            None,
        );
        assert_eq!(schema["max_output_tokens"], 2048);
        assert_eq!(schema["text"]["format"]["strict"], true);
        assert_eq!(
            schema["text"]["format"]["schema"]["required"],
            json!(["z_answer", "a_label"])
        );
        assert_eq!(
            schema["text"]["format"]["schema"]["properties"]
                .as_object()
                .unwrap()
                .keys()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            ["z_answer", "a_label"],
        );
        let image = scenario_request(
            Profile::Responses,
            "m",
            Case::Image,
            Delivery::Json,
            1,
            None,
        );
        assert_eq!(image["max_output_tokens"], 512);
        let parts = image["input"][0]["content"].as_array().unwrap();
        assert_eq!(parts.len(), 4);
        // Digests of the independently generated SDK probe PNGs.
        for (index, expected) in [
            (
                1,
                "3d90e707ad8e4ee8083642b6b6497870978e8414ad9c332daac4257f5c690a87",
            ),
            (
                3,
                "09fe43177780ae8870512a285fed35df07c36bf0e7571500484fb473ee3d0230",
            ),
        ] {
            let data = parts[index]["image_url"]
                .as_str()
                .unwrap()
                .strip_prefix("data:image/png;base64,")
                .unwrap();
            let bytes = base64::prelude::BASE64_STANDARD.decode(data).unwrap();
            let digest: String = sha2::Sha256::digest(bytes)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect();
            assert_eq!(digest, expected);
        }
    }

    #[test]
    fn length_diagnostic_matches_sdk_boundary_without_expanding_default_matrix() {
        assert!(!CASES.contains(&Case::Length));
        let request = scenario_request(Profile::Chat, "m", Case::Length, Delivery::Json, 1, None);
        assert_eq!(request["max_completion_tokens"], 8);
        assert_eq!(request["messages"][0]["content"], LENGTH_PROMPT);
        assert!(request.get("tools").is_none());
    }
    #[test]
    fn continuation_uses_actual_reasoning_and_call_history() {
        for protocol in [Profile::Chat, Profile::Responses] {
            let history = if protocol == Profile::Chat {
                vec![
                    json!({"role":"assistant","content":null,"reasoning_details":[{"type":"reasoning.encrypted","data":"synthetic","id":"rs","index":0,"format":"openai-responses-v1"}],"tool_calls":[{"type":"function","id":"c","function":{"name":"lookup","arguments":"{\"key\":\"alpha\"}"}}]}),
                ]
            } else {
                vec![
                    json!({"type":"reasoning","id":"rs","summary":[],"encrypted_content":"synthetic"}),
                    json!({"type":"function_call","call_id":"c","name":"lookup","arguments":"{\"key\":\"alpha\"}"}),
                ]
            };
            let call = ToolCallInfo {
                call_id: "c".into(),
                name: "lookup".into(),
                arguments: "{\"key\":\"alpha\"}".into(),
                history: history.clone(),
            };
            let request =
                scenario_request(protocol, "m", Case::Tool, Delivery::Json, 2, Some(&call));
            let key = if protocol == Profile::Chat {
                "messages"
            } else {
                "input"
            };
            assert_eq!(
                &request[key].as_array().unwrap()[1..1 + history.len()],
                history.as_slice()
            );
            assert_eq!(request["tool_choice"], "none");
        }
    }
    #[test]
    fn credential_parse_errors_never_include_source_lines() {
        use std::io::Write;
        let directory = test_files::private_directory();
        let path = directory.path().join("synthetic.json");
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&path).unwrap();
        file.write_all(br#"{"api_keys":["synthetic-must-not-appear" invalid"#)
            .unwrap();
        drop(file);
        assert!(morphiecore::credential::read_private_file(&path, 4096).is_ok());
        let manager =
            morphiecore::credential::CredentialManager::new(directory.path(), vec![]).unwrap();
        assert!(matches!(
            manager.pools(),
            Err(morphiecore::credential::CredentialError::Storage)
        ));
        let error = load_credentials(directory.path().to_str().unwrap())
            .err()
            .unwrap();
        assert_eq!(error, "invalid credential configuration");
    }
}
