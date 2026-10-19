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
        anthropic::{self, Dialect, Profile, TargetCaps},
        native::ReplayTarget,
        openai::stream::Obfuscation,
        stream::SseLimits,
    },
    semantic::{context::StreamOptions, task::generation::*},
};
use serde_json::{Value, json};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{net::TcpListener, process::Command};

async fn handle(
    State(seen): State<Arc<Mutex<usize>>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if headers.get("x-api-key").and_then(|v| v.to_str().ok()) != Some("synthetic-local-token") {
        return Response::builder()
            .status(StatusCode::UNAUTHORIZED)
            .body(Body::empty())
            .unwrap();
    }
    let request = anthropic::client::decode_client_request(&body).expect("synthetic SDK request");
    assert_eq!(
        request.context.identity.user,
        morphiecore::semantic::value::Presence::Value("sdk-user".into())
    );
    assert_eq!(request.task.semantic.reasoning().budget_tokens, Some(1024));
    let mut caps = TargetCaps::new(Profile::AdaptiveTextTools, Dialect::Standard);
    caps.semantics = Profile::AdaptiveTextTools.representation_contract();
    let target = ReplayTarget::new("synthetic-sdk", "fixture-model").unwrap();
    let native = caps.encode_request(&request, &target).unwrap();
    assert_eq!(native.value["metadata"], json!({"user_id":"sdk-user"}));
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
    assert!(turn <= 3);
    if turn == 1 {
        let wire = json!({"id":"msg1","type":"message","role":"assistant","model":"fixture-model",
            "content":[{"type":"tool_use","id":"call_sdk","name":"lookup","input":{"n":7}}],
            "stop_reason":"tool_use","stop_sequence":null,"usage":{"input_tokens":1,"output_tokens":1}});
        let decoded = Profile::AdaptiveTextTools
            .decode_generation_response(wire.to_string().as_bytes(), LocalScope::new(2), &target)
            .unwrap();
        let output = anthropic::client::encode_client_response(&decoded, "fixture-model").unwrap();
        return Response::builder()
            .header("content-type", "application/json")
            .body(Body::from(output.to_string()))
            .unwrap();
    }
    if turn == 2 {
        assert!(request.task.semantic.items().iter().any(
            |(_, item)| matches!(item,Item::ToolResult(r)
            if r.required_call_id().ok()==Some("call_sdk") && r.is_error==Some(false)
                && matches!(&r.output,ToolOutput::Text(text) if text=="found"))
        ));
    }
    let mut frames = vec![
        json!({"type":"message_start","message":{"id":"msg2","type":"message","role":"assistant","model":"fixture-model",
        "content":[],"stop_reason":null,"stop_sequence":null,"usage":{"input_tokens":2,"output_tokens":0}}}),
    ];
    if turn == 2 {
        frames.extend([
            json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}),
            json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"done"}}),
        ]);
    } else {
        frames.extend([
            json!({"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"partial_sdk","name":"lookup","input":{}}}),
            json!({"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":"{\"n\":"}}),
        ]);
    }
    frames.extend([
        json!({"type":"content_block_stop","index":0}),
        json!({"type":"message_delta","delta":{"stop_reason":if turn==2{"end_turn"}else{"max_tokens"},"stop_sequence":null},"usage":{"output_tokens":2}}),
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
    let mut command = Command::new(
        std::env::var_os("MORPHIECORE_SDK_PYTHON").unwrap_or_else(|| "python3".into()),
    );
    command
        .env_clear()
        .env("HOME", home.path())
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .args(["tests/sdk/messages_loop.py", &format!("http://{address}")])
        .current_dir(env!("CARGO_MANIFEST_DIR"));
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
    assert_eq!(*observed.lock().unwrap(), 3);
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report, json!({"requests":3,"partial":true}));
}
