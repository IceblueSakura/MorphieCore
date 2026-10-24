//! Public upstream bytes -> real Attempt/ResponseDelivery -> pinned SDK -> returned history.
//! This codec/delivery gate is not Router, Provider or installed-agent acceptance.
use crate::{
    child_process,
    upstream_assets::{asset, messages_with_delimiter},
};
use axum::{
    Router,
    body::{Body, Bytes},
    extract::State,
    http::HeaderMap,
    response::Response,
    routing::post,
};
use morphiecore::{
    adapter::{ClientAdapter, UpstreamAdapter},
    execution::{Attempt, ResponseDelivery},
    protocol::{
        anthropic::messages::Profile as MessagesProfile,
        native::ReplayTarget,
        openai::{Adapter, Dialect, Profile, TargetCaps, obfuscation::Obfuscation},
        stream::SseLimits,
    },
    semantic::{context::StreamOptions, task::generation::Item, value::Presence},
};
use serde_json::Value;
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

async fn handle(
    State(counts): State<Arc<Mutex<[usize; 2]>>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    assert_eq!(
        headers.get("authorization").unwrap(),
        "Bearer synthetic-local-token"
    );
    let value: Value = serde_json::from_slice(&body).unwrap();
    let model = value["model"].as_str().unwrap();
    let messages = model == "messages";
    assert!(messages || model == "chat");
    if messages {
        assert_eq!(headers["anthropic-version"], "2026-10-21");
    }
    let index = usize::from(messages);
    let turn = {
        let mut counts = counts.lock().unwrap();
        counts[index] += 1;
        assert!(counts[index] <= 4, "unexpected SDK retry");
        counts[index]
    };
    let feedback = turn % 2 == 0;
    let client = if messages {
        ClientAdapter::Messages
    } else {
        ClientAdapter::openai(
            Adapter::new(Profile::Chat, Dialect::Standard, None),
            TargetCaps::full(),
        )
    };
    let request = client.decode_client_request(&body).unwrap();
    if feedback {
        let id = if messages {
            "toolu_01NRLabsLyVHZPKxbKvkfSMn"
        } else {
            "call_c91SqDXlYFuETYv8mUHzz6pp"
        };
        let calls = request
            .task
            .semantic
            .items()
            .iter()
            .filter(|(_, item)| matches!(item, Item::ToolCall(call) if call.call_id.as_str() == id))
            .count();
        let results = request.task.semantic.items().iter().filter(|(_, item)| matches!(item, Item::ToolResult(result) if result.required_call_id().ok() == Some(id))).count();
        assert_eq!(
            (calls, results),
            (1, 1),
            "actual SDK history must re-enter the codec"
        );
    }
    let (upstream, bytes) = if messages {
        let (source, native_model) = if feedback {
            ("anthropic/basic.sse", "claude-3-opus-latest")
        } else {
            ("anthropic/tool-use.sse", "claude-sonnet-4-20250514")
        };
        (
            UpstreamAdapter::AnthropicMessages {
                profile: MessagesProfile::AdaptiveTextTools,
                target: ReplayTarget::new("public-fixture", native_model).unwrap(),
            },
            messages_with_delimiter(source),
        )
    } else {
        (
            Adapter::new(Profile::Chat, Dialect::Standard, None).into(),
            asset(if feedback {
                "openai/chat-length.sse"
            } else {
                "openai/chat-tool.sse"
            }),
        )
    };
    let mut attempt = Attempt::new(upstream, 2 << 20, SseLimits::default());
    attempt.begin(200, "text/event-stream").unwrap();
    let mut delivery = ResponseDelivery::new(
        client,
        model,
        SseLimits::default(),
        StreamOptions {
            include_usage: Presence::Value(true),
            include_obfuscation: Presence::Value(false),
        },
        Obfuscation::Disabled,
    );
    let streamed = value["stream"] == true;
    let mut output = vec![];
    for chunk in bytes.chunks(17) {
        let mut rest = chunk;
        while !rest.is_empty() {
            let (used, events) = attempt.push(rest).unwrap();
            assert!(used > 0);
            rest = &rest[used..];
            if streamed {
                let frames = delivery
                    .encode_events(&attempt, &events)
                    .map(|batch| batch.into_bytes())
                    .unwrap();
                if !frames.is_empty() && delivery.lifecycle().may_retry_or_fallback() {
                    delivery.commit().unwrap();
                }
                output.extend(frames.into_iter().flatten());
            }
        }
    }
    attempt.finish().unwrap();
    if streamed {
        output.extend(
            delivery
                .finish_stream(&attempt)
                .map(|batch| batch.into_bytes())
                .unwrap()
                .into_iter()
                .flatten(),
        );
    } else {
        output = delivery.encode_json(&attempt).unwrap().to_vec();
    }
    Response::builder()
        .header(
            "content-type",
            if streamed {
                "text/event-stream"
            } else {
                "application/json"
            },
        )
        .body(Body::from(output))
        .unwrap()
}

#[tokio::test]
#[ignore = "requires locked Python SDKs; eight synthetic requests using vendored upstream fixtures"]
async fn upstream_fixtures_reach_real_sdks_and_returned_history() {
    let counts = Arc::new(Mutex::new([0, 0]));
    let observed = counts.clone();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new()
                .route("/v1/chat/completions", post(handle))
                .route("/v1/messages", post(handle))
                .with_state(counts),
        )
        .await
        .unwrap();
    });
    let guard = super::ServerGuard(server.abort_handle());
    let home = tempfile::tempdir().unwrap();
    let mut command = super::isolated_sdk_python(home.path());
    command.args([
        "tests/sdk/upstream_assets.py",
        &format!("http://{address}/v1"),
    ]);
    let output = child_process::run(&mut command, b"", Duration::from_secs(45), 16 << 10)
        .await
        .unwrap();
    guard.0.abort();
    let _ = server.await;
    assert!(
        output.status.success(),
        "SDK asset consumer failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(*observed.lock().unwrap(), [4, 4]);
    assert_eq!(
        output.stdout,
        b"upstream assets: JSON, SSE, terminal facts and returned history verified\n"
    );
}
