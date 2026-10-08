//! Official SDK -> production Router -> synthetic HTTP Provider -> the same Router.
#[allow(dead_code)]
#[path = "../support/tokenplan_audio.rs"]
mod native_audio;
#[path = "../support/gateway.rs"]
mod support;
use super::{ServerGuard, chat_sdk::wire as chat_wire, wire};
use axum::{
    Router, body::Body, extract::State, http::HeaderMap, response::Response, routing::post,
};
use morphiecore::gateway::Limits;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    process::Stdio,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{net::TcpListener, process::Command, sync::oneshot};
fn tool_response(turn: u8) -> Value {
    let mut response = wire::response(turn);
    if turn == 1 {
        response["output"].as_array_mut().unwrap().push(json!({
            "id":"function-large","type":"function_call","call_id":"c_lookup_large",
            "name":"lookup","arguments":" { \"n\": 9007199254740993 } ","status":"completed"
        }));
    }
    response
}
fn tool_events(turn: u8) -> Vec<Value> {
    let mut events = wire::events(turn);
    if turn == 1 {
        let mut terminal = events.pop().unwrap();
        terminal["response"] = tool_response(turn);
        events.extend([
            json!({"type":"response.output_item.added","output_index":4,
                "item":{"id":"function-large","type":"function_call","call_id":"c_lookup_large",
                    "name":"lookup","arguments":"","status":"in_progress"}}),
            json!({"type":"response.function_call_arguments.delta","output_index":4,
                "item_id":"function-large","delta":" { \"n\": 9007199254740993 } "}),
            json!({"type":"response.function_call_arguments.done","output_index":4,
                "item_id":"function-large","arguments":" { \"n\": 9007199254740993 } "}),
            json!({"type":"response.output_item.done","output_index":4,
                "item":tool_response(turn)["output"][4]}),
            terminal,
        ]);
    }
    events
}
fn owner_response() -> Value {
    let mut value = wire::response(2);
    value["output"] = json!([
        {"id":"empty-owner","type":"message","role":"assistant","content":[],"status":"completed"},
        {"id":"lookup-call","type":"function_call","call_id":"call-local","name":"lookup","arguments":"{\"n\":1}","status":"completed"}
    ]);
    value
}
fn owner_events() -> Vec<Value> {
    let completed = owner_response();
    let mut initial = completed.clone();
    initial["status"] = json!("in_progress");
    initial["output"] = json!([]);
    initial["usage"] = Value::Null;
    initial["completed_at"] = Value::Null;
    vec![
        json!({"type":"response.created","response":initial}),
        json!({"type":"response.output_item.added","output_index":0,"item":{"id":"empty-owner","type":"message","role":"assistant","content":[],"status":"in_progress"}}),
        json!({"type":"response.output_item.done","output_index":0,"item":completed["output"][0]}),
        json!({"type":"response.output_item.added","output_index":1,"item":{"id":"lookup-call","type":"function_call","call_id":"call-local","name":"lookup","arguments":"","status":"in_progress"}}),
        json!({"type":"response.function_call_arguments.delta","output_index":1,"item_id":"lookup-call","delta":"{\"n\":1}"}),
        json!({"type":"response.function_call_arguments.done","output_index":1,"item_id":"lookup-call","arguments":"{\"n\":1}"}),
        json!({"type":"response.output_item.done","output_index":1,"item":completed["output"][1]}),
        json!({"type":"response.completed","response":completed}),
    ]
}
#[derive(Clone, Copy, Eq, PartialEq, Ord, PartialOrd)]
struct Scenario {
    chat: bool,
    stream: bool,
    empty_owner: bool,
    nonstandard_usage: bool,
}
#[derive(Clone, Default)]
struct Observed(
    Arc<Mutex<BTreeMap<Scenario, u8>>>,
    Arc<std::sync::atomic::AtomicUsize>,
    Arc<std::sync::atomic::AtomicUsize>,
    Arc<std::sync::atomic::AtomicUsize>,
    Arc<std::sync::atomic::AtomicUsize>,
    Arc<std::sync::atomic::AtomicUsize>,
);
async fn provider(
    State(state): State<Observed>,
    headers: HeaderMap,
    axum::Json(request): axum::Json<Value>,
) -> Response {
    assert_eq!(
        headers["authorization"],
        "Bearer synthetic-upstream-credential-0001"
    );
    assert_eq!(request["model"], "private-model");
    let chat = request.get("messages").is_some();
    let stream = request["stream"] == true;
    let owner_case = request["metadata"]["case"] == "empty-owner";
    let nonstandard_usage = request["metadata"]["case"] == "nonstandard-usage";
    if owner_case {
        assert!(
            !chat,
            "SDK item validation uses a complete Responses envelope"
        );
    }
    assert_eq!(
        request[if chat {
            "max_completion_tokens"
        } else {
            "max_output_tokens"
        }],
        32
    );
    if !chat && !owner_case && !nonstandard_usage {
        // The fixed SDK must keep file description and ordered source parts
        // in the initial request and the history it appends for tool replay.
        assert_eq!(
            request["input"][0]["content"],
            json!([
                {"type":"input_text","text":"lookup"},
                {"type":"input_file","file_data":"data:application/pdf;base64,AQID","filename":"synthetic.pdf","detail":"low"},
                {"type":"input_image","image_url":"data:image/png;base64,AQID"},
                {"type":"input_file","file_url":"https://example.invalid/resource?sig=synthetic","filename":"remote.pdf"}
            ])
        );
    }
    let turn = {
        let mut seen = state.0.lock().unwrap();
        let count = seen
            .entry(Scenario {
                chat,
                stream,
                empty_owner: owner_case,
                nonstandard_usage,
            })
            .or_default();
        *count += 1;
        *count
    };
    assert!(
        turn <= if owner_case || nonstandard_usage {
            1
        } else {
            2
        }
    );
    if !chat && !owner_case && !nonstandard_usage {
        assert_eq!(
            request["reasoning"],
            json!({"summary": if turn == 1 { json!("auto") } else { Value::Null }})
        );
        assert_eq!(request["tools"][0]["strict"], true);
        assert_eq!(
            request["tools"][0]["parameters"],
            json!({
                "type":"object","properties":{"n":{"type":"integer"}},
                "required":["n"],"additionalProperties":false
            })
        );
        if turn == 2 {
            assert_eq!(
                request["text"],
                json!({"format":{
                    "type":"json_schema","name":"answer","strict":true,"schema":{
                        "type":"object","properties":{"ok":{"type":"boolean"}},
                        "required":["ok"],"additionalProperties":false}
                }})
            );
        } else {
            assert!(request.get("text").is_none());
        }
    }
    if turn == 2 {
        let history = request[if chat { "messages" } else { "input" }]
            .as_array()
            .unwrap();
        if chat {
            assert!(history.iter().any(|item| item["role"] == "tool"
                && item["tool_call_id"] == "call-local"
                && item["content"] == "{\"n\":1}"));
        } else {
            assert!(history.iter().any(|item|item["type"]=="function_call" && item["arguments"]=="{\"n\":1}"));
            assert_eq!(
                history
                    .iter()
                    .filter(|item| item["type"] == "function_call")
                    .map(|item| (
                        item["call_id"].as_str().unwrap(),
                        item["name"].as_str().unwrap(),
                        item["arguments"].as_str().unwrap()
                    ))
                    .collect::<Vec<_>>(),
                [
                    ("c_lookup", "lookup", "{\"n\":1}"),
                    ("c_lookup_large", "lookup", " { \"n\": 9007199254740993 } ")
                ]
            );
            assert_eq!(
                history
                    .iter()
                    .filter(|item| item["type"] == "function_call_output")
                    .cloned()
                    .collect::<Vec<_>>(),
                [
                    json!({"type":"function_call_output","call_id":"c_lookup_large","output":"{\"n\":9007199254740993}"}),
                    json!({"type":"function_call_output","call_id":"c_lookup","output":"{\"n\":1}"})
                ]
            );
            assert!(
                history
                    .iter()
                    .any(|item| item["type"] == "function_call_output"
                        && item["call_id"] == "c_lookup"
                        && item["output"] == "{\"n\":1}")
            );
            assert!(history.iter().any(
                |item| item["type"] == "custom_tool_call_output" && item["call_id"] == "c_sql"
            ));
            assert!(history.iter().any(|item| item["type"] == "reasoning"
                && item["encrypted_content"] == "synthetic-final-token"));
        }
    }
    let snapshot = |mut value: Value| {
        value["model"] = json!("private-model");
        if !chat {
            value["created_at"] = json!(1);
            if !value["completed_at"].is_null() {
                value["completed_at"] = json!(2);
            }
            value["max_output_tokens"] = json!(32);
            if nonstandard_usage && value["usage"].is_object() {
                value["usage"]["input_tokens_details"]["image_tokens"] = json!(1);
            }
        }
        value
    };
    let bytes = if stream {
        let mut output = Vec::new();
        let frames = if owner_case {
            owner_events()
        } else if chat {
            chat_wire::events(turn)
        } else {
            tool_events(if nonstandard_usage { 2 } else { turn })
        };
        for (sequence, mut value) in frames.into_iter().enumerate() {
            if !chat {
                value["sequence_number"] = json!(sequence);
            }
            if chat {
                value["model"] = json!("private-model");
            } else if value.get("response").is_some() {
                value["response"] = snapshot(value["response"].take());
            }
            if chat {
                output.extend_from_slice(format!("data: {value}\n\n").as_bytes());
            } else {
                output.extend_from_slice(
                    &morphiecore::protocol::openai::sse::encode_frame(&value, 1 << 20).unwrap(),
                );
            }
        }
        if chat {
            output.extend_from_slice(b"data: [DONE]\n\n");
        }
        output
    } else {
        serde_json::to_vec(&snapshot(if owner_case {
            owner_response()
        } else if chat {
            chat_wire::response(turn)
        } else {
            tool_response(if nonstandard_usage { 2 } else { turn })
        }))
        .unwrap()
    };
    Response::builder()
        .header(
            "content-type",
            if stream {
                "text/event-stream"
            } else {
                "application/json"
            },
        )
        .body(Body::from(bytes))
        .unwrap()
}
async fn image_provider(
    State(state): State<Observed>,
    headers: HeaderMap,
    axum::Json(request): axum::Json<Value>,
) -> Response {
    assert_eq!(
        headers["authorization"],
        "Bearer synthetic-upstream-credential-0001"
    );
    let index = state.1.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    assert!(index < 3);
    let count = if index == 0 { 1 } else { 2 };
    assert_eq!(
        request,
        json!({"model":"private-image","prompt":"synthetic image","n":count,"stream":false,"size":"1536x1024","quality":"high","background":"transparent","output_format":"webp","output_compression":80,"moderation":"low","user":"synthetic-sdk-user"})
    );
    let data = match index {
        0 => json!([{"b64_json":"AQID"}]),
        1 => json!([{"b64_json":"AQID"},{"b64_json":"BAUG"}]),
        _ => json!([{"b64_json":"AQID"},{"b64_json":"!"}]),
    };
    Response::builder().header("content-type","application/json").body(Body::from(json!({"created":7,"data":data,"size":"1536x1024","quality":"high","background":"transparent","output_format":"webp"}).to_string())).unwrap()
}
#[tokio::test]
#[ignore = "requires the pinned SDK; real gateway Router and synthetic HTTP Provider only"]
async fn sdk_uses_gateway_for_both_protocols_and_deliveries() {
    async fn native_provider(
        State(state): State<Observed>,
        headers: HeaderMap,
        axum::Json(request): axum::Json<Value>,
    ) -> Response {
        let speech = request["model"] == "private-native-speech";
        let index = if speech { &state.4 } else { &state.5 }
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        assert!(index < 3);
        if speech {
            native_audio::speech(&headers, &request, index == 2)
        } else {
            native_audio::transcription(&headers, &request, index == 2)
        }
    }
    async fn speech_provider(
        State(state): State<Observed>,
        headers: HeaderMap,
        axum::Json(request): axum::Json<Value>,
    ) -> Response {
        if request["model"] == "private-router-speech" {
            assert_eq!(
                headers["authorization"],
                "Bearer synthetic-router-credential-0001"
            );
            assert_eq!(
                request,
                json!({"model":"private-router-speech","input":"synthetic router speech","voice":"router-voice","response_format":"mp3"})
            );
            let index = state.3.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            assert!(index < 3);
            return Response::builder()
                .header(
                    "content-type",
                    if index == 2 {
                        "audio/pcm"
                    } else {
                        "audio/mpeg"
                    },
                )
                .body(Body::from("synthetic-router-audio"))
                .unwrap();
        }
        assert_eq!(
            headers["authorization"],
            "Bearer synthetic-speech-credential-0001"
        );
        assert_eq!(headers["accept"], "application/octet-stream");
        assert_eq!(
            request,
            json!({"model":"private-speech","input":"synthetic speech","voice":"alloy","instructions":"","speed":1.25,"response_format":"wav","stream_format":"audio"})
        );
        let index = state.2.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        assert!(index < 3);
        if index == 2 {
            return Response::builder()
                .header("content-type", "application/json")
                .body(Body::from(r#"{"error":"synthetic-private"}"#))
                .unwrap();
        }
        // Independent PCM WAV: 24 kHz mono, two signed 16-bit samples.
        let wav = b"RIFF\x28\0\0\0WAVEfmt \x10\0\0\0\x01\0\x01\0\xc0\x5d\0\0\x80\xbb\0\0\x02\0\x10\0data\x04\0\0\0\0\0\x01\0";
        Response::builder()
            .header(
                "content-type",
                if index == 0 {
                    "application/octet-stream"
                } else {
                    "audio/wav"
                },
            )
            .body(Body::from_stream(futures_util::stream::iter(
                wav.chunks(7)
                    .map(|chunk| Ok::<_, std::io::Error>(axum::body::Bytes::copy_from_slice(chunk)))
                    .collect::<Vec<_>>(),
            )))
            .unwrap()
    }
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let observed = Observed::default();
    let state = observed.clone();
    let upstream = tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new()
                .route("/chat/completions", post(provider))
                .route("/responses", post(provider))
                .route("/images/generations", post(image_provider))
                .route("/audio/speech", post(speech_provider))
                .route("/api/v1/audio/speech", post(speech_provider))
                .route(
                    "/api/v1/services/audio/tts/SpeechSynthesizer",
                    post(native_provider),
                )
                .route(
                    "/api/v1/services/aigc/multimodal-generation/generation",
                    post(native_provider),
                )
                .with_state(state),
        )
        .await
        .unwrap();
    });
    let _upstream = ServerGuard(upstream.abort_handle());
    let gateway = support::gateway(
        &origin,
        Limits {
            default_output_tokens: 32,
            ..Limits::default()
        },
    );
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}/v1", listener.local_addr().unwrap());
    let (stop, stopped) = oneshot::channel();
    let serving = tokio::spawn(gateway.serve(listener, async {
        let _ = stopped.await;
    }));
    let _server = ServerGuard(serving.abort_handle());
    let mut command = Command::new(
        std::env::var_os("MORPHIECORE_SDK_PYTHON").unwrap_or_else(|| "python3".into()),
    );
    command
        .args(["tests/sdk/gateway_text_loop.py", &base])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (name, _) in std::env::vars_os() {
        let text = name.to_string_lossy();
        if text.starts_with("OPENAI_") || text.starts_with("MORPHIECORE_") {
            command.env_remove(name);
        }
    }
    let output = super::child_process::run(&mut command, b"", Duration::from_secs(35), 64 << 10)
        .await
        .expect("bounded SDK gateway process");
    assert!(
        output.status.success(),
        "SDK gateway failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["requests"], 50);
    assert_eq!(report["model_requests"], 7);
    assert_eq!(report["native_audio_requests"], 10);
    assert_eq!(report["speech_requests"], 9);
    assert_eq!(observed.1.load(std::sync::atomic::Ordering::SeqCst), 3);
    assert_eq!(observed.2.load(std::sync::atomic::Ordering::SeqCst), 3);
    assert_eq!(observed.3.load(std::sync::atomic::Ordering::SeqCst), 3);
    assert_eq!(observed.4.load(std::sync::atomic::Ordering::SeqCst), 3);
    assert_eq!(observed.5.load(std::sync::atomic::Ordering::SeqCst), 3);
    {
        let observed = observed.0.lock().unwrap();
        assert_eq!(observed.len(), 8);
        assert!(observed.iter().all(|(scenario, count)| *count
            == if scenario.empty_owner || scenario.nonstandard_usage {
                1
            } else {
                2
            }));
    }
    stop.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(3), serving)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
}
