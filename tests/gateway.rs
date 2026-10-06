//! One real HTTP Router smoke and a bootstrap process gate; resource/failure
//! mechanisms live at their owners.
#[allow(dead_code)]
#[path = "support/chat_profile.rs"]
mod chat_wire;
#[path = "support/child_line.rs"]
mod child_line;
#[path = "../examples/support/child_process.rs"]
mod child_process;
#[path = "support/tokenplan_audio.rs"]
mod native_audio;
#[allow(dead_code)]
#[path = "support/responses_profile.rs"]
mod responses_wire;
#[path = "support/gateway.rs"]
mod support;
use axum::{
    Router,
    body::Body,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::Response,
    routing::post,
};
use morphiecore::{
    adapter::{Adapter, Dialect},
    gateway::Limits,
    lowering::generation::GenerationRepresentationContract,
    protocol::openai::{DecodedResponse, Profile, sse::ResponsesSseDecoder},
    semantic::{
        task::generation::{Continuation, Outcome},
        value::ReplayOrigin,
    },
};
use serde_json::{Value, json};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{net::TcpListener, sync::oneshot};

#[tokio::test]
async fn renamed_binaries_report_current_cli_names_without_loading_credentials() {
    for (binary, name) in [
        (env!("CARGO_BIN_EXE_morphiecore"), "morphiecore"),
        (env!("CARGO_BIN_EXE_morphiecore-auth"), "morphiecore-auth"),
    ] {
        let mut command = tokio::process::Command::new(binary);
        command.arg("--help").env_clear().kill_on_drop(true);
        let output = child_process::run(&mut command, b"", Duration::from_secs(5), 64 << 10)
            .await
            .expect("bounded CLI help");
        assert!(output.status.success());
        assert!(output.stderr.is_empty());
        let help = String::from_utf8(output.stdout).unwrap();
        assert!(help.contains(&format!("Usage: {name} ")), "{help}");
    }
}

#[derive(Clone, Default)]
struct Upstream(
    Arc<Mutex<Vec<Value>>>,
    Arc<tokio::sync::Notify>,
    Arc<tokio::sync::Notify>,
);
struct Guard(tokio::task::JoinHandle<()>);
impl Drop for Guard {
    fn drop(&mut self) {
        self.0.abort();
    }
}
async fn answer(
    State(state): State<Upstream>,
    headers: HeaderMap,
    axum::Json(request): axum::Json<Value>,
) -> Response {
    assert_eq!(
        headers["authorization"],
        "Bearer synthetic-upstream-credential-0001"
    );
    assert!(!headers.contains_key("x-never-forward"));
    assert_eq!(request["model"], "private-model");
    let chat = request.get("messages").is_some();
    let cap = if chat {
        "max_completion_tokens"
    } else {
        "max_output_tokens"
    };
    assert_eq!(
        request[cap], 32,
        "trusted default must pass through IR into upstream wire"
    );
    let probability_case = request["metadata"]["case"] == "probabilities";
    if probability_case {
        assert!(chat);
        assert_eq!(request["service_tier"], "fast");
        assert_eq!(request["logprobs"], true);
        assert_eq!(request["top_logprobs"], 1);
    }
    let probability = |token: &str| json!({"token":token,"logprob":-0.5,"bytes":token.as_bytes(),"top_logprobs":[]});
    let tool_image_case = request["metadata"]["case"] == "tool-images";
    if tool_image_case {
        assert!(!chat);
        assert_eq!(
            request["input"],
            json!([
                {"type":"function_call","call_id":"media-call","name":"lookup","arguments":"{}"},
                {"type":"function_call_output","call_id":"media-call","output":[
                    {"type":"input_text","text":"caption"},
                    {"type":"input_image","image_url":"data:image/png;base64,AQID"},
                    {"type":"input_image","image_url":"https://example.test/tool.png","detail":"low"}
                ]}
            ])
        );
    }
    let file_case = request["metadata"]["case"] == "files";
    if file_case {
        assert!(!chat);
        assert_eq!(
            request["input"][0]["content"],
            json!([
                {"type":"input_text","text":"first"},
                {"type":"input_file","file_data":"data:application/pdf;base64,AQID","filename":"synthetic.pdf","detail":"low"},
                {"type":"input_image","image_url":"data:image/png;base64,AQID"},
                {"type":"input_file","file_url":"https://example.invalid/file?sig=synthetic"},
                {"type":"input_text","text":"last"}
            ])
        );
    }
    let image_case = !tool_image_case
        && !file_case
        && request.to_string().contains("data:image/png;base64,AQID");
    if image_case {
        let expected = if chat {
            json!([
                {"type":"text","text":"first"},
                {"type":"image_url","image_url":{"url":"data:image/png;base64,AQID"}},
                {"type":"text","text":"second"},
                {"type":"image_url","image_url":{"url":"https://example.test/synthetic.png","detail":"low"}}
            ])
        } else {
            json!([
                {"type":"input_text","text":"first"},
                {"type":"input_image","image_url":"data:image/png;base64,AQID"},
                {"type":"input_text","text":"second"},
                {"type":"input_image","image_url":"https://example.test/synthetic.png","detail":"low"}
            ])
        };
        assert_eq!(
            request[if chat { "messages" } else { "input" }][0]["content"],
            expected
        );
    }
    let turn = if probability_case
        || file_case
        || image_case
        || request.to_string().contains("tool_call_id")
        || request.to_string().contains("function_call_output")
    {
        2
    } else {
        1
    };
    if !chat && !tool_image_case && request.to_string().contains("function_call_output") {
        let history = request["input"].as_array().unwrap();
        for expected in [
            json!({"type":"function_call_output","call_id":"c_lookup","output":"{\"n\":1}"}),
            json!({"type":"custom_tool_call_output","call_id":"c_sql","output":"1"}),
        ] {
            assert!(history.contains(&expected));
        }
        assert!(
            history
                .iter()
                .any(|i| i["type"] == "reasoning"
                    && i["encrypted_content"] == "synthetic-final-token")
        );
        assert!(history.iter().any(|i| i["type"] == "function_call"
            && i["call_id"] == "c_lookup"
            && i["arguments"] == "{\"n\":1}"));
    }
    state.0.lock().unwrap().push(request.clone());
    let stream = request["stream"] == true;
    let mut output = if chat {
        chat_wire::response(turn)
    } else {
        responses_wire::response(turn)
    };
    output["model"] = json!("private-model");
    if probability_case {
        output["choices"][0]["logprobs"] =
            json!({"content":[probability("old "),probability("🧪")]});
        output["service_tier"] = json!("default");
        output["metadata"] = json!({"provider":"reported"});
    }
    if !chat {
        output["created_at"] = json!(1);
        output["completed_at"] = json!(2);
    }
    let bytes = if stream {
        let mut frames = if chat {
            chat_wire::events(turn)
        } else {
            responses_wire::events(turn)
        };
        let mut bytes = Vec::new();
        for frame in &mut frames {
            if chat {
                frame["model"] = json!("private-model");
                if probability_case {
                    frame["service_tier"] = json!("default");
                    if let Some(fragment) = frame
                        .pointer("/choices/0/delta/content")
                        .and_then(Value::as_str)
                        .filter(|s| !s.is_empty())
                    {
                        frame["choices"][0]["logprobs"] =
                            json!({"content":[probability(fragment)]});
                    }
                }
            } else if let Some(snapshot) = frame.get_mut("response") {
                snapshot["model"] = json!("private-model");
                snapshot["created_at"] = json!(1);
                if !snapshot["completed_at"].is_null() {
                    snapshot["completed_at"] = json!(2);
                }
            }
            if chat {
                bytes.extend_from_slice(format!("data: {frame}\n\n").as_bytes());
            } else {
                bytes.extend_from_slice(
                    &morphiecore::protocol::openai::sse::encode_frame(frame, 1 << 20).unwrap(),
                );
            }
        }
        if chat {
            bytes.extend_from_slice(b"data: [DONE]\n\n");
        }
        bytes
    } else {
        serde_json::to_vec(&output).unwrap()
    };
    Response::builder()
        .status(StatusCode::OK)
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
fn responses_delivery(body: &[u8], streaming: bool) -> DecodedResponse {
    let scope = Some(ReplayOrigin::new("fixture").unwrap());
    if !streaming {
        return Adapter::new(Profile::Responses, Dialect::Standard, scope)
            .decode_response(body)
            .unwrap();
    }
    let mut decoder =
        ResponsesSseDecoder::new(200, "text/event-stream", Default::default(), scope).unwrap();
    for fragment in body.chunks(13) {
        let mut rest = fragment;
        while !rest.is_empty() {
            let (used, _) = decoder.consume(rest).unwrap();
            assert!(used > 0);
            rest = &rest[used..];
        }
    }
    decoder.finish().unwrap();
    decoder.materialize().unwrap()
}

async fn openrouter_image_answer(
    State(state): State<Upstream>,
    headers: HeaderMap,
    axum::Json(request): axum::Json<Value>,
) -> Response {
    assert_eq!(
        headers["authorization"],
        "Bearer synthetic-router-credential-0001"
    );
    assert!(!headers.contains_key("x-never-forward"));
    if request["prompt"] == "controlled-flare" {
        assert_eq!(
            request,
            json!({"model":"openai/gpt-image-2.5-flare","prompt":"controlled-flare","n":1,"stream":false,"quality":"high","background":"transparent","user":"synthetic-user-control","provider":{"only":["openai"],"allow_fallbacks":false,"options":{"openai":{"moderation":"low"}}}})
        );
    } else if request["prompt"].as_str().unwrap().starts_with("multi-") {
        assert_eq!(
            request,
            json!({"model":"openai/gpt-image-2.5-flare","prompt":request["prompt"],"n":2,"stream":false,"provider":{"only":["openai"],"allow_fallbacks":false}})
        );
        state.0.lock().unwrap().push(request.clone());
        let data = match request["prompt"].as_str().unwrap() {
            "multi-short" => json!([{"b64_json":"AQID","media_type":"image/png"}]),
            "multi-bad" => json!([{"b64_json":"AQID"},{"b64_json":"!"}]),
            "multi-mixed" => {
                json!([{"b64_json":"AQID","media_type":"image/png"},{"b64_json":"BAUG","media_type":"image/webp"}])
            }
            _ => {
                json!([{"b64_json":"AQID","media_type":"image/png"},{"b64_json":"BAUG","media_type":"image/png"}])
            }
        };
        let wire = json!({"created":456,"data":data}).to_string();
        return Response::builder()
            .header("content-type", "application/json")
            .body(Body::from(if request["prompt"] == "multi-tail" {
                wire + " trailing"
            } else {
                wire
            }))
            .unwrap();
    } else {
        assert_eq!(
            request,
            json!({"model":"openai/gpt-image-2.5-flare","prompt":"blue square","n":1,"stream":false,"provider":{"only":["openai"],"allow_fallbacks":false}})
        );
    }
    state.0.lock().unwrap().push(request);
    Response::builder().header("content-type","application/json").body(Body::from(json!({"created":456,"data":[{"b64_json":"BAUG","media_type":"image/png"}],"usage":{"prompt_tokens":3,"completion_tokens":5,"total_tokens":8,"cost":0.000123}}).to_string())).unwrap()
}
async fn image_answer(
    State(state): State<Upstream>,
    headers: HeaderMap,
    axum::Json(request): axum::Json<Value>,
) -> Response {
    assert_eq!(
        headers["authorization"],
        "Bearer synthetic-upstream-credential-0001"
    );
    assert!(!headers.contains_key("x-never-forward"));
    assert_eq!(request["model"], "private-image");
    assert!(request.get("max_output_tokens").is_none());
    state.0.lock().unwrap().push(request.clone());
    let (status, media, body) = match request["prompt"].as_str().unwrap() {
        "multi-pending" => {
            assert_eq!(request["n"], 2);
            let started = state.1.clone();
            let release = state.2.clone();
            let stream = futures_util::stream::unfold(
                (true, started, release),
                |(first, started, release)| async move {
                    if first {
                        let prefix = axum::body::Bytes::from_static(
                            br#"{"created":1,"data":[{"b64_json":"AQID"},"#,
                        );
                        Some((Ok::<_, std::io::Error>(prefix), (false, started, release)))
                    } else {
                        started.notify_one();
                        release.notified().await;
                        None
                    }
                },
            );
            return Response::builder().header("content-type", "application/json")
                .body(Body::from_stream(stream)).unwrap();
        },
        "controlled" => {
            assert_eq!(request,json!({"model":"private-image","prompt":"controlled","size":"1536x1024","quality":"high","background":"transparent","output_format":"webp","output_compression":80,"moderation":"low","user":"synthetic-user-control"}));
            (200,"application/json",json!({"created":123,"data":[{"b64_json":"AQID"}],"output_format":"webp","size":"1536x1024","background":"transparent","quality":"high"}).to_string())
        },
        "decoded-over-budget" => {
            use base64::Engine;
            let data=base64::engine::general_purpose::STANDARD.encode(vec![0;(2<<20)+1]);
            (200,"application/json",json!({"created":123,"data":[{"b64_json":data}],"output_format":"png"}).to_string())
        },
        "bad-json" => (200, "application/json", "{\"created\":1".into()),
        "wrong-format" => (200,"application/json",json!({"created":1,"data":[{"b64_json":"AQID"}],"output_format":"jpeg"}).to_string()),
        "truncated" => return Response::builder().header("content-type","application/json").body(Body::from_stream(futures_util::stream::iter([
            Ok(axum::body::Bytes::from_static(b"{\"created\":1,\"data\":[{\"b64_json\":\"AQID\"}]}")),
            Err(std::io::Error::other("synthetic truncated transport")),
        ]))).unwrap(),
        "missing-image" => (200, "application/json", "{\"created\":1,\"data\":[]}".into()),
        "wrong-media" => (200, "text/event-stream", "data: [DONE]".into()),
        "rate-limit" => (429, "application/json", "synthetic-private-error".into()),
        "redirect" => (302, "application/json", "synthetic-private-location".into()),
        "over-budget" => (200, "application/json", " ".repeat((4 << 20) + 1)),
        "timeout" => return Response::builder().header("content-type","application/json").body(Body::from_stream(futures_util::stream::pending::<Result<axum::body::Bytes,std::io::Error>>())).unwrap(),
        _ => (200, "application/json", json!({"created":123,"data":[{"b64_json":"AQID"}],"output_format":"png","size":"1024x1024"}).to_string()),
    };
    Response::builder()
        .status(status)
        .header("content-type", media)
        .body(Body::from(body))
        .unwrap()
}

#[tokio::test]
async fn real_router_uses_provider_http_for_json_sse_tools_and_cross_profile() {
    async fn native_answer(
        State(state): State<Upstream>,
        headers: HeaderMap,
        axum::Json(request): axum::Json<Value>,
    ) -> Response {
        state.0.lock().unwrap().push(request.clone());
        if request["model"] == "private-native-speech" {
            native_audio::speech(&headers, &request, false)
        } else {
            native_audio::transcription(&headers, &request, false)
        }
    }
    async fn speech_answer(
        State(state): State<Upstream>,
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
                json!({"model":"private-router-speech","input":"router speech","voice":"router-voice","response_format":"mp3"})
            );
            assert!(!headers.contains_key("x-never-forward"));
            state.0.lock().unwrap().push(request);
            // Opaque fixture tests transport, not MP3 decoding or generated quality.
            return Response::builder()
                .header("content-type", "audio/mpeg")
                .body(Body::from("synthetic-router-audio"))
                .unwrap();
        }
        assert_eq!(
            headers["authorization"],
            "Bearer synthetic-speech-credential-0001"
        );
        assert_eq!(headers["accept"], "application/octet-stream");
        assert!(!headers.contains_key("x-never-forward"));
        assert_eq!(request["model"], "private-speech");
        assert_eq!(request["voice"], "alloy");
        assert_eq!(request["instructions"], "");
        assert_eq!(request["speed"], 1.25);
        assert_eq!(request["stream_format"], "audio");
        assert!(request.get("max_output_tokens").is_none());
        state.0.lock().unwrap().push(request.clone());
        let media = if request["response_format"] == "wav" {
            "audio/wav"
        } else {
            "application/octet-stream"
        };
        Response::builder()
            .header("content-type", media)
            .body(Body::from_stream(futures_util::stream::iter([
                Ok::<_, std::io::Error>(axum::body::Bytes::from_static(b"synthetic-")),
                Ok(axum::body::Bytes::from_static(b"audio")),
            ])))
            .unwrap()
    }
    let upstream = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", upstream.local_addr().unwrap());
    let observed = Upstream::default();
    let app = Router::new()
        .route("/chat/completions", post(answer))
        .route("/responses", post(answer))
        .route("/images/generations", post(image_answer))
        .route("/api/v1/images", post(openrouter_image_answer))
        .route("/audio/speech", post(speech_answer))
        .route("/api/v1/audio/speech", post(speech_answer))
        .route(
            "/api/v1/services/audio/tts/SpeechSynthesizer",
            post(native_answer),
        )
        .route(
            "/api/v1/services/aigc/multimodal-generation/generation",
            post(native_answer),
        )
        .with_state(observed.clone());
    let upstream_guard = Guard(tokio::spawn(async move {
        axum::serve(upstream, app).await.unwrap();
    }));
    let gateway = support::gateway(
        &origin,
        Limits {
            default_output_tokens: 32,
            ..Limits::default()
        },
    );
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let (shutdown, stopped) = oneshot::channel();
    let serving = tokio::spawn(gateway.serve(listener, async {
        let _ = stopped.await;
    }));
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(3))
        .build()
        .unwrap();
    for profile in [Profile::Chat, Profile::Responses] {
        for stream in [false, true] {
            let path = if profile == Profile::Chat {
                "/v1/chat/completions"
            } else {
                "/v1/responses"
            };
            let request = if profile == Profile::Chat {
                json!({"model":"public-model","messages":[{"role":"user","content":"lookup"}],"stream":stream,"stream_options":if stream {json!({"include_usage":true,"include_obfuscation":false})}else{Value::Null}})
            } else {
                json!({"model":"public-model","input":"lookup","stream":stream})
            };
            let response = client
                .post(format!("{url}{path}"))
                .bearer_auth(support::CLIENT_KEY)
                .header("x-never-forward", "private-input")
                .json(&request)
                .send()
                .await
                .unwrap();
            assert_eq!(response.status(), 200);
            let body = response.bytes().await.unwrap();
            assert!(!String::from_utf8_lossy(&body).contains("private-model"));
            if !stream {
                let value: Value = serde_json::from_slice(&body).unwrap();
                assert_eq!(value["model"], "public-model");
                if profile == Profile::Chat {
                    assert_eq!(
                        value["choices"][0]["message"]["tool_calls"][0]["id"],
                        "call-local"
                    );
                } else {
                    assert!(
                        value["output"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .any(|i| i["call_id"] == "c_lookup")
                    );
                }
            } else {
                assert!(
                    String::from_utf8_lossy(&body).contains(if profile == Profile::Chat {
                        "[DONE]"
                    } else {
                        "response.completed"
                    })
                );
            }
            if profile == Profile::Responses {
                let decoded = responses_delivery(&body, stream);
                assert_eq!(decoded.semantic.outcome(), Outcome::Completed);
                let Continuation::ToolResults(calls) = decoded.semantic.continuation() else {
                    panic!("completed tool response still needs results")
                };
                assert_eq!(
                    calls.iter().map(|c| c.call_id).collect::<Vec<_>>(),
                    ["c_sql", "c_lookup"]
                );
                let contract = GenerationRepresentationContract {
                    replay_origin: Some(ReplayOrigin::new("fixture").unwrap()),
                    ..GenerationRepresentationContract::full()
                };
                let projected = Adapter::new(Profile::Responses, Dialect::Standard, None)
                    .encode_response(&decoded, &contract)
                    .unwrap();
                let mut history = vec![json!({"role":"user","content":"lookup"})];
                history.extend(projected["output"].as_array().unwrap().iter().cloned());
                for call in calls {
                    // Synthetic results only; call arguments never execute code.
                    history.push(match call.call_id {
                        "c_sql" => json!({"type":"custom_tool_call_output","call_id":call.call_id,"output":"1"}),
                        "c_lookup" => json!({"type":"function_call_output","call_id":call.call_id,"output":"{\"n\":1}"}),
                        _ => panic!("unexpected call identity"),
                    });
                }
                let followup = client
                    .post(format!("{url}{path}"))
                    .bearer_auth(support::CLIENT_KEY)
                    .json(&json!({"model":"public-model","input":history,"stream":stream}))
                    .send()
                    .await
                    .unwrap();
                assert_eq!(followup.status(), 200);
                let finished = responses_delivery(&followup.bytes().await.unwrap(), stream);
                assert_eq!(finished.semantic.outcome(), Outcome::Completed);
                assert_eq!(finished.semantic.continuation(), Continuation::Unreported);
                assert_eq!(finished.semantic.usage().unwrap().total_tokens, Some(8));

                let before = observed.0.lock().unwrap().len();
                history.last_mut().unwrap()["call_id"] = json!("unmatched-call");
                let rejected = client
                    .post(format!("{url}{path}"))
                    .bearer_auth(support::CLIENT_KEY)
                    .json(&json!({"model":"public-model","input":history,"stream":stream}))
                    .send()
                    .await
                    .unwrap();
                assert_eq!(rejected.status(), 400);
                assert_eq!(observed.0.lock().unwrap().len(), before);
            }
        }
    }
    // New controls and reported probabilities traverse the actual HTTP chain;
    // metadata is a static reported fact, never a request echo or a chunk field.
    for stream in [false, true] {
        let response = client.post(format!("{url}/v1/chat/completions"))
            .bearer_auth(support::CLIENT_KEY)
            .json(&json!({"model":"public-model","messages":[{"role":"user","content":"hi"}],"stream":stream,"metadata":{"case":"probabilities"},"service_tier":"fast","logprobs":true,"top_logprobs":1}))
            .send().await.unwrap();
        assert_eq!(response.status(), 200);
        let bytes = response.bytes().await.unwrap();
        let decoded = if stream {
            let mut decoder = morphiecore::protocol::openai::chat_sse::ChatSseDecoder::new(
                200,
                "text/event-stream",
                Default::default(),
            )
            .unwrap();
            let mut rest = bytes.as_ref();
            while !rest.is_empty() {
                let (used, _) = decoder.consume(rest).unwrap();
                assert!(used > 0);
                rest = &rest[used..];
            }
            decoder.finish().unwrap();
            decoder.materialize().unwrap()
        } else {
            let wire: Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(wire["metadata"], json!({"provider":"reported"}));
            morphiecore::adapter::Adapter::new(
                Profile::Chat,
                morphiecore::adapter::Dialect::Standard,
                None,
            )
            .decode_response(&bytes)
            .unwrap()
        };
        let morphiecore::semantic::task::generation::Item::Message(message) =
            &decoded.semantic.items()[0].1
        else {
            panic!("message")
        };
        let morphiecore::semantic::task::generation::ContentPart::Text(text) =
            &message.parts[0].content
        else {
            panic!("text")
        };
        assert_eq!(text.as_str(), "old 🧪");
        assert_eq!(text.logprobs().value().unwrap().len(), 2);
        assert_eq!(
            decoded.metadata.context.execution.service_tier,
            morphiecore::semantic::value::Presence::Value(
                morphiecore::semantic::context::ServiceTier::Default
            )
        );
        if stream {
            assert!(decoded.metadata.context.execution.metadata.is_absent());
        }
    }
    // Images traverse the same request IR and real provider I/O, including cross-wire.
    for (profile, model) in [
        (Profile::Chat, "public-model"),
        (Profile::Responses, "public-model"),
        (Profile::Responses, "cross-model"),
    ] {
        for stream in [false, true] {
            let parts = if profile == Profile::Chat {
                json!([
                    {"type":"text","text":"first"},
                    {"type":"image_url","image_url":{"url":"data:image/png;base64,AQID"}},
                    {"type":"text","text":"second"},
                    {"type":"image_url","image_url":{"url":"https://example.test/synthetic.png","detail":"low"}}
                ])
            } else {
                json!([
                    {"type":"input_text","text":"first"},
                    {"type":"input_image","image_url":"data:image/png;base64,AQID"},
                    {"type":"input_text","text":"second"},
                    {"type":"input_image","image_url":"https://example.test/synthetic.png","detail":"low"}
                ])
            };
            let request = if profile == Profile::Chat {
                json!({"model":model,"messages":[{"role":"user","content":parts}],"stream":stream,"stream_options":if stream {json!({"include_usage":true,"include_obfuscation":false})}else{Value::Null}})
            } else {
                json!({"model":model,"input":[{"role":"user","content":parts}],"stream":stream})
            };
            let path = if profile == Profile::Chat {
                "/v1/chat/completions"
            } else {
                "/v1/responses"
            };
            let response = client
                .post(format!("{url}{path}"))
                .bearer_auth(support::CLIENT_KEY)
                .json(&request)
                .send()
                .await
                .unwrap();
            assert_eq!(response.status(), 200);
            let bytes = response.bytes().await.unwrap();
            let decoded = if !stream {
                morphiecore::adapter::Adapter::new(
                    profile,
                    morphiecore::adapter::Dialect::Standard,
                    None,
                )
                .decode_response(&bytes)
                .unwrap()
            } else if profile == Profile::Chat {
                let mut decoder = morphiecore::protocol::openai::chat_sse::ChatSseDecoder::new(
                    200,
                    "text/event-stream",
                    Default::default(),
                )
                .unwrap();
                let mut rest = bytes.as_ref();
                while !rest.is_empty() {
                    let (used, _) = decoder.consume(rest).unwrap();
                    assert!(used > 0);
                    rest = &rest[used..];
                }
                decoder.finish().unwrap();
                decoder.materialize().unwrap()
            } else {
                let mut decoder = morphiecore::protocol::openai::sse::ResponsesSseDecoder::new(
                    200,
                    "text/event-stream",
                    Default::default(),
                    None,
                )
                .unwrap();
                let mut rest = bytes.as_ref();
                while !rest.is_empty() {
                    let (used, _) = decoder.consume(rest).unwrap();
                    assert!(used > 0);
                    rest = &rest[used..];
                }
                decoder.finish().unwrap();
                decoder.materialize().unwrap()
            };
            assert_eq!(
                decoded.semantic.outcome(),
                morphiecore::semantic::task::generation::Outcome::Completed
            );
            let morphiecore::semantic::task::generation::Item::Message(message) =
                &decoded.semantic.items()[0].1
            else {
                panic!("text output")
            };
            let expected = if profile == Profile::Responses && model == "public-model" {
                "{\"ok\":false}"
            } else {
                "old 🧪"
            };
            assert!(
                matches!(&message.parts[0].content,morphiecore::semantic::task::generation::ContentPart::Text(text) if text.as_str()==expected)
            );
        }
    }
    // Tool image results are a Responses history carrier, not a user message
    // or a Chat tool-result extension. No URL/inline bytes are fetched locally.
    for stream in [false, true] {
        let request = json!({"model":"public-model","metadata":{"case":"tool-images"},"stream":stream,"input":[
            {"type":"function_call","call_id":"media-call","name":"lookup","arguments":"{}"},
            {"type":"function_call_output","call_id":"media-call","output":[
                {"type":"input_text","text":"caption"},
                {"type":"input_image","image_url":"data:image/png;base64,AQID"},
                {"type":"input_image","image_url":"https://example.test/tool.png","detail":"low"}
            ]}
        ]});
        let before = observed.0.lock().unwrap().len();
        let response = client
            .post(format!("{url}/v1/responses"))
            .bearer_auth(support::CLIENT_KEY)
            .json(&request)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let decoded = responses_delivery(&response.bytes().await.unwrap(), stream);
        assert_eq!(decoded.semantic.outcome(), Outcome::Completed);
        assert_eq!(observed.0.lock().unwrap().len(), before + 1);
        let mut unrepresentable = request;
        unrepresentable["model"] = json!("cross-model");
        let response = client
            .post(format!("{url}/v1/responses"))
            .bearer_auth(support::CLIENT_KEY)
            .json(&unrepresentable)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 400);
        assert_eq!(observed.0.lock().unwrap().len(), before + 1);
    }
    // Inline/URL files traverse real request I/O without resource fetching. The
    // synthetic fixture proves representation, not a valid PDF or model parsing.
    for stream in [false, true] {
        let request = json!({"model":"public-model","metadata":{"case":"files"},"stream":stream,"input":[{
            "role":"user","content":[
                {"type":"input_text","text":"first"},
                {"type":"input_file","file_data":"data:application/pdf;base64,AQID","filename":"synthetic.pdf","detail":"low"},
                {"type":"input_image","image_url":"data:image/png;base64,AQID"},
                {"type":"input_file","file_url":"https://example.invalid/file?sig=synthetic"},
                {"type":"input_text","text":"last"}
            ]
        }]});
        let before = observed.0.lock().unwrap().len();
        let response = client
            .post(format!("{url}/v1/responses"))
            .bearer_auth(support::CLIENT_KEY)
            .json(&request)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let decoded = responses_delivery(&response.bytes().await.unwrap(), stream);
        assert_eq!(decoded.semantic.outcome(), Outcome::Completed);
        assert_eq!(observed.0.lock().unwrap().len(), before + 1);
        for case in 0..5 {
            let mut rejected = request.clone();
            match case {
                0 => rejected["model"] = json!("cross-model"),
                1 => rejected["model"] = json!("no-files-model"),
                2 => rejected["input"][0]["role"] = json!("assistant"),
                3 => {
                    rejected["input"][0]["content"][1] =
                        json!({"type":"input_file","file_url":"file:///tmp/private.pdf"})
                }
                _ => rejected["input"][0]["content"][1]["file_id"] = json!("file-synthetic"),
            }
            let response = client
                .post(format!("{url}/v1/responses"))
                .bearer_auth(support::CLIENT_KEY)
                .json(&rejected)
                .send()
                .await
                .unwrap();
            assert_eq!(response.status(), 400);
            assert_eq!(observed.0.lock().unwrap().len(), before + 1);
        }
    }
    // Private attachments fail admission before any Provider I/O.
    for input in [
        json!({"model":"public-model","input":"hi","_openbridge":{"version":1,"progress":"turn_finished"}}),
        json!({"model":"public-model","input":[{"type":"function_call","call_id":"c","name":"lookup","arguments":"{}","_openbridge":{"version":1,"arguments":"json"}}]}),
    ] {
        let before = observed.0.lock().unwrap().len();
        let response = client
            .post(format!("{url}/v1/responses"))
            .bearer_auth(support::CLIENT_KEY)
            .json(&input)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 400);
        assert_eq!(observed.0.lock().unwrap().len(), before);
    }
    // Responses has no message-call membership carrier: static projection fails,
    // while an already-published stream aborts without a fabricated terminal.
    for stream in [false, true] {
        let before = observed.0.lock().unwrap().len();
        let mut response = client
            .post(format!("{url}/v1/responses"))
            .bearer_auth(support::CLIENT_KEY)
            .json(&json!({"model":"cross-model","input":"lookup","stream":stream}))
            .send()
            .await
            .unwrap();
        if stream {
            assert_eq!(response.status(), 200);
            let mut body = Vec::new();
            loop {
                match response.chunk().await {
                    Ok(Some(chunk)) => body.extend_from_slice(&chunk),
                    Err(_) => break,
                    Ok(None) => panic!("unrepresentable output completed its HTTP body"),
                }
            }
            let text = String::from_utf8_lossy(&body);
            assert!(!text.contains("private-model"));
            assert!(!text.contains("_openbridge"));
            assert!(!text.contains("response.completed"));
        } else {
            assert_eq!(response.status(), 502);
            let value: Value = response.json().await.unwrap();
            assert_eq!(value["error"]["code"], "upstream_error");
        }
        assert_eq!(observed.0.lock().unwrap().len(), before + 1);

        // Opposite request direction: explicit Chat history must be rejected
        // during candidate projection, before any HTTP Provider call.
        let before = observed.0.lock().unwrap().len();
        let response = client
            .post(format!("{url}/v1/chat/completions"))
            .bearer_auth(support::CLIENT_KEY)
            .json(&json!({"model":"cross-model","stream":stream,"messages":[
                {"role":"assistant","content":null,"tool_calls":[{"id":"c","type":"function","function":{"name":"lookup","arguments":"{}"}}]},
                {"role":"tool","tool_call_id":"c","content":"ok"}
            ]}))
            .send().await.unwrap();
        assert_eq!(response.status(), 400);
        assert_eq!(observed.0.lock().unwrap().len(), before);
    }
    let before = observed.0.lock().unwrap().len();
    let response = client
        .post(format!("{url}/v1/responses"))
        .json(&json!({"model":"public-model","input":"hello"}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 401);
    let response = client
        .post(format!("{url}/v1/responses"))
        .bearer_auth(support::CLIENT_KEY)
        .json(&json!({"model":"missing","input":"hello"}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 404);
    let response = client
        .post(format!("{url}/v1/responses"))
        .bearer_auth(support::CLIENT_KEY)
        .json(
            &json!({"model":"public-model","input":"hello","base_url":"https://untrusted.invalid"}),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 400);
    let response = client
        .post(format!("{url}/v1/responses"))
        .bearer_auth(support::CLIENT_KEY)
        .json(&json!({"model":"public-model","input":[
            {"type":"program","id":"p","call_id":"pending-program","code":"opaque code","fingerprint":"opaque fingerprint"}
        ]}))
        .send().await.unwrap();
    assert_eq!(response.status(), 400);
    assert_eq!(observed.0.lock().unwrap().len(), before);
    // A distinct operation traverses the same authenticated Router and acknowledged body.
    let response=client.post(format!("{url}/v1/images/generations"))
        .bearer_auth(support::CLIENT_KEY).header("x-never-forward","private-client-header")
        .json(&json!({"model":"public-image","prompt":"square","n":1,"stream":false,"output_format":"png"}))
        .send().await.unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(response.headers()["cache-control"], "no-store");
    assert_eq!(
        response.json::<Value>().await.unwrap(),
        json!({"created":123,"data":[{"b64_json":"AQID"}],"output_format":"png","size":"1024x1024"})
    );
    let response=client.post(format!("{url}/v1/images/generations")).bearer_auth(support::CLIENT_KEY)
        .json(&json!({"model":"public-image","prompt":"controlled","size":"1536x1024","quality":"high","background":"transparent","output_format":"webp","output_compression":80,"moderation":"low","user":"synthetic-user-control"})).send().await.unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(
        response.json::<Value>().await.unwrap(),
        json!({"created":123,"data":[{"b64_json":"AQID"}],"output_format":"webp","size":"1536x1024","background":"transparent","quality":"high"})
    );
    for (prompt, status) in [
        ("bad-json", 502),
        ("wrong-format", 502),
        ("truncated", 502),
        ("missing-image", 502),
        ("wrong-media", 502),
        ("rate-limit", 429),
        ("redirect", 502),
        ("over-budget", 502),
        ("decoded-over-budget", 502),
        ("timeout", 504),
    ] {
        let before = observed.0.lock().unwrap().len();
        let response = client
            .post(format!("{url}/v1/images/generations"))
            .bearer_auth(support::CLIENT_KEY)
            .json(&json!({"model":"public-image","prompt":prompt,"output_format":"png"}))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), status, "{prompt}");
        let text = response.text().await.unwrap();
        assert!(!text.contains("synthetic-private"));
        assert_eq!(observed.0.lock().unwrap().len(), before + 1, "no retries");
    }
    let before = observed.0.lock().unwrap().len();
    for (request, status) in [
        (json!({"model":"public-image","prompt":"x","n":11}), 400),
        (
            json!({"model":"public-image","prompt":"x","output_format":"jpeg","background":"transparent"}),
            400,
        ),
        (
            json!({"model":"public-image","prompt":"x","output_format":"png","output_compression":50}),
            400,
        ),
        (
            json!({"model":"router-image","prompt":"x","size":"1024x1024"}),
            400,
        ),
        (
            json!({"model":"public-image","prompt":"x","stream":true}),
            400,
        ),
        (
            json!({"model":"public-image","prompt":"x","provider":null}),
            400,
        ),
        (json!({"model":"public-model","prompt":"x"}), 404),
    ] {
        let response = client
            .post(format!("{url}/v1/images/generations"))
            .bearer_auth(support::CLIENT_KEY)
            .json(&request)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), status);
    }
    let response = client
        .post(format!("{url}/v1/images/generations"))
        .body("not JSON")
        .send()
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        401,
        "authentication precedes body interpretation"
    );
    assert_eq!(observed.0.lock().unwrap().len(), before);
    let response = client
        .post(format!("{url}/v1/images/generations"))
        .bearer_auth(support::CLIENT_KEY)
        .header("x-never-forward", "synthetic-private")
        .json(&json!({"model":"router-image","prompt":"blue square"}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(
        response.json::<Value>().await.unwrap(),
        json!({"created":456,"data":[{"b64_json":"BAUG"}],"output_format":"png"})
    );
    for (prompt, status) in [
        ("multi-ok", 200),
        ("multi-short", 502),
        ("multi-bad", 502),
        ("multi-mixed", 502),
        ("multi-tail", 502),
    ] {
        let before = observed.0.lock().unwrap().len();
        let response = client
            .post(format!("{url}/v1/images/generations"))
            .bearer_auth(support::CLIENT_KEY)
            .json(&json!({"model":"router-image","prompt":prompt,"n":2}))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), status, "{prompt}");
        let wire = response.json::<Value>().await.unwrap();
        if status == 200 {
            assert_eq!(
                wire,
                json!({"created":456,"data":[{"b64_json":"AQID"},{"b64_json":"BAUG"}],"output_format":"png"})
            );
        } else {
            assert!(wire.get("data").is_none(), "no successful prefix");
        }
        assert_eq!(
            observed.0.lock().unwrap().len(),
            before + 1,
            "one upstream request"
        );
    }
    let before = observed.0.lock().unwrap().len();
    let rejected = client
        .post(format!("{url}/v1/images/generations"))
        .bearer_auth(support::CLIENT_KEY)
        .json(&json!({"model":"router-image","prompt":"blue square","output_format":"png"}))
        .send()
        .await
        .unwrap();
    assert_eq!(rejected.status(), 400);
    assert_eq!(observed.0.lock().unwrap().len(), before);
    let response=client.post(format!("{url}/v1/images/generations")).bearer_auth(support::CLIENT_KEY)
        .json(&json!({"model":"router-image","prompt":"controlled-flare","quality":"high","background":"transparent","moderation":"low","user":"synthetic-user-control"})).send().await.unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(
        response.json::<Value>().await.unwrap(),
        json!({"created":456,"data":[{"b64_json":"BAUG"}],"output_format":"png"})
    );
    // The first valid image cannot escape while the second image is unfinished.
    for cancel in [false, true] {
        let before = observed.0.lock().unwrap().len();
        let send = client
            .post(format!("{url}/v1/images/generations"))
            .bearer_auth(support::CLIENT_KEY)
            .json(&json!({"model":"public-image","prompt":"multi-pending","n":2}))
            .send();
        let task = tokio::spawn(send);
        tokio::time::timeout(Duration::from_secs(2), observed.1.notified())
            .await
            .unwrap();
        assert!(!task.is_finished(), "no first-image publication");
        if cancel {
            task.abort();
            assert!(task.await.unwrap_err().is_cancelled());
        } else {
            let response = tokio::time::timeout(Duration::from_secs(2), task)
                .await
                .unwrap()
                .unwrap()
                .unwrap();
            assert_eq!(response.status(), 504);
            assert!(
                response
                    .json::<Value>()
                    .await
                    .unwrap()
                    .get("data")
                    .is_none()
            );
        }
        // Local cancellation is not proof of remote termination. Explicitly
        // release the synthetic peer; shared body-owner tests cover local drop.
        observed.2.notify_waiters();
        assert_eq!(observed.0.lock().unwrap().len(), before + 1);
    }
    for format in ["mp3", "wav"] {
        let response = client.post(format!("{url}/v1/audio/speech"))
            .bearer_auth(support::CLIENT_KEY).header("x-never-forward", "private-input")
            .json(&json!({"model":"public-speech","input":"hello","voice":"alloy","instructions":"","speed":1.25,"response_format":format,"stream_format":"audio"}))
            .send().await.unwrap();
        assert_eq!(response.status(), 200);
        assert_eq!(
            response.headers()["content-type"],
            if format == "wav" {
                "audio/wav"
            } else {
                "application/octet-stream"
            }
        );
        assert_eq!(response.headers()["cache-control"], "no-store");
        assert_eq!(response.bytes().await.unwrap().as_ref(), b"synthetic-audio");
    }
    let before = observed.0.lock().unwrap().len();
    // Full control matrices live in codec/admission tests; keep distinct ingress failures.
    for (payload, status) in [
        (
            json!({"model":"public-speech","input":"hello","voice":"alloy","stream_format":"sse"}),
            400,
        ),
        (
            json!({"model":"router-speech","input":"hello","voice":"router-voice","speed":1}),
            400,
        ),
        (
            json!({"model":"public-speech","input":"hello","voice":"unbound"}),
            400,
        ),
        (
            json!({"model":"public-model","input":"hello","voice":"alloy"}),
            404,
        ),
    ] {
        assert_eq!(
            client
                .post(format!("{url}/v1/audio/speech"))
                .bearer_auth(support::CLIENT_KEY)
                .json(&payload)
                .send()
                .await
                .unwrap()
                .status()
                .as_u16(),
            status
        );
    }
    assert_eq!(
        client
            .post(format!("{url}/v1/audio/speech"))
            .body("not-json")
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    assert_eq!(
        observed.0.lock().unwrap().len(),
        before,
        "rejected requests never dispatch"
    );
    for explicit in [false, true] {
        let mut payload =
            json!({"model":"router-speech","input":"router speech","voice":"router-voice"});
        if explicit {
            payload["response_format"] = json!("mp3");
            payload["stream_format"] = json!("audio");
        }
        let response = client
            .post(format!("{url}/v1/audio/speech"))
            .bearer_auth(support::CLIENT_KEY)
            .header("x-never-forward", "private-input")
            .json(&payload)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        assert_eq!(response.headers()["content-type"], "audio/mpeg");
        assert_eq!(
            response.bytes().await.unwrap().as_ref(),
            b"synthetic-router-audio"
        );
    }
    let response=client.post(format!("{url}/v1/audio/speech")).bearer_auth(support::CLIENT_KEY)
        .header("x-never-forward","private-input")
        .json(&json!({"model":"native-speech","input":"synthetic plan speech","voice":"native-voice"}))
        .send().await.unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(
        response.headers()["content-type"],
        "application/octet-stream"
    );
    assert_eq!(response.bytes().await.unwrap().as_ref(), b"native-audio");
    let mut upload=b"--fixture\r\nContent-Disposition: form-data; name=\"model\"\r\n\r\nnative-transcription\r\n--fixture\r\nContent-Disposition: form-data; name=\"language\"\r\n\r\nen\r\n--fixture\r\nContent-Disposition: form-data; name=\"file\"; filename=\"clip.wav\"\r\nContent-Type: audio/wav\r\n\r\n".to_vec();
    upload.extend_from_slice(native_audio::WAV);
    upload.extend_from_slice(b"\r\n--fixture--\r\n");
    let before = observed.0.lock().unwrap().len();
    let unauthorized = client
        .post(format!("{url}/v1/audio/transcriptions"))
        .body(upload.clone())
        .send()
        .await
        .unwrap();
    assert_eq!(unauthorized.status(), 401);
    assert_eq!(observed.0.lock().unwrap().len(), before);
    let response = client
        .post(format!("{url}/v1/audio/transcriptions"))
        .bearer_auth(support::CLIENT_KEY)
        .header("content-type", "multipart/form-data; boundary=fixture")
        .header("x-never-forward", "private-input")
        .body(upload)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(
        response.json::<Value>().await.unwrap(),
        json!({"text":"Hi.","usage":{"type":"duration","seconds":1}})
    );
    assert_eq!(observed.0.lock().unwrap().len(), before + 1);
    let models: Value = client
        .get(format!("{url}/v1/models"))
        .bearer_auth(support::CLIENT_KEY)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(models["data"].as_array().unwrap().contains(&json!({"id":"public-speech","object":"model","created":9,"owned_by":"Synthetic Speech Developer"})));
    shutdown.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(3), serving)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    drop(upstream_guard);
}

#[tokio::test]
async fn binary_bootstraps_default_home_files_and_ignores_environment_keys() {
    use morphiecore::credential::{CredentialPool, CredentialRef, Secret};
    use std::sync::atomic::{AtomicUsize, Ordering};
    // A rejecting loopback proxy makes even an accidental upstream dispatch offline.
    let trap = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy = format!("http://{}", trap.local_addr().unwrap());
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = calls.clone();
    let _trap = Guard(tokio::spawn(async move {
        axum::serve(
            trap,
            Router::new().fallback(move || {
                let observed = observed.clone();
                async move {
                    observed.fetch_add(1, Ordering::SeqCst);
                    StatusCode::BAD_GATEWAY
                }
            }),
        )
        .await
        .unwrap();
    }));
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join(".local/share/morphiecore/credentials");
    std::fs::create_dir_all(root.parent().unwrap()).unwrap();
    let manager = morphiecore::credential::CredentialManager::new(&root, vec![]).unwrap();
    manager
        .add_api_key(
            "deepseek",
            "one",
            morphiecore::credential::Secret::new("synthetic-file-key".into()).unwrap(),
        )
        .unwrap();
    manager
        .set_pool(
            "deepseek",
            "deepseek-api-key",
            0,
            morphiecore::credential::CredentialPool {
                members: vec![morphiecore::credential::CredentialRef::ApiKey {
                    alias: "one".into(),
                }],
                fallback: false,
                max_attempts: 1,
            },
        )
        .unwrap();
    let path = root.join("gateway.json");
    manager
        .add_api_key(
            "openrouter",
            "speech",
            Secret::new("synthetic-speech-file-key".into()).unwrap(),
        )
        .unwrap();
    manager
        .set_pool(
            "openrouter",
            "openrouter-api-key",
            0,
            CredentialPool {
                members: vec![CredentialRef::ApiKey {
                    alias: "speech".into(),
                }],
                fallback: false,
                max_attempts: 1,
            },
        )
        .unwrap();
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    use std::io::Write;
    options
        .open(&path)
        .unwrap()
        .write_all(
            &serde_json::to_vec(
                &json!({"client_key":support::CLIENT_KEY,"bind":"127.0.0.1:0","proxy":"unsupported://invalid","models":["deepseek-flash","qwen-audio-3.0-tts-flash"]}),
            )
            .unwrap(),
        )
        .unwrap();
    let mut process = tokio::process::Command::new(env!("CARGO_BIN_EXE_morphiecore"))
        .args(["--proxy", &proxy])
        .env_clear()
        .env("HTTPS_PROXY", "unsupported://environment")
        .env(
            if cfg!(windows) { "USERPROFILE" } else { "HOME" },
            dir.path(),
        )
        .env(
            "MORPHIECORE_CLIENT_KEY",
            "synthetic-ignored-environment-key-0001",
        )
        .env(
            "MORPHIECORE_DEEPSEEK_API_KEY",
            "synthetic-not-a-provider-key-0001",
        )
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let mut output = tokio::io::BufReader::new(process.stdout.take().unwrap());
    let ready = match child_line::read_line(&mut output, Duration::from_secs(3), 127).await {
        Ok(line) => line,
        Err(error) => {
            let _ = process.kill().await;
            let _ = process.wait().await;
            panic!("{error}");
        }
    };
    let origin = ready
        .trim()
        .strip_prefix("MorphieCore listening on ")
        .expect("bounded readiness line");
    assert!(origin.starts_with("http://127.0.0.1:"));
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(2))
        .build()
        .unwrap();
    assert_eq!(
        client
            .post(format!("{origin}/v1/responses"))
            .json(&json!({"model":"missing","input":"hello"}))
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    assert_eq!(
        client
            .post(format!("{origin}/v1/responses"))
            .bearer_auth(support::CLIENT_KEY)
            .json(&json!({"model":"missing","input":"hello"}))
            .send()
            .await
            .unwrap()
            .status(),
        404
    );
    let models: Value = client
        .get(format!("{origin}/v1/models"))
        .bearer_auth(support::CLIENT_KEY)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(models["data"].as_array().unwrap().len(), 2);
    assert!(models["data"].as_array().unwrap().contains(&json!({
        "id":"qwen-audio-3.0-tts-flash","object":"model","created":1784592000,"owned_by":"Alibaba"
    })));
    // No admitted request is issued: the synthetic key must never reach a Provider.
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    process.kill().await.unwrap();
    process.wait().await.unwrap();
}
