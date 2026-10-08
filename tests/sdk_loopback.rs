//! Explicit, ignored SDK gates: codec fixtures plus the actual gateway Router
//! against a synthetic HTTP Provider. All listeners and credentials are test-owned.
#[path = "sdk/chat_media.rs"]
mod chat_media_sdk;
#[path = "sdk/chat.rs"]
mod chat_sdk;
#[path = "../examples/support/child_process.rs"]
mod child_process;
#[path = "sdk/gateway.rs"]
mod gateway_sdk;
#[path = "sdk/process.rs"]
mod process_tests;
#[path = "support/responses_profile.rs"]
mod wire;

use std::{
    process::Stdio,
    sync::{Arc, Mutex},
    time::Duration,
};

use axum::{
    Router,
    body::{Body, Bytes},
    extract::State,
    http::{HeaderMap, StatusCode, header::CONTENT_TYPE},
    response::Response,
    routing::post,
};
use morphiecore::{
    lowering::generation::{GenerationRepresentationContract, lower_request, lower_response},
    protocol::openai::{
        Profile, envelope,
        sse::{Obfuscation, ResponsesSseDecoder, ResponsesSseEncoder, SseLimits, encode_frame},
    },
    semantic::{
        task::generation::{
            ContentPart, Continuation, GenerationResponse, Item, Outcome, StreamEvent,
        },
        value::{Presence, ReplayOrigin, Text},
    },
};
use serde_json::Value;
use tokio::{net::TcpListener, process::Command};

const SCRIPT: &str = "tests/sdk/responses_text_loop.py";

fn origin() -> ReplayOrigin {
    ReplayOrigin::new("synthetic-loopback").unwrap()
}

fn contract() -> GenerationRepresentationContract {
    let mut contract = GenerationRepresentationContract::full();
    contract.replay_origin = Some(origin());
    contract
}

#[derive(Clone, Default)]
struct Suite(
    Arc<Mutex<(usize, Option<&'static str>)>>,
    // Responses-only fixture variant: readable reasoning without reported opaque.
    bool,
);

fn failure(code: StatusCode, phase: &'static str, state: &Suite) -> Response {
    state.0.lock().unwrap().1 = Some(phase);
    Response::builder()
        .status(code)
        .body(Body::empty())
        .unwrap()
}

fn remove_opaque(item: &mut Value) {
    if item["type"] == "reasoning" {
        item.as_object_mut().unwrap().remove("encrypted_content");
    }
}

fn response_fixture(turn: u8, without_opaque: bool) -> Value {
    let mut response = wire::response(turn);
    // SDK v3's strict response model requires integer timestamps.
    response["created_at"] = 1.into();
    response["completed_at"] = 2.into();
    if without_opaque {
        for item in response["output"].as_array_mut().unwrap() {
            remove_opaque(item);
        }
    }
    response
}

fn assert_continuation(response: &GenerationResponse, turn: usize) {
    assert_eq!(response.outcome(), Outcome::Completed);
    if turn == 1 {
        let Continuation::ToolResults(calls) = response.continuation() else {
            panic!("completed response requires tool results")
        };
        assert_eq!(
            calls.iter().map(|c| c.call_id).collect::<Vec<_>>(),
            ["c_sql", "c_lookup"]
        );
    } else {
        assert_eq!(response.continuation(), Continuation::Unreported);
    }
}

async fn handle(State(state): State<Suite>, headers: HeaderMap, body: Bytes) -> Response {
    if headers.get("authorization").and_then(|v| v.to_str().ok())
        != Some("Bearer synthetic-local-token")
        || headers
            .get(CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .is_none_or(|v| !v.starts_with("application/json"))
    {
        return failure(StatusCode::UNAUTHORIZED, "local request boundary", &state);
    }
    let mut decoded = match envelope::decode_request_bytes(&body) {
        Ok(decoded) => decoded,
        Err(error) => {
            eprintln!("synthetic SDK request rejected: {error}");
            return failure(StatusCode::BAD_REQUEST, "envelope decode", &state);
        }
    };
    if decoded.context.model != "fixture-model"
        || decoded.task.fidelity.bind_replay_origin(&origin()).is_err()
    {
        return failure(
            StatusCode::BAD_REQUEST,
            "fixed model or replay binding",
            &state,
        );
    }
    let Ok(target) = lower_request(
        &decoded.task.semantic,
        &decoded.task.fidelity,
        Profile::Responses,
        contract(),
    ) else {
        return failure(StatusCode::BAD_REQUEST, "request lowering", &state);
    };
    let Ok(encoded) = envelope::encode_request(&target, &decoded.context) else {
        return failure(StatusCode::BAD_REQUEST, "request encoding", &state);
    };
    if encoded["store"] != false
        || encoded["model"] != "fixture-model"
        || encoded["input"].is_null()
    {
        return failure(StatusCode::BAD_REQUEST, "request projection", &state);
    }
    let turn = {
        let mut seen = state.0.lock().unwrap();
        seen.0 += 1;
        seen.0
    };
    if encoded["reasoning"] != serde_json::json!({"effort":"low","summary":"auto"}) {
        return failure(StatusCode::BAD_REQUEST, "reasoning controls", &state);
    }
    if turn >= 2 {
        let items: Vec<_> = encoded["input"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|item| item["type"] == "reasoning")
            .collect();
        let expected_opaque = (!state.1).then(|| Value::from("synthetic-final-token"));
        if items.len() != 1
            || items[0]["summary"] != serde_json::json!([{"type":"summary_text","text":"Plan 🧪"}])
            || items[0]["content"] != serde_json::json!([{"type":"reasoning_text","text":"Check"}])
            || items[0].get("encrypted_content") != expected_opaque.as_ref()
        {
            return failure(StatusCode::BAD_REQUEST, "reasoning history replay", &state);
        }
    }
    if turn > 3
        || turn == 2
            && encoded["input"].as_array().is_none_or(|items| {
                !items
                    .iter()
                    .any(|item| item["type"] == "custom_tool_call_output")
                    || !items
                        .iter()
                        .any(|item| item["type"] == "function_call_output")
            })
    {
        return failure(StatusCode::BAD_REQUEST, "two-turn tool results", &state);
    }
    // The replayed parsed view passed consistency admission; the raw body stays authoritative.
    if turn == 3
        && !encoded["input"].as_array().is_some_and(|items| {
            items.iter().any(|item| {
                item["content"]
                    .as_array()
                    .is_some_and(|parts| parts.iter().any(|part| part["text"] == "{\"ok\":true}"))
            })
        })
    {
        return failure(StatusCode::BAD_REQUEST, "parsed view replay", &state);
    }
    // Replayed function views never replace the authoritative raw arguments.
    if turn >= 2
        && !encoded["input"].as_array().is_some_and(|items| {
            items
                .iter()
                .any(|item| item["type"] == "function_call" && item["arguments"] == "{\"n\":1}")
        })
    {
        return failure(StatusCode::BAD_REQUEST, "parsed_arguments replay", &state);
    }
    let response = response_fixture(turn as u8, state.1);
    if !decoded.context.delivery.streaming() {
        let Ok(mut decoded) =
            envelope::decode_response_bytes(&serde_json::to_vec(&response).unwrap())
        else {
            return failure(StatusCode::INTERNAL_SERVER_ERROR, "response decode", &state);
        };
        decoded.fidelity.bind_replay_origin(&origin()).unwrap();
        assert_continuation(&decoded.semantic, turn);
        if turn == 2 {
            let mut items = decoded.semantic.items().to_vec();
            let Item::Message(message) = &mut items[0].1 else {
                unreachable!()
            };
            let ContentPart::Text(text) = &message.parts[0].content else {
                unreachable!()
            };
            message.parts[0].content = ContentPart::Text(
                text.clone()
                    .replace_text(Text::new("{\"ok\":true}", "synthetic", 100).unwrap()),
            );
            decoded.semantic = decoded.semantic.with_items(items).unwrap();
        }
        let Ok(lowered) = lower_response(
            &decoded.semantic,
            &decoded.fidelity,
            &decoded.metadata,
            Profile::Responses,
            contract(),
        ) else {
            return failure(
                StatusCode::INTERNAL_SERVER_ERROR,
                "response lowering",
                &state,
            );
        };
        let Ok(out) = envelope::encode_response(&lowered) else {
            return failure(
                StatusCode::INTERNAL_SERVER_ERROR,
                "response encoding",
                &state,
            );
        };
        return Response::builder()
            .header(CONTENT_TYPE, "application/json")
            .body(Body::from(out.to_string()))
            .unwrap();
    }

    let mut decoder = ResponsesSseDecoder::new(
        200,
        "text/event-stream",
        SseLimits::default(),
        Some(origin()),
    )
    .unwrap();
    let mut events = Vec::new();
    for mut payload in wire::events(turn as u8) {
        if state.1 {
            if let Some(item) = payload.get_mut("item") {
                remove_opaque(item);
            }
            if let Some(output) = payload
                .pointer_mut("/response/output")
                .and_then(Value::as_array_mut)
            {
                for item in output {
                    remove_opaque(item);
                }
            }
        }
        if payload["type"] == "response.created" || payload["type"] == "response.completed" {
            payload["response"]["created_at"] = 1.into();
            payload["response"]["completed_at"] = if payload["type"] == "response.created" {
                Value::Null
            } else {
                2.into()
            };
        }
        let frame = encode_frame(&payload, SseLimits::default().max_event_bytes).unwrap();
        let mut rest = frame.as_ref();
        while !rest.is_empty() {
            let Ok((used, mut next)) = decoder.consume(rest) else {
                return failure(StatusCode::INTERNAL_SERVER_ERROR, "event decode", &state);
            };
            assert!(used > 0);
            rest = &rest[used..];
            events.append(&mut next);
        }
    }
    if decoder.finish().is_err() {
        return failure(StatusCode::INTERNAL_SERVER_ERROR, "event closure", &state);
    }
    assert_continuation(&decoder.materialize().unwrap().semantic, turn);
    let Ok(mut encoder) = ResponsesSseEncoder::new(
        decoder.metadata().unwrap().clone(),
        contract(),
        SseLimits::default(),
        Obfuscation::Disabled,
    ) else {
        return failure(StatusCode::INTERNAL_SERVER_ERROR, "SSE encoder", &state);
    };
    // Capacity one prevents pre-encoding the stream or consuming unbounded producer output.
    // Dropping the response receiver interrupts a blocked send without emitting a terminal.
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Bytes, std::convert::Infallible>>(1);
    tokio::spawn(async move {
        for mut event in events {
            if turn == 2 {
                match &mut event {
                    StreamEvent::Delta {
                        fragment, logprobs, ..
                    } if fragment == "{\"ok\":false}" => {
                        *fragment = "{\"ok\":true}".into();
                        logprobs.clear();
                    }
                    StreamEvent::TextMetadata {
                        annotations,
                        logprobs,
                        ..
                    } => {
                        annotations.clear();
                        *logprobs = Presence::Absent;
                    }
                    StreamEvent::AnnotationAdded { .. } | StreamEvent::LogprobsSnapshot { .. } => {
                        continue;
                    }
                    _ => {}
                }
            }
            let Ok(frames) = encoder.encode(&event, decoder.fidelity()) else {
                return;
            };
            for frame in frames {
                if tx.send(Ok(frame)).await.is_err() {
                    return;
                }
            }
        }
        let _ = encoder.finish();
    });
    Response::builder()
        .header(CONTENT_TYPE, "text/event-stream")
        .body(Body::from_stream(futures_util::stream::unfold(
            rx,
            |mut receiver| async move { receiver.recv().await.map(|value| (value, receiver)) },
        )))
        .unwrap()
}

struct ServerGuard(tokio::task::AbortHandle);
impl Drop for ServerGuard {
    fn drop(&mut self) {
        self.0.abort();
    }
}

async fn sdk_case(sse: bool, profile: Profile, without_opaque: bool) {
    let suite = Suite(Default::default(), without_opaque);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let observed = suite.clone();
    let server = tokio::spawn(async move {
        let router = match profile {
            Profile::Responses => Router::new().route("/v1/responses", post(handle)),
            Profile::Chat => Router::new().route("/v1/chat/completions", post(chat_sdk::handle)),
        };
        axum::serve(listener, router.with_state(suite))
            .await
            .unwrap();
    });
    let guard = ServerGuard(server.abort_handle());
    let mut command = Command::new(
        std::env::var_os("MORPHIECORE_SDK_PYTHON").unwrap_or_else(|| "python3".into()),
    );
    command
        .args([
            if profile == Profile::Responses {
                SCRIPT
            } else {
                "tests/sdk/chat_text_loop.py"
            },
            &format!("http://{address}/v1"),
            if sse { "sse" } else { "json" },
        ])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if profile == Profile::Responses {
        command.arg(if without_opaque { "readable" } else { "opaque" });
    }
    for name in [
        "OPENAI_API_KEY",
        "OPENAI_BASE_URL",
        "OPENAI_ORG_ID",
        "OPENAI_PROJECT_ID",
        "MORPHIECORE_CONFIG",
    ] {
        command.env_remove(name);
    }
    let output = child_process::run(&mut command, b"", Duration::from_secs(35), 64 << 10)
        .await
        .expect("bounded SDK process");
    guard.0.abort();
    let _ = server.await;
    assert!(
        output.status.success(),
        "SDK rejected synthetic response (phase={:?}): {}",
        observed.0.lock().unwrap().1,
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(observed.0.lock().unwrap().0, 3);
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["turns"], 3);
    if profile == Profile::Responses {
        assert_eq!(report["opaque"], !without_opaque);
    }
    if sse && profile == Profile::Responses {
        assert!(report["event_counts"][0].as_u64().unwrap() > 5);
    }
}

#[tokio::test]
#[ignore = "requires locked tests/sdk Python environment; JSON/SSE synthetic loopback"]
async fn sdk_three_turn_text_json_and_sse() {
    for mode in [false, true] {
        for without_opaque in [false, true] {
            sdk_case(mode, Profile::Responses, without_opaque).await;
        }
    }
}

#[tokio::test]
#[ignore = "requires locked tests/sdk Python environment; Chat JSON/SSE synthetic loopback"]
async fn sdk_chat_three_turn_text_json_and_sse() {
    for mode in [false, true] {
        sdk_case(mode, Profile::Chat, false).await;
    }
}
