//! Standard Chat media over an isolated fixed SDK loopback; no product catalog or keys.
use axum::{
    Router,
    body::{Body, Bytes},
    extract::State,
    http::{HeaderMap, StatusCode},
    response::Response,
    routing::post,
};
use morphiecore::{
    adapter::{Adapter, Dialect},
    lowering::generation::GenerationRepresentationContract as Contract,
    protocol::openai::{
        Profile,
        chat_sse::ChatSseEncoder,
        sse::{Obfuscation, SseLimits},
    },
    semantic::value::{Presence, ReplayOrigin},
};
use serde_json::{Value, json};
use std::{
    process::Stdio,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{net::TcpListener, process::Command};
fn adapter() -> Adapter {
    Adapter::new(Profile::Chat, Dialect::Standard, Some(origin()))
}
fn origin() -> ReplayOrigin {
    ReplayOrigin::new("synthetic-audio-origin").unwrap()
}
fn contract() -> Contract {
    let mut c = Contract::full();
    c.replay_origin = Some(origin());
    c.resource_time = Some(1000);
    c
}
fn chunk(delta: Value, finish: Value) -> Value {
    json!({"id":"synthetic-media","object":"chat.completion.chunk","created":1,"model":"fixture-model","choices":[{"index":0,"delta":delta,"finish_reason":finish}]})
}
#[derive(Clone, Default)]
struct Context(Arc<Mutex<(usize, bool)>>);
async fn handle(State(context): State<Context>, headers: HeaderMap, body: Bytes) -> Response {
    let result = (|| -> Result<(bool, Vec<u8>), ()> {
        if body.len() > 65536
            || headers.get("authorization").and_then(|v| v.to_str().ok())
                != Some("Bearer synthetic-local-token")
        {
            return Err(());
        }
        let request = adapter().decode_request(&body).map_err(|_| ())?;
        if request.model != "fixture-model" {
            return Err(());
        }
        let sent = adapter()
            .encode_request(&request, "fixture-model", &contract())
            .map_err(|_| ())?;
        let history = sent["messages"].as_array().ok_or(())?;
        let follow = history.len() > 1;
        if follow && history[1].get("audio") != Some(&json!({"id":"synthetic-audio"})) {
            return Err(());
        }
        let audio = request.task.semantic.settings().audio.value().is_some();
        let stream = request.delivery.streaming();
        if follow || !audio {
            let message = if follow {
                json!({"role":"assistant","content":"continued"})
            } else {
                json!({"role":"assistant","content":"hello","annotations":[{"type":"url_citation","url_citation":{"start_index":0,"end_index":5,"title":"Example","url":"https://example.invalid/"}}]})
            };
            let wire = json!({"id":"synthetic-media","object":"chat.completion","created":1,"model":"fixture-model","choices":[{"index":0,"message":message,"finish_reason":"stop"}]});
            let decoded = adapter()
                .decode_response(&serde_json::to_vec(&wire).map_err(|_| ())?)
                .map_err(|_| ())?;
            return Ok((
                false,
                serde_json::to_vec(
                    &adapter()
                        .encode_response(&decoded, &contract())
                        .map_err(|_| ())?,
                )
                .map_err(|_| ())?,
            ));
        }
        let wire = json!({"id":"synthetic-media","object":"chat.completion","created":1,"model":"fixture-model","choices":[{"index":0,"message":{"role":"assistant","content":null,"audio":{"id":"synthetic-audio","data":"AQIDBA==","transcript":"hello","expires_at":2000}},"finish_reason":"stop"}]});
        if !stream {
            let decoded = adapter()
                .decode_response(&serde_json::to_vec(&wire).map_err(|_| ())?)
                .map_err(|_| ())?;
            return Ok((
                false,
                serde_json::to_vec(
                    &adapter()
                        .encode_response(&decoded, &contract())
                        .map_err(|_| ())?,
                )
                .map_err(|_| ())?,
            ));
        }
        let mut decoder = adapter().event_decoder();
        let mut frames = vec![];
        let mut encoder = None;
        for payload in [
            chunk(
                json!({"role":"assistant","audio":{"id":"synthetic-audio"}}),
                Value::Null,
            ),
            chunk(
                json!({"audio":{"data":"AQ","transcript":"hel"}}),
                Value::Null,
            ),
            chunk(
                json!({"audio":{"data":"IDBA==","transcript":"lo"}}),
                Value::Null,
            ),
            chunk(json!({"audio":{"expires_at":2000}}), Value::Null),
            chunk(json!({}), json!("stop")),
        ] {
            let events = decoder.push(&payload).map_err(|_| ())?;
            if encoder.is_none() {
                encoder = Some(
                    ChatSseEncoder::new(
                        decoder.metadata().ok_or(())?.clone(),
                        contract(),
                        SseLimits::default(),
                        morphiecore::semantic::context::StreamOptions {
                            include_obfuscation: Presence::Value(false),
                            ..Default::default()
                        },
                        Obfuscation::Disabled,
                    )
                    .map_err(|_| ())?,
                );
            }
            for event in events {
                frames.extend(
                    encoder
                        .as_mut()
                        .ok_or(())?
                        .encode(&event, decoder.fidelity())
                        .map_err(|_| ())?,
                );
            }
        }
        for event in decoder.done().map_err(|_| ())? {
            frames.extend(
                encoder
                    .as_mut()
                    .ok_or(())?
                    .encode(&event, decoder.fidelity())
                    .map_err(|_| ())?,
            );
        }
        encoder.as_ref().ok_or(())?.finish().map_err(|_| ())?;
        Ok((true, frames.into_iter().flatten().collect()))
    })();
    let mut state = context.0.lock().unwrap();
    state.0 += 1;
    match result {
        Ok((stream, bytes)) => Response::builder()
            .header(
                "content-type",
                if stream {
                    "text/event-stream"
                } else {
                    "application/json"
                },
            )
            .body(Body::from(bytes))
            .unwrap(),
        Err(()) => {
            state.1 = true;
            Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body(Body::empty())
                .unwrap()
        }
    }
}
#[tokio::test]
#[ignore = "requires pinned SDK, synthetic Chat audio/citation and ID history"]
async fn sdk_chat_media_values_streams_and_history() {
    let context = Context::default();
    let state = context.clone();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new()
                .route("/v1/chat/completions", post(handle))
                .with_state(state),
        )
        .await
        .unwrap()
    });
    let guard = crate::ServerGuard(server.abort_handle());
    let mut command = Command::new(
        std::env::var_os("MORPHIECORE_SDK_PYTHON").unwrap_or_else(|| "python3".into()),
    );
    command
        .args([
            "tests/sdk/chat_media_loop.py",
            &format!("http://{address}/v1"),
        ])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for name in [
        "OPENAI_API_KEY",
        "OPENAI_BASE_URL",
        "OPENAI_ORG_ID",
        "OPENAI_PROJECT_ID",
        "MORPHIECORE_CONFIG",
    ] {
        command.env_remove(name);
    }
    let output = crate::child_process::run(&mut command, b"", Duration::from_secs(35), 65536)
        .await
        .unwrap();
    guard.0.abort();
    let _ = server.await;
    assert!(
        output.status.success(),
        "synthetic SDK failure: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(*context.0.lock().unwrap(), (5, false));
}
