//! Pinned Messages SDK over loopback; no real Provider or tool execution.
use super::{ServerGuard, child_process};
use axum::{
    Router,
    body::{Body, Bytes},
    extract::State,
    http::{HeaderMap, StatusCode},
    response::Response,
    routing::post,
};
use morphiecore::{
    adapter::{ClientAdapter, UpstreamAdapter},
    execution::{Attempt, ResponseDelivery},
    protocol::{
        anthropic::{
            self,
            messages::{Dialect, Profile, TargetContract},
        },
        native::ReplayTarget,
        openai::obfuscation::Obfuscation,
        stream::SseLimits,
    },
    semantic::{context::StreamOptions, task::generation::*},
};
use serde_json::{Value, json};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::net::TcpListener;

async fn handle(
    State(seen): State<Arc<Mutex<usize>>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    // Current stable SDKs use Bearer; the API also accepts one X-Api-Key.
    let bearer = headers.get("authorization").and_then(|v| v.to_str().ok());
    let key = headers.get("x-api-key").and_then(|v| v.to_str().ok());
    if !matches!(
        (bearer, key),
        (Some("Bearer synthetic-local-token"), None) | (None, Some("synthetic-local-token"))
    ) {
        return Response::builder()
            .status(StatusCode::UNAUTHORIZED)
            .body(Body::empty())
            .unwrap();
    }
    let versions: Vec<_> = headers.get_all("anthropic-version").iter().collect();
    assert_eq!(versions.len(), 1);
    assert_eq!(versions[0], "2026-10-21");
    let wire: Value = serde_json::from_slice(&body).unwrap();
    if matches!(
        wire["model"].as_str(),
        Some("schedule" | "schedule-incomplete" | "schedule-metadata")
    ) {
        return scheduled_history(&seen, &body, &wire);
    }
    if wire["model"] == "cross-model" {
        return cross_protocol(&seen, &body, &wire);
    }
    for field in ["eager_input_streaming", "cache_control"] {
        assert_eq!(wire["tools"][0].get(field), Some(&Value::Null));
    }
    let mut request =
        anthropic::messages::client::decode_client_request(&body).expect("synthetic SDK request");
    morphiecore::execution::replay::open_from_client(&mut request).unwrap();
    assert_eq!(
        request.context.identity.user,
        morphiecore::semantic::value::Presence::Value("sdk-user".into())
    );
    assert_eq!(request.task.semantic.reasoning().budget_tokens, Some(1024));
    assert_eq!(
        request
            .task
            .semantic
            .instructions()
            .value()
            .unwrap()
            .plain_text()
            .unwrap()
            .as_str(),
        "SDK root instructions"
    );
    assert!(
        matches!(&request.task.semantic.items()[0].1, Item::Message(m) if m.role == MessageRole::User)
    );
    let ToolDefinition::Function(tool) = &request.task.semantic.tools()[0] else {
        panic!("function")
    };
    assert!(tool.dispatch.is_inactive());
    assert_eq!(tool.eager_input, None);
    assert_eq!(tool.cache, None);
    assert_eq!(
        tool.input_examples
            .as_ref()
            .unwrap()
            .iter()
            .map(|v| v.value().clone())
            .collect::<Vec<_>>(),
        vec![json!({"n":9007199254740993_u64}), json!({"n":7})]
    );
    let mut caps = TargetContract::new(Profile::AdaptiveTextTools, Dialect::Standard);
    caps.semantics = Profile::AdaptiveTextTools.representation_contract();
    caps.carriers = anthropic::messages::Carriers::all();
    let target = ReplayTarget::new("synthetic-sdk", "fixture-model").unwrap();
    let native = caps.encode_request(&request, &target).unwrap();
    assert_eq!(native.value["system"], "SDK root instructions");
    assert_eq!(native.value["metadata"], json!({"user_id":"sdk-user"}));
    assert!(native.value["tools"][0].get("allowed_callers").is_none());
    assert!(native.value["tools"][0].get("type").is_none());
    assert!(
        native.value["tools"][0]
            .get("eager_input_streaming")
            .is_none()
    );
    assert!(native.value["tools"][0].get("cache_control").is_none());
    assert_eq!(
        native.value["tools"][0]["input_examples"],
        json!([{"n":9007199254740993_u64},{"n":7}])
    );
    assert_eq!(
        native.value["thinking"],
        json!({"type":"enabled","budget_tokens":1024})
    );
    assert_eq!(
        native.evidence.len(),
        request
            .task
            .semantic
            .items()
            .iter()
            .filter(|(_, item)| matches!(
                item,
                morphiecore::semantic::task::generation::Item::ToolCall(_)
            ))
            .count()
    );
    assert!(native.evidence.iter().all(|effect| effect.rule()
        == "messages.normalize-tool-arguments.v1"
        && effect.semantic_field()
            == morphiecore::protocol::conversion::generation::Field::ToolArguments
        && effect.change() == morphiecore::protocol::conversion::generation::Change::Normalize));
    let turn = {
        let mut n = seen.lock().unwrap();
        *n += 1;
        *n
    };
    assert!(turn <= 5);
    if turn <= 2 {
        assert_eq!(
            native.value["messages"][1],
            json!({"role":"system","content":[{"type":"text","text":"Keep history."}]})
        );
        for part in native.value["messages"][0]["content"].as_array().unwrap() {
            assert!(part.get("cache_control").is_none());
        }
        assert_eq!(
            native.value["messages"][0]["content"][1],
            json!({"type":"document","source":{"type":"text","media_type":"text/plain",
                "data":"synthetic notes"},"context":"summary-only"})
        );
        assert_eq!(
            native.value["messages"][0]["content"][2],
            json!({"type":"image","source":{"type":"url","url":"https://example.invalid/a.png"},
                "transformations":{"oversized_image":"error"}})
        );
    }
    if turn == 1 || turn == 4 {
        let mut wire = json!({"id":"msg1","type":"message","role":"assistant","model":"fixture-model",
            "content":[{"type":"thinking","thinking":"Plan","signature":"synthetic-native-signature"},
                {"type":"tool_use","id":"call_sdk","name":"lookup","input":{"n":7}}],
            "stop_reason":"tool_use","stop_sequence":null,"usage":{"input_tokens":1,"output_tokens":1}});
        if turn == 4 {
            wire["content"] = json!([{"type":"text","text":"partial"}]);
            wire["stop_reason"] = json!("refusal");
            wire["stop_details"] =
                json!({"type":"refusal","category":"cyber","explanation":"synthetic explanation"});
        }
        let mut decoded = Profile::AdaptiveTextTools
            .decode_generation_response(wire.to_string().as_bytes(), LocalScope::new(2), &target)
            .unwrap();
        morphiecore::execution::replay::seal_for_client(
            &mut decoded,
            &morphiecore::protocol::replay_envelope::CLIENT_ORIGIN,
        )
        .unwrap();
        let output =
            anthropic::messages::client::encode_client_response(&decoded, "fixture-model").unwrap();
        return Response::builder()
            .header("content-type", "application/json")
            .body(Body::from(output.to_string()))
            .unwrap();
    }
    if turn == 2 {
        assert!(request.task.semantic.items().iter().any(
            |(_, item)| matches!(item,Item::ToolResult(r)
            if r.required_call_id().ok()==Some("call_sdk") && r.is_error==Some(false)
                && matches!(&r.output,ToolOutput::Parts(parts)
                    if matches!(&parts[0].content, InputPart::Text(text) if text.as_str()=="found")
                        && matches!(&parts[1].content, InputPart::Resource(r)
                            if matches!(r.description, ResourceDescription::Image {
                                size_policy: ImageSizePolicy::RejectOversized, ..
                            }))))
        ));
        assert_eq!(
            native.value["messages"][3]["content"][0]["content"],
            json!([{"type":"text","text":"found"},
                {"type":"image","source":{"type":"url","url":"https://example.invalid/a.png"},
                    "transformations":{"oversized_image":"error"}}])
        );
    }
    let mut frames = vec![
        json!({"type":"message_start","message":{"id":"msg2","type":"message","role":"assistant","model":"fixture-model",
        "content":[],"stop_reason":null,"stop_sequence":null,"usage":{"input_tokens":2,"output_tokens":0}}}),
    ];
    if turn != 3 {
        frames.extend([
            json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}),
            json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":if turn == 2 {"done"} else {"partial"}}}),
        ]);
    } else {
        frames.extend([
            json!({"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"partial_sdk","name":"lookup","input":{}}}),
            json!({"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":"{\"n\":"}}),
        ]);
    }
    let mut closing = json!({"type":"message_delta","delta":{
        "stop_reason":match turn { 2 => "end_turn", 3 => "max_tokens", _ => "refusal" },
        "stop_sequence":null},"usage":{"output_tokens":2}});
    if turn == 5 {
        closing["delta"]["stop_details"] = json!({"type":"refusal","category":"cyber",
            "explanation":"synthetic explanation"});
    }
    frames.extend([
        json!({"type":"content_block_stop","index":0}),
        closing,
        json!({"type":"message_stop"}),
    ]);
    let mut attempt = Attempt::new(
        UpstreamAdapter::AnthropicMessages {
            profile: Profile::AdaptiveTextTools,
            target,
        },
        1 << 20,
        SseLimits::default(),
    );
    attempt.begin(200, "text/event-stream").unwrap();
    let mut delivery = ResponseDelivery::new(
        ClientAdapter::Messages,
        "fixture-model",
        SseLimits::default(),
        StreamOptions::default(),
        Obfuscation::Disabled,
    );
    let mut output = vec![];
    for frame in frames {
        let bytes = format!(
            "event: {}\ndata: {frame}\n\n",
            frame["type"].as_str().unwrap()
        );
        let (_, events) = attempt.push(bytes.as_bytes()).unwrap();
        let produced = delivery
            .encode_events(&attempt, &events)
            .map(|batch| batch.into_bytes())
            .unwrap();
        if !produced.is_empty() && delivery.lifecycle().may_retry_or_fallback() {
            delivery.commit().unwrap();
        }
        output.extend(produced.into_iter().flatten());
    }
    attempt.finish().unwrap();
    output.extend(
        delivery
            .finish_stream(&attempt)
            .map(|batch| batch.into_bytes())
            .unwrap()
            .into_iter()
            .flatten(),
    );
    delivery.complete(&attempt).unwrap();
    Response::builder()
        .header("content-type", "text/event-stream")
        .body(Body::from(output))
        .unwrap()
}

#[tokio::test]
#[ignore = "requires locked Messages SDK; synthetic JSON/SSE and raw partial deltas"]
async fn sdk_messages_native_controls_history_and_partial_stream() {
    let seen = Arc::new(Mutex::new(0));
    let observed = seen.clone();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new()
                .route("/v1/messages", post(handle))
                .with_state(seen),
        )
        .await
        .unwrap()
    });
    let guard = ServerGuard(server.abort_handle());
    let home = tempfile::tempdir().unwrap();
    let mut command = super::isolated_sdk_python(home.path());
    command.args(["tests/sdk/messages_loop.py", &format!("http://{address}")]);
    let output = child_process::run(&mut command, b"", Duration::from_secs(30), 16 << 10)
        .await
        .unwrap();
    assert!(
        output.status.success(),
        "synthetic Messages SDK failure: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(*observed.lock().unwrap(), 15);
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        report,
        json!({"requests":15,"partial":true,"refusal_details":true,"unsigned_replay_rejected":true,"scheduled_history":true,"incomplete_history":true,"null_metadata_history":true})
    );
    let mut node = tokio::process::Command::new("node");
    node.env_clear()
        .env("HOME", home.path())
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args([
            "tests/sdk/pi/messages-scheduling.mjs",
            &format!("http://{address}"),
        ]);
    let output = child_process::run(&mut node, b"", Duration::from_secs(30), 16 << 10)
        .await
        .unwrap();
    assert!(
        output.status.success(),
        "Messages TypeScript callbacks: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(*observed.lock().unwrap(), 21);
    guard.0.abort();
    let _ = server.await;
}

fn scheduled_history(seen: &Mutex<usize>, body: &[u8], wire: &Value) -> Response {
    use anthropic::messages::client::{encode_client_response, sse::ClientEventEncoder};
    *seen.lock().unwrap() += 1;
    assert!(*seen.lock().unwrap() <= 21);
    let model = wire["model"].as_str().unwrap();
    let incomplete = model != "schedule";
    let target = ReplayTarget::new("synthetic-schedule", model).unwrap();
    let expected = if incomplete {
        json!([
            {"type":"text","text":"AC"},{"type":"text","text":"B"},{"type":"text","text":"D"},
        ])
    } else {
        json!([
            {"type":"thinking","thinking":"Plan","signature":"synthetic-native-signature"},
            {"type":"text","text":"AC"},{"type":"text","text":"B"},
            {"type":"tool_use","id":"first","name":"lookup","input":{"n":1}},
            {"type":"tool_use","id":"second","name":"lookup","input":{"n":2}},
        ])
    };
    if wire["messages"].as_array().unwrap().len() > 1 {
        let mut request = anthropic::messages::client::decode_client_request(body).unwrap();
        morphiecore::execution::replay::open_from_client(&mut request).unwrap();
        let mut caps = TargetContract::new(Profile::AdaptiveTextTools, Dialect::Standard);
        caps.semantics = Profile::AdaptiveTextTools.representation_contract();
        let native = caps.encode_request(&request, &target).unwrap();
        let feedback = if incomplete {
            json!([{"type":"text","text":"continue"}])
        } else {
            json!([
                {"type":"tool_result","tool_use_id":"second","content":"two"},
                {"type":"tool_result","tool_use_id":"first","content":"one"},
            ])
        };
        assert_eq!(
            native.value["messages"],
            json!([
                {"role":"user","content":[{"type":"text","text":"go"}]},
                {"role":"assistant","content":expected},
                {"role":"user","content":feedback},
            ])
        );
        return Response::builder().header("content-type", "application/json")
            .body(Body::from(json!({"id":"replayed","type":"message","role":"assistant","model":model,
                "content":[{"type":"text","text":"replayed"}],"stop_reason":"end_turn","stop_sequence":null,
                "usage":{"input_tokens":1,"output_tokens":1}}).to_string())).unwrap();
    }
    assert_eq!(wire["stream"], true);
    assert_eq!(wire["messages"], json!([{"role":"user","content":"go"}]));
    let mut decoded = Profile::AdaptiveTextTools.decode_generation_response(
        json!({"id":"scheduled","type":"message","role":"assistant","model":model,"content":expected,
            "stop_reason":if incomplete {"max_tokens"} else {"tool_use"},"stop_sequence":null,"usage":{"input_tokens":1,"output_tokens":2}})
            .to_string().as_bytes(), LocalScope::new(2), &target).unwrap();
    morphiecore::execution::replay::seal_for_client(
        &mut decoded,
        &morphiecore::protocol::replay_envelope::CLIENT_ORIGIN,
    )
    .unwrap();
    let events = if incomplete {
        crate::events_support::incomplete_text_items(true)
    } else {
        let Item::Reasoning(reasoning) = &decoded.semantic.items()[0].1 else {
            panic!("reasoning")
        };
        crate::events_support::interleaved_messages(ItemReplay {
            value: reasoning.replay.clone().unwrap(),
            origin: None,
        })
    };
    if model == "schedule-metadata" {
        return responses_null_metadata(events, model);
    }
    let mut source = StreamState::new();
    let mut encoder = ClientEventEncoder::new("scheduled", model);
    let mut output = vec![];
    for event in events {
        source = reduce(source, event.clone()).unwrap();
        output.extend(encoder.encode(&event).unwrap().into_iter().flatten());
    }
    encoder.finish().unwrap();
    assert_eq!(
        materialize(&source).unwrap().items().len(),
        if incomplete { 2 } else { 4 }
    );
    let static_wire = encode_client_response(&decoded, model).unwrap();
    assert_eq!(
        static_wire["content"].as_array().unwrap().len(),
        if incomplete { 3 } else { 5 }
    );
    Response::builder()
        .header("content-type", "text/event-stream")
        .body(Body::from(output))
        .unwrap()
}

fn responses_null_metadata(events: Vec<StreamEvent>, model: &str) -> Response {
    use morphiecore::protocol::{
        fidelity::FidelityRecords,
        openai::{Dialect, GenerationApi, GenerationCodec, events::EventEncoder},
    };
    use morphiecore::semantic::value::Presence;
    let mut metadata = crate::events_support::metadata();
    metadata.context.execution.metadata = Presence::Null;
    let mut producer = EventEncoder::new(GenerationApi::Responses, metadata)
        .unwrap()
        .with_contract(crate::events_support::contract());
    let mut attempt = Attempt::new(
        GenerationCodec::new(GenerationApi::Responses, Dialect::Standard, None),
        1 << 20,
        SseLimits::default(),
    );
    attempt.begin(200, "text/event-stream").unwrap();
    let mut delivery = ResponseDelivery::new(
        ClientAdapter::Messages,
        model,
        SseLimits::default(),
        StreamOptions::default(),
        Obfuscation::Disabled,
    );
    let mut output = vec![];
    for mut event in events {
        if let StreamEvent::Usage(usage) = &mut event {
            usage.input_relation = InputTokenRelation::IncludesCache;
            usage.cached_input_tokens = Some(0);
        }
        for value in producer
            .encode(&event, &FidelityRecords::default())
            .unwrap()
        {
            let wire = format!("data: {value}\n\n");
            for fragment in wire.as_bytes().chunks(7) {
                let mut rest = fragment;
                while !rest.is_empty() {
                    let (used, events) = attempt.push(rest).unwrap();
                    assert!(used > 0);
                    rest = &rest[used..];
                    output.extend(
                        delivery
                            .encode_events(&attempt, &events)
                            .unwrap()
                            .into_bytes()
                            .into_iter()
                            .flatten(),
                    );
                }
            }
        }
    }
    producer.finish().unwrap();
    assert!(!String::from_utf8_lossy(&output).contains("message_stop"));
    attempt.finish().unwrap();
    output.extend(
        delivery
            .finish_stream(&attempt)
            .unwrap()
            .into_bytes()
            .into_iter()
            .flatten(),
    );
    delivery.commit().unwrap();
    delivery.complete(&attempt).unwrap();
    assert!(matches!(
        attempt
            .response()
            .unwrap()
            .metadata
            .context
            .execution
            .metadata,
        Presence::Null
    ));
    Response::builder()
        .header("content-type", "text/event-stream")
        .body(Body::from(output))
        .unwrap()
}

fn cross_protocol(seen: &Mutex<usize>, body: &[u8], wire: &Value) -> Response {
    use morphiecore::{
        execution::attempt::prepare,
        protocol::openai::{Dialect, GenerationApi, GenerationCodec},
        provider::SecretMaterial,
        topology::{GenerationProtocol, catalog::API_KEY_BINDINGS},
    };
    let turn = {
        let mut n = seen.lock().unwrap();
        *n += 1;
        *n - 6
    };
    assert!(turn < 4);
    if turn % 2 == 1 {
        assert!(anthropic::messages::client::decode_client_request(body).is_err());
        return Response::builder()
            .status(400)
            .header("content-type", "application/json")
            .body(Body::from(
                json!({"type":"error","error":{
                    "type":"invalid_request_error","message":"Invalid synthetic history"
                }})
                .to_string(),
            ))
            .unwrap();
    }
    let request = anthropic::messages::client::decode_client_request(body).unwrap();
    let binding = API_KEY_BINDINGS
        .iter()
        .find(|b| (b.provider)().id.as_str() == "aliyun-tokenplan-cn" && b.model == "qwen3.8-flash")
        .unwrap();
    let upstream = prepare(
        &binding.endpoint(GenerationProtocol::OpenAiChat).unwrap(),
        &(binding.provider)(),
        &SecretMaterial::new("synthetic-cross-protocol-key").unwrap(),
        &request,
    )
    .unwrap();
    let prepared: Value = serde_json::from_slice(&upstream.body).unwrap();
    assert_eq!(
        prepared["messages"],
        json!([{"role":"user","content":"go"}])
    );
    let chunk = |delta, finish| {
        json!({"id":"cross","object":"chat.completion.chunk",
        "created":1,"model":"synthetic","choices":[{"index":0,"delta":delta,"finish_reason":finish}]})
    };
    let frames = [
        chunk(
            json!({"role":"assistant","reasoning_content":"Plan 🧪 "}),
            Value::Null,
        ),
        chunk(json!({"reasoning_content":"Check"}), Value::Null),
        chunk(
            json!({"tool_calls":[{"index":0,"id":"call_cross","type":"function",
            "function":{"name":"lookup","arguments":"{\"n\":7}"}}]}),
            Value::Null,
        ),
        chunk(json!({}), json!("tool_calls")),
        json!({"id":"cross","object":"chat.completion.chunk","created":1,"model":"synthetic",
            "choices":[],"usage":{"prompt_tokens":2,"completion_tokens":3,"total_tokens":5,
                "prompt_tokens_details":{"cached_tokens":0}}}),
    ];
    let mut attempt = Attempt::new(
        GenerationCodec::new(GenerationApi::Chat, Dialect::ChatReasoning, None),
        1 << 20,
        SseLimits::default(),
    );
    attempt.begin(200, "text/event-stream").unwrap();
    let mut delivery = ResponseDelivery::new(
        ClientAdapter::Messages,
        "cross-model",
        SseLimits::default(),
        StreamOptions::default(),
        Obfuscation::Disabled,
    );
    let input = frames
        .iter()
        .map(|v| format!("data: {v}\n\n"))
        .collect::<String>()
        + "data: [DONE]\n\n";
    let mut rejected = false;
    let streaming = wire["stream"] == true;
    for fragment in input.as_bytes().chunks(7) {
        let mut rest = fragment;
        while !rest.is_empty() {
            let (used, events) = attempt.push(rest).unwrap();
            if streaming && !rejected {
                match delivery.encode_events(&attempt, &events) {
                    Ok(batch) => assert!(batch.into_bytes().iter().all(|frame| {
                        !std::str::from_utf8(frame).unwrap().contains("message_stop")
                    })),
                    Err(_) => rejected = true,
                }
            }
            rest = &rest[used..];
        }
    }
    attempt.finish().unwrap();
    // The upstream is valid. Its unsigned reasoning, not source damage, prevents
    // a standard Messages completion in both JSON and streamed delivery.
    if streaming {
        assert!(rejected);
        assert!(delivery.finish_stream(&attempt).is_err());
    } else {
        assert!(delivery.encode_json(&attempt).is_err());
    }
    assert!(delivery.complete(&attempt).is_err());
    Response::builder()
        .status(500)
        .header("content-type", "application/json")
        .body(Body::from(
            json!({"type":"error","error":{
                "type":"api_error","message":"Unrepresentable synthetic response"
            }})
            .to_string(),
        ))
        .unwrap()
}
