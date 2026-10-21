//! Router HTTP smoke with independent Generation and media expectations.
use super::{Guard, chat_wire, native_audio, responses_wire, support};
use axum::{
    Router,
    body::Body,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::Response,
    routing::post,
};
use morphiecore::{
    gateway::Limits,
    protocol::openai::{
        Adapter, DecodedResponse, Dialect, Profile, TargetCaps, responses::sse::ResponsesSseDecoder,
    },
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

#[derive(Clone, Default)]
struct Upstream(Arc<Mutex<Vec<Value>>>);
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
    assert!(!headers.contains_key("x-morphiecore-conversation-id"));
    assert_eq!(request["model"], "private-model");
    let chat = request.get("messages").is_some();
    let cap = if chat {
        "max_completion_tokens"
    } else {
        "max_output_tokens"
    };
    assert_eq!(
        request[cap], 32,
        "the client cap must pass through IR into upstream wire"
    );
    let turn = if request.to_string().contains("tool_call_id")
        || request.to_string().contains("function_call_output")
    {
        2
    } else {
        1
    };
    if !chat && request.to_string().contains("function_call_output") {
        let history = request["input"].as_array().unwrap();
        for expected in [
            json!({"type":"function_call_output","call_id":"c_lookup","output":[
                {"type":"input_text","text":"{\"n\":1}"},
                {"type":"input_image","image_url":"data:image/png;base64,AQID","detail":"auto"}]}),
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
    if request.to_string().contains("synthetic-overflow") {
        return Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .header("content-type", "application/json")
            .body(Body::from(
                json!({"error":{
                    "code":"context_length_exceeded","message":"maximum context length"
                }})
                .to_string(),
            ))
            .unwrap();
    }
    let stream = request["stream"] == true;
    let mut output = if chat {
        chat_wire::response(turn)
    } else {
        responses_wire::response(turn)
    };
    output["model"] = json!("private-model");
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
                    &morphiecore::protocol::openai::responses::sse::encode_frame(frame, 1 << 20)
                        .unwrap(),
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
    let body = if stream {
        let chunks = bytes
            .chunks(7)
            .map(|c| Ok::<_, std::convert::Infallible>(bytes::Bytes::copy_from_slice(c)))
            .collect::<Vec<_>>();
        Body::from_stream(futures_util::stream::iter(chunks))
    } else {
        Body::from(bytes)
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
        .body(body)
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
    assert_eq!(
        request,
        json!({"model":"openai/gpt-image-2.5-flare","prompt":"blue square","n":1,"stream":false,"provider":{"only":["openai"],"allow_fallbacks":false}})
    );
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
    Response::builder()
        .header("content-type", "application/json")
        .body(Body::from(
            json!({"created":123,"data":[{"b64_json":"AQID"}],"output_format":"png","size":"1024x1024"})
                .to_string(),
        ))
        .unwrap()
}

#[tokio::test]
async fn real_router_uses_provider_http_for_json_sse_tools_and_cross_profile() {
    async fn embedding_answer(
        State(state): State<Upstream>,
        headers: HeaderMap,
        axum::Json(request): axum::Json<Value>,
    ) -> Response {
        assert_eq!(
            headers["authorization"],
            "Bearer synthetic-upstream-credential-0001"
        );
        assert!(!headers.contains_key("x-never-forward"));
        assert_eq!(request["model"], "private-vector");
        assert_eq!(request["encoding_format"], "float");
        assert!(request.get("max_output_tokens").is_none());
        assert!(request.get("max_completion_tokens").is_none());
        state.0.lock().unwrap().push(request.clone());
        let inputs = request["input"].as_array().unwrap();
        let data: Vec<_> = (0..inputs.len())
            .rev()
            .map(|index| {
                json!({
                    "object":"embedding","index":index,
                    "embedding":[0.5,-0.125]
                })
            })
            .collect();
        let value = json!({"object":"list","model":"reported-vector","id":"synthetic-request-id","data":data,
            "usage":{"total_tokens":7}});
        Response::builder()
            .header("content-type", "application/json")
            .body(Body::from(value.to_string()))
            .unwrap()
    }
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
        .route("/embeddings", post(embedding_answer))
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
    let gateway = support::gateway_with_embeddings(&origin, Limits::default());
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
                json!({"model":"public-model","messages":[{"role":"user","content":"lookup"}],"max_completion_tokens":32,"stream":stream,"stream_options":if stream {json!({"include_usage":true,"include_obfuscation":false})}else{Value::Null}})
            } else {
                json!({"model":"public-model","input":"lookup","store":false,"max_output_tokens":32,"stream":stream})
            };
            let response = client
                .post(format!("{url}{path}"))
                .bearer_auth(support::CLIENT_KEY)
                .header("x-never-forward", "private-input")
                .header("X-MorphieCore-Conversation-Id", "conversation-one")
                .json(&request)
                .send()
                .await
                .unwrap();
            assert_eq!(response.status(), 200);
            assert_eq!(response.headers()["x-morphiecore-adaptation"], "possible");
            assert_eq!(
                response.headers()["link"],
                "</v1/morphiecore/capabilities>; rel=\"describedby\""
            );
            assert!(
                !response
                    .headers()
                    .contains_key("x-morphiecore-conversation-id")
            );
            assert_eq!(
                observed.0.lock().unwrap().last().unwrap()["prompt_cache_key"],
                "2b9fca8fab555e6f158d416aad2a034e943010b3721ca44a5fb1c3e77d9668af"
            );
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
                let contract = TargetCaps {
                    replay_origin: Some(ReplayOrigin::new("fixture").unwrap()),
                    ..TargetCaps::full()
                };
                let projected = Adapter::new(Profile::Responses, Dialect::Standard, None)
                    .encode_response(&decoded, &contract)
                    .unwrap();
                let token = projected["output"][0]["encrypted_content"]
                    .as_str()
                    .unwrap();
                assert!(token.starts_with("mc1."));
                assert!(!String::from_utf8_lossy(&body).contains("synthetic-final-token"));
                let (_, format, payload) =
                    morphiecore::protocol::replay_envelope::open(token).unwrap();
                assert_eq!(
                    format,
                    morphiecore::semantic::task::generation::ReplayFormat::ResponsesEncrypted
                );
                assert_eq!(payload.as_str(), "synthetic-final-token");
                let mut history = vec![json!({"role":"user","content":"lookup"})];
                history.extend(projected["output"].as_array().unwrap().iter().cloned());
                for call in calls {
                    // Synthetic results only; call arguments never execute code.
                    history.push(match call.call_id {
                        "c_sql" => json!({"type":"custom_tool_call_output","call_id":call.call_id,"output":"1"}),
                        "c_lookup" => json!({"type":"function_call_output","call_id":call.call_id,"output":[
                            {"type":"input_text","text":"{\"n\":1}"},
                            {"type":"input_image","image_url":"data:image/png;base64,AQID","detail":"auto"}]}),
                        _ => panic!("unexpected call identity"),
                    });
                }
                let followup = client
                    .post(format!("{url}{path}"))
                    .bearer_auth(support::CLIENT_KEY)
                    .header("X-MorphieCore-Conversation-Id", "conversation-one")
                    .json(&json!({"model":"public-model","input":history,"store":false,"max_output_tokens":32,"stream":stream}))
                    .send()
                    .await
                    .unwrap();
                assert_eq!(followup.status(), 200);
                assert_eq!(
                    observed.0.lock().unwrap().last().unwrap()["prompt_cache_key"],
                    "2b9fca8fab555e6f158d416aad2a034e943010b3721ca44a5fb1c3e77d9668af"
                );
                let finished = responses_delivery(&followup.bytes().await.unwrap(), stream);
                assert_eq!(finished.semantic.outcome(), Outcome::Completed);
                assert_eq!(finished.semantic.continuation(), Continuation::Unreported);
                assert_eq!(finished.semantic.usage().unwrap().total_tokens, Some(8));

                let mut later = history.clone();
                let completed = Adapter::new(Profile::Responses, Dialect::Standard, None)
                    .encode_response(&finished, &contract)
                    .unwrap();
                later.extend(completed["output"].as_array().unwrap().iter().cloned());
                later.push(json!({"role":"user","content":"Continue."}));
                let replayed = client
                    .post(format!("{url}{path}"))
                    .bearer_auth(support::CLIENT_KEY)
                    .json(&json!({"model":"public-model","input":later,
                        "store":false,"max_output_tokens":32,"stream":stream}))
                    .send()
                    .await
                    .unwrap();
                assert_eq!(replayed.status(), 200);
                assert_eq!(
                    responses_delivery(&replayed.bytes().await.unwrap(), stream)
                        .semantic
                        .outcome(),
                    Outcome::Completed
                );

                let before = observed.0.lock().unwrap().len();
                history.last_mut().unwrap()["call_id"] = json!("unmatched-call");
                let rejected = client
                    .post(format!("{url}{path}"))
                    .bearer_auth(support::CLIENT_KEY)
                    .json(&json!({"model":"public-model","input":history,"max_output_tokens":32,"stream":stream}))
                    .send()
                    .await
                    .unwrap();
                assert_eq!(rejected.status(), 400);
                assert_eq!(observed.0.lock().unwrap().len(), before);
            }
        }
    }
    // A Chat tool response has an exact Responses carrier.
    for stream in [false, true] {
        let before = observed.0.lock().unwrap().len();
        let response = client
            .post(format!("{url}/v1/responses"))
            .bearer_auth(support::CLIENT_KEY)
            .json(&json!({"model":"cross-model","input":"lookup","store":false,"max_output_tokens":32,"stream":stream}))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let body = response.bytes().await.unwrap();
        let text = String::from_utf8_lossy(&body);
        assert!(!text.contains("private-model"));
        assert!(!text.contains("_openbridge"));
        let decoded = responses_delivery(&body, stream);
        assert_eq!(decoded.semantic.outcome(), Outcome::Completed);
        let encoded = Adapter::new(Profile::Responses, Dialect::Standard, None)
            .encode_response(&decoded, &TargetCaps::full())
            .unwrap();
        assert_eq!(encoded["output"][1]["call_id"], "call-local");
        assert_eq!(encoded["output"][1]["arguments"], "{\"n\":1}");
        assert_eq!(observed.0.lock().unwrap().len(), before + 1);
        // Wrong result identity still fails before any upstream dispatch.
        let before = observed.0.lock().unwrap().len();
        let response = client
            .post(format!("{url}/v1/chat/completions"))
            .bearer_auth(support::CLIENT_KEY)
            .json(&json!({"model":"cross-model","stream":stream,"messages":[
                {"role":"assistant","content":null,"tool_calls":[{"id":"c","type":"function","function":{"name":"lookup","arguments":"{}"}}]},
                {"role":"tool","tool_call_id":"wrong","content":"ok"}
            ]}))
            .send().await.unwrap();
        assert_eq!(response.status(), 400);
        assert!(!response.headers().contains_key("x-morphiecore-adaptation"));
        assert_eq!(observed.0.lock().unwrap().len(), before);
    }
    // A pre-frame Responses failure is still a stream, not a successful exchange.
    let response = client
        .post(format!("{url}/v1/responses"))
        .bearer_auth(support::CLIENT_KEY)
        .json(&json!({"model":"public-model","input":"synthetic-overflow",
            "store":false,"max_output_tokens":32,"stream":true}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(response.headers()["x-morphiecore-adaptation"], "possible");
    assert_eq!(
        response.headers()["link"],
        "</v1/morphiecore/capabilities>; rel=\"describedby\""
    );
    let failed = response.text().await.unwrap();
    assert!(failed.contains("response.failed") && failed.contains("context_length_exceeded"));
    assert!(!failed.contains("response.completed"));
    let before = observed.0.lock().unwrap().len();
    let response = client
        .post(format!("{url}/v1/responses"))
        .json(&json!({"model":"public-model","input":"hello"}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 401);
    assert!(!response.headers().contains_key("x-morphiecore-adaptation"));
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
            &json!({"model":"public-model","input":"hello","max_output_tokens":32,"base_url":"https://untrusted.invalid"}),
        )
        .send()
        .await
        .unwrap();
    // Unregistered fields are ignored, never forwarded and never select an origin.
    assert_eq!(response.status(), 200);
    assert_eq!(observed.0.lock().unwrap().len(), before + 1);
    assert!(
        !observed
            .0
            .lock()
            .unwrap()
            .last()
            .unwrap()
            .to_string()
            .contains("untrusted.invalid")
    );
    let before = before + 1;
    let response = client
        .post(format!("{url}/v1/responses"))
        .bearer_auth(support::CLIENT_KEY)
        .json(&json!({"model":"public-model","input":[
            {"type":"program","id":"p","call_id":"pending-program","code":"opaque code","fingerprint":"opaque fingerprint"}
        ]}))
        .send().await.unwrap();
    assert_eq!(response.status(), 400);
    assert_eq!(observed.0.lock().unwrap().len(), before);
    let response = client.post(format!("{url}/v1/embeddings"))
        .bearer_auth(support::CLIENT_KEY).header("x-never-forward","synthetic-header")
        .json(&json!({"model":"public-vector","input":["first","second"],"dimensions":2,"encoding_format":"float"}))
        .send().await.unwrap();
    assert_eq!(response.status(), 200);
    let vector: Value = response.json().await.unwrap();
    assert_eq!(
        vector,
        json!({"object":"list","model":"reported-vector","data":[
        {"object":"embedding","index":1,"embedding":[0.5,-0.125]},
        {"object":"embedding","index":0,"embedding":[0.5,-0.125]}],
        "usage":{"prompt_tokens":7,"total_tokens":7}})
    );
    let before = observed.0.lock().unwrap().len();
    let response = client
        .post(format!("{url}/v1/embeddings"))
        .json(&json!({"model":"public-vector","input":"x"}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 401);
    assert_eq!(observed.0.lock().unwrap().len(), before);
    // A distinct operation traverses the same authenticated Router and acknowledged body.
    let response=client.post(format!("{url}/v1/images/generations"))
        .bearer_auth(support::CLIENT_KEY).header("x-never-forward","private-client-header")
        .json(&json!({"model":"public-image","prompt":"square","n":1,"stream":false,"output_format":"png"}))
        .send().await.unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(response.headers()["cache-control"], "no-store");
    assert!(!response.headers().contains_key("x-morphiecore-adaptation"));
    assert_eq!(
        response.json::<Value>().await.unwrap(),
        json!({"created":123,"data":[{"b64_json":"AQID"}],"output_format":"png","size":"1024x1024"})
    );
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
    assert!(models["data"].as_array().unwrap().contains(&json!({"id":"public-vector","object":"model","created":14,"owned_by":"Synthetic Vector Developer"})));
    shutdown.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(3), serving)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    drop(upstream_guard);
}
