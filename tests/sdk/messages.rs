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
            messages::{Dialect, Profile, TargetCaps},
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
    for field in ["eager_input_streaming", "cache_control"] {
        assert_eq!(wire["tools"][0].get(field), Some(&Value::Null));
    }
    let request =
        anthropic::messages::client::decode_client_request(&body).expect("synthetic SDK request");
    assert_eq!(
        request.context.identity.user,
        morphiecore::semantic::value::Presence::Value("sdk-user".into())
    );
    assert_eq!(request.task.semantic.reasoning().budget_tokens, Some(1024));
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
    let mut caps = TargetCaps::new(Profile::AdaptiveTextTools, Dialect::Standard);
    caps.semantics = Profile::AdaptiveTextTools.representation_contract();
    caps.carriers = anthropic::messages::Carriers::all();
    let target = ReplayTarget::new("synthetic-sdk", "fixture-model").unwrap();
    let native = caps.encode_request(&request, &target).unwrap();
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
    assert!(native.rules.is_empty());
    let turn = {
        let mut n = seen.lock().unwrap();
        *n += 1;
        *n
    };
    assert!(turn <= 5);
    if turn <= 2 {
        assert_eq!(
            native.value["messages"][1],
            json!({"role":"system","content":[],"output_config":{"effort":"low"}})
        );
        assert_eq!(
            native.value["messages"][2],
            json!({"role":"user","content":[{"type":"text","text":"second input"}]})
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
            "content":[{"type":"tool_use","id":"call_sdk","name":"lookup","input":{"n":7}}],
            "stop_reason":"tool_use","stop_sequence":null,"usage":{"input_tokens":1,"output_tokens":1}});
        if turn == 4 {
            wire["content"] = json!([{"type":"text","text":"partial"}]);
            wire["stop_reason"] = json!("refusal");
            wire["stop_details"] =
                json!({"type":"refusal","category":"cyber","explanation":"synthetic explanation"});
        }
        let decoded = Profile::AdaptiveTextTools
            .decode_generation_response(wire.to_string().as_bytes(), LocalScope::new(2), &target)
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
            native.value["messages"][4]["content"][0]["content"],
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
        let produced = delivery.encode_events(&attempt, &events).unwrap();
        if !produced.is_empty() && delivery.lifecycle().may_retry_or_fallback() {
            delivery.commit().unwrap();
        }
        output.extend(produced.into_iter().flatten());
    }
    attempt.finish().unwrap();
    output.extend(
        delivery
            .finish_stream(&attempt)
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
    guard.0.abort();
    let _ = server.await;
    assert!(
        output.status.success(),
        "synthetic Messages SDK failure: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(*observed.lock().unwrap(), 5);
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        report,
        json!({"requests":5,"partial":true,"refusal_details":true})
    );
}
