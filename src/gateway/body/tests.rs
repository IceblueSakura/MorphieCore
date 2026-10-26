//! Delivery owner tests use an instrumented HTTP body, not a second Router.
use super::*;
use crate::{
    gateway::{
        BoundCandidate, BoundEntry,
        generation::exchange::{Upstreams, produce_chain, respond_source},
        tests::{responses_wire, support::gateway},
    },
    protocol::{ClientProtocol, GenerationRequestEnvelope as Request, openai::GenerationApi},
};
use futures_util::StreamExt;
use std::{sync::atomic::AtomicUsize, time::Duration};
#[allow(clippy::too_many_arguments)]
async fn respond(
    entry: Arc<BoundEntry>,
    candidate: Arc<crate::gateway::BoundCandidate>,
    request: Request,
    upstream: reqwest::Response,
    limits: &Limits,
    deadline: Instant,
    shutdown: tokio_util::sync::CancellationToken,
    permit: OwnedSemaphorePermit,
    trace: Trace,
) -> Result<Response, ApiError> {
    respond_source(
        entry,
        request,
        Upstreams::Observed {
            candidate,
            response: upstream,
        },
        limits.clone(),
        deadline,
        shutdown,
        permit,
        trace,
    )
    .await
}
#[allow(dead_code)]
#[path = "../../../tests/support/chat_profile.rs"]
mod wire;
struct Tracked {
    rx: mpsc::Receiver<Result<Bytes, std::io::Error>>,
    reads: Arc<AtomicUsize>,
    dropped: Option<oneshot::Sender<()>>,
}
impl Stream for Tracked {
    type Item = Result<Bytes, std::io::Error>;
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let result = self.rx.poll_recv(cx);
        if matches!(&result, Poll::Ready(Some(Ok(_)))) {
            self.reads.fetch_add(1, Ordering::SeqCst);
        }
        result
    }
}
impl Drop for Tracked {
    fn drop(&mut self) {
        if let Some(done) = self.dropped.take() {
            let _ = done.send(());
        }
    }
}
type UpstreamProbe = (
    mpsc::Sender<Result<Bytes, std::io::Error>>,
    reqwest::Response,
    oneshot::Receiver<()>,
    Arc<AtomicUsize>,
);
fn upstream(status: u16, media: &str) -> UpstreamProbe {
    let (tx, rx) = mpsc::channel(2);
    let (done, dropped) = oneshot::channel();
    let reads = Arc::new(AtomicUsize::new(0));
    let body = reqwest::Body::wrap_stream(Tracked {
        rx,
        reads: reads.clone(),
        dropped: Some(done),
    });
    let response = reqwest::Response::from(
        axum::http::Response::builder()
            .status(status)
            .header("content-type", media)
            .body(body)
            .unwrap(),
    );
    (tx, response, dropped, reads)
}
fn frames() -> Vec<Bytes> {
    wire::events(2)
        .iter()
        .map(|v| Bytes::from(format!("data: {v}\n\n")))
        .collect()
}
async fn response(
    gate: &crate::gateway::Gateway,
    upstream: reqwest::Response,
    stream: bool,
    deadline: Instant,
) -> Result<Response, ApiError> {
    let entry = gate.state.entries[&(
        crate::protocol::ClientProtocol::Chat,
        "fixture-model".into(),
    )]
        .clone();
    let request=entry.client.as_openai().unwrap().0.decode_request(serde_json::json!({"model":"fixture-model","messages":[{"role":"user","content":"hello"}],"stream":stream,"stream_options":if stream {serde_json::json!({"include_usage":true,"include_obfuscation":false})}else{serde_json::Value::Null}}).to_string().as_bytes()).unwrap();
    let candidate = entry.candidates[0].clone();
    let mut trace = super::Trace::new(gate.state.diagnostics.as_ref());
    trace.capture = gate.state.capture.as_ref().map(|sink| {
        sink.exchange(
            gate.state.request_ids.next().as_ref(),
            &crate::gateway::ingress::auth::AuthenticatedAgent {
                agent_id: "fixture-agent".into(),
            },
        )
    });
    respond(
        entry,
        candidate,
        request,
        upstream,
        &gate.state.limits,
        deadline,
        gate.state.shutdown.clone(),
        gate.state.permits.clone().acquire_owned().await.unwrap(),
        trace,
    )
    .await
}
async fn observed_response(
    gate: &crate::gateway::Gateway,
    upstream: reqwest::Response,
    stream: bool,
) -> Result<Response, ApiError> {
    // Test-owned codec pair, not activation of an additional public binding.
    let mut entry = gate.state.entries[&(
        crate::protocol::ClientProtocol::Chat,
        "fixture-model".into(),
    )]
        .as_ref()
        .clone();
    entry.client = crate::adapter::ClientAdapter::openai(
        crate::protocol::openai::GenerationCodec::new(
            GenerationApi::Responses,
            crate::protocol::openai::Dialect::Standard,
            None,
        ),
        crate::protocol::openai::TargetContract::full(),
    );
    let entry = Arc::new(entry);
    let template = &entry.candidates[0];
    let mut endpoint = template.endpoint.clone();
    if stream {
        endpoint.protocol = crate::topology::GenerationProtocol::AnthropicMessages;
        endpoint.representation = crate::protocol::anthropic::messages::TargetContract::new(
            crate::protocol::anthropic::messages::Profile::AdaptiveTextTools,
            crate::protocol::anthropic::messages::Dialect::Standard,
        )
        .into();
    } else {
        endpoint.protocol = crate::topology::GenerationProtocol::OpenAiResponses;
        let mut caps = crate::protocol::openai::TargetContract::full();
        caps.adaptation.rules.responses_forced_stream = true;
        endpoint.representation = caps.into();
    }
    let candidate = Arc::new(BoundCandidate {
        endpoint,
        provider: template.provider.clone(),
        secret: template.secret.clone(),
        credential_fallback: false,
        credential_attempts: 1,
        health: template.health.clone(),
        cache_scope: template.cache_scope.clone(),
    });
    let mut request = entry
        .client
        .decode_client_request(
            serde_json::json!({"model":"fixture-model","input":"hello","stream":stream})
                .to_string()
                .as_bytes(),
        )
        .unwrap();
    request.delivery.options =
        crate::semantic::value::Presence::Value(crate::semantic::context::StreamOptions {
            include_obfuscation: crate::semantic::value::Presence::Value(false),
            ..Default::default()
        });
    respond(
        entry,
        candidate,
        request,
        upstream,
        &gate.state.limits,
        Instant::now() + Duration::from_secs(3),
        gate.state.shutdown.clone(),
        gate.state.permits.clone().acquire_owned().await.unwrap(),
        super::Trace::new(gate.state.diagnostics.as_ref()),
    )
    .await
}
#[tokio::test]
async fn diagnostic_status_and_final_intake_are_from_the_original_attempt() {
    let directory = crate::test_files::private_directory();
    let path = directory.path().join("events.jsonl");
    let capture_path = directory.path().join("capture.jsonl");
    let config = serde_json::from_value(serde_json::json!({
        "path":capture_path,"upstream_response":{"headers":true,"body":true}
    }))
    .unwrap();
    let gate = gateway(Limits::default())
        .with_diagnostics(&path)
        .unwrap()
        .with_capture(config)
        .unwrap();
    for status in [429, 503, 200] {
        let (tx, mut source, _, _) = upstream(status, "application/json");
        source
            .headers_mut()
            .insert("retry-after", "7".parse().unwrap());
        tx.send(Ok(Bytes::from_static(b"synthetic-private-invalid-json")))
            .await
            .unwrap();
        drop(tx);
        let error = response(
            &gate,
            source,
            false,
            Instant::now() + Duration::from_secs(2),
        )
        .await
        .err()
        .unwrap();
        assert_eq!(
            error.status.as_u16(),
            if status == 200 { 502 } else { status }
        );
    }
    gate.flush_diagnostics().await;
    let raw = std::fs::read_to_string(&path).unwrap();
    assert!(!raw.contains("synthetic-private"));
    let records: Vec<serde_json::Value> = raw
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .filter(|value: &serde_json::Value| value["event"] == "request")
        .collect();
    assert_eq!(records.len(), 3);
    for (record, status) in records.iter().zip([429, 503, 200]) {
        assert_eq!(record["upstream_status"], status);
        assert_eq!(record["retry_after_seconds"], 7);
        assert_eq!(
            record["stage"],
            if status == 200 {
                "terminal"
            } else {
                "response_head"
            }
        );
    }
    let (tx, source, _, _) = upstream(503, "application/json");
    tx.send(Ok(Bytes::from_static(
        br#"{"error":{"message":"synthetic-failure"}}"#,
    )))
    .await
    .unwrap();
    tx.send(Err(std::io::Error::other("synthetic read failure")))
        .await
        .unwrap();
    drop(tx);
    assert!(
        response(
            &gate,
            source,
            false,
            Instant::now() + Duration::from_secs(2)
        )
        .await
        .is_err()
    );
    gate.flush_diagnostics().await;
    let captured = std::fs::read_to_string(&capture_path).unwrap();
    let last: Vec<serde_json::Value> = captured
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .filter(|value: &serde_json::Value| value["exchange"] == 4)
        .collect();
    assert!(last.iter().any(|value| value["status"] == "incomplete"));
    assert!(!last.iter().any(|value| value["status"] == "source_eof"));
    let mut entry = (*gate.state.entries[&(
        crate::protocol::ClientProtocol::Chat,
        "fixture-model".into(),
    )])
        .clone();
    entry.client = crate::adapter::ClientAdapter::openai(
        crate::protocol::openai::GenerationCodec::new(
            GenerationApi::Responses,
            crate::protocol::openai::Dialect::MorphieCore,
            None,
        ),
        crate::protocol::openai::TargetContract::full(),
    );
    let entry = Arc::new(entry);
    let request = entry
        .client
        .as_openai()
        .unwrap()
        .0
        .decode_request(br#"{"model":"fixture-model","input":"hello","stream":true}"#)
        .unwrap();
    let candidate = gate.state.entries[&(
        crate::protocol::ClientProtocol::Chat,
        "fixture-model".into(),
    )]
        .candidates[0]
        .clone();
    let (tx, source, _, _) = upstream(200, "text/event-stream");
    for (index, mut chunk) in wire::events(2).into_iter().take(2).enumerate() {
        if index == 1 {
            chunk["system_fingerprint"] = serde_json::json!("synthetic-private-fingerprint");
        }
        tx.send(Ok(Bytes::from(format!("data: {chunk}\n\n"))))
            .await
            .unwrap();
    }
    let response = respond(
        entry,
        candidate,
        request,
        source,
        &gate.state.limits,
        Instant::now() + Duration::from_secs(2),
        gate.state.shutdown.clone(),
        gate.state.permits.clone().acquire_owned().await.unwrap(),
        super::Trace::generation_trace(gate.state.diagnostics.as_ref()),
    )
    .await
    .unwrap();
    tx.send(Err(std::io::Error::other("synthetic truncated stream")))
        .await
        .unwrap();
    drop(tx);
    let mut stream = response.into_body().into_data_stream();
    let mut published = vec![];
    while let Some(frame) = stream.next().await {
        match frame {
            Ok(bytes) => published.extend_from_slice(&bytes),
            Err(_) => break,
        }
    }
    drop(stream);
    assert!(!published.is_empty());
    assert!(
        !published
            .windows(b"response.completed".len())
            .any(|value| value == b"response.completed")
    );
    gate.flush_diagnostics().await;
    let raw = std::fs::read_to_string(&path).unwrap();
    assert!(!raw.contains("synthetic-private-fingerprint"));
    let record: serde_json::Value = raw
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .rfind(|row| row["event"] == "request")
        .unwrap();
    let losses = record["generation"]["attempts"][0]["evidence"]["entries"]
        .as_array()
        .unwrap();
    assert_eq!(
        losses
            .iter()
            .filter(
                |loss| loss["rule"] == "responses.omit-system-fingerprint.v1"
                    && loss["boundary"] == "event.project"
            )
            .count(),
        1
    );
}
#[tokio::test]
async fn upstream_failures_reach_clients_as_agent_error_classes() {
    use axum::response::IntoResponse;
    let gate = gateway(Limits::default());
    for (status, body, retry, expected, code) in [
        // OpenAI-compatible and Anthropic overflow wording.
        (
            400,
            r#"{"error":{"message":"This model's maximum context length is 131072 tokens. However, you requested 140000 tokens.","type":"invalid_request_error","param":null,"code":null}}"#,
            None,
            400,
            "context_length_exceeded",
        ),
        (
            400,
            r#"{"type":"error","error":{"type":"invalid_request_error","message":"prompt is too long: 213462 tokens > 200000 maximum"}}"#,
            None,
            400,
            "context_length_exceeded",
        ),
        // DeepSeek balance and OpenCode Go subscription limits.
        (
            402,
            r#"{"error":{"message":"Insufficient Balance","type":"unknown_error","param":null,"code":"invalid_request_error"}}"#,
            None,
            429,
            "insufficient_quota",
        ),
        (
            429,
            r#"{"type":"error","error":{"type":"GoUsageLimitError","message":"Monthly usage limit reached for 117 private"}}"#,
            None,
            429,
            "insufficient_quota",
        ),
        (
            429,
            r#"{"error":{"message":"Rate limit reached for 117 private","type":"requests","code":"rate_limit_exceeded"}}"#,
            Some("7"),
            429,
            "rate_limit_exceeded",
        ),
        (
            529,
            r#"{"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}}"#,
            None,
            503,
            "server_is_overloaded",
        ),
        // Overflow wording only counts on request errors; other rejections stay generic.
        (
            500,
            r#"{"error":{"message":"maximum context length is 117 tokens"}}"#,
            None,
            502,
            "upstream_error",
        ),
        (
            400,
            r#"{"error":{"message":"Invalid schema for function 117"}}"#,
            None,
            502,
            "upstream_error",
        ),
    ] {
        let (tx, mut source, _, _) = upstream(status, "application/json");
        if let Some(retry) = retry {
            source
                .headers_mut()
                .insert("retry-after", retry.parse().unwrap());
        }
        tx.send(Ok(Bytes::from_static(body.as_bytes())))
            .await
            .unwrap();
        drop(tx);
        let error = response(
            &gate,
            source,
            false,
            Instant::now() + Duration::from_secs(2),
        )
        .await
        .err()
        .unwrap();
        let rendered = error.into_response();
        assert_eq!(rendered.status().as_u16(), expected, "{body}");
        assert_eq!(
            rendered
                .headers()
                .get("retry-after")
                .map(|v| v.to_str().unwrap()),
            retry,
        );
        let bytes = axum::body::to_bytes(rendered.into_body(), 4096)
            .await
            .unwrap();
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(value["error"]["code"], code, "{body}");
        // Gateway-authored text only; agents match on these words.
        let message = value["error"]["message"].as_str().unwrap();
        assert!(!message.contains("117") && !message.contains("213462"));
        match code {
            "context_length_exceeded" => assert!(message.contains("exceeds the context window")),
            "insufficient_quota" => assert!(message.contains("billing")),
            "rate_limit_exceeded" => assert!(message.contains("try again in 7s")),
            _ => {}
        }
    }
}
#[tokio::test]
async fn chat_in_band_stream_errors_keep_their_agent_error_class() {
    use axum::response::IntoResponse;
    // Before publication the classified failure is the HTTP answer.
    let gate = gateway(Limits::default());
    let (tx, source, _, _) = upstream(200, "text/event-stream");
    tx.send(Ok(Bytes::from_static(
        br#"data: {"error":{"message":"private upstream detail","type":"insufficient_quota","param":null,"code":"insufficient_quota"}}

"#,
    )))
    .await
    .unwrap();
    drop(tx);
    let error = bounded(response(
        &gate,
        source,
        true,
        Instant::now() + Duration::from_secs(3),
    ))
    .await
    .err()
    .unwrap();
    let rendered = error.into_response();
    assert_eq!(rendered.status().as_u16(), 429);
    let bytes = axum::body::to_bytes(rendered.into_body(), 4096)
        .await
        .unwrap();
    let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(value["error"]["code"], "insufficient_quota");
    assert!(!value.to_string().contains("private upstream detail"));
    // After publication an OpenRouter-style chunk error becomes a failure frame.
    let gate = gateway(Limits::default());
    let (tx, source, _, _) = upstream(200, "text/event-stream");
    tx.send(Ok(frames()[..3].concat().into())).await.unwrap();
    let response = bounded(response(
        &gate,
        source,
        true,
        Instant::now() + Duration::from_secs(3),
    ))
    .await
    .unwrap();
    tx.send(Ok(Bytes::from_static(
        br#"data: {"id":"chunk","object":"chat.completion.chunk","created":1,"model":"private","error":{"code":429,"message":"private upstream detail"},"choices":[{"index":0,"delta":{"content":""},"finish_reason":"error"}]}

"#,
    )))
    .await
    .unwrap();
    drop(tx);
    let mut body = response.into_body().into_data_stream();
    let mut seen = vec![];
    while let Some(Ok(bytes)) = bounded(body.next()).await {
        seen.extend_from_slice(&bytes);
    }
    let wire = String::from_utf8_lossy(&seen);
    assert!(wire.contains("🧪"));
    assert!(wire.contains(r#""code":"rate_limit_exceeded""#), "{wire}");
    assert!(!wire.contains("private upstream detail") && !wire.contains("[DONE]"));
}
#[tokio::test]
async fn failed_responses_reach_chat_clients_as_classified_failures() {
    use axum::response::IntoResponse;
    async fn chat_over_responses(
        gate: &crate::gateway::Gateway,
        upstream: reqwest::Response,
        stream: bool,
        report: crate::execution::report::Report,
    ) -> Result<Response, ApiError> {
        // Test-owned codec pair: a Chat client over a Responses target.
        let entry = gate.state.entries[&(
            crate::protocol::ClientProtocol::Chat,
            "fixture-model".into(),
        )]
            .clone();
        let template = &entry.candidates[0];
        let mut endpoint = template.endpoint.clone();
        endpoint.protocol = crate::topology::GenerationProtocol::OpenAiResponses;
        endpoint.representation = crate::protocol::openai::TargetContract::full().into();
        let candidate = Arc::new(BoundCandidate {
            endpoint,
            ..template.as_ref().clone()
        });
        let request = entry
            .client
            .decode_client_request(
                serde_json::json!({"model":"fixture-model","stream":stream,
                    "messages":[{"role":"user","content":"hello"}]})
                .to_string()
                .as_bytes(),
            )
            .unwrap();
        let mut trace = super::Trace::new(gate.state.diagnostics.as_ref());
        trace.report = report;
        respond(
            entry,
            candidate,
            request,
            upstream,
            &gate.state.limits,
            Instant::now() + Duration::from_secs(3),
            gate.state.shutdown.clone(),
            gate.state.permits.clone().acquire_owned().await.unwrap(),
            trace,
        )
        .await
    }
    let mut failed = serde_json::json!({"id":"failed","object":"response","created_at":1,
        "model":"fixture-model","status":"failed",
        "error":{"code":"rate_limit_exceeded","message":"private upstream detail"},
        "output":[{"id":"message","type":"message","role":"assistant","status":"incomplete",
            "content":[{"type":"output_text","text":"partial 🧪","annotations":[],"logprobs":[]}]}]});
    // Static: no Chat carrier for a failed response, so the class is the HTTP answer.
    let gate = gateway(Limits::default());
    let (tx, source, _, _) = upstream(200, "application/json");
    tx.send(Ok(failed.to_string().into())).await.unwrap();
    drop(tx);
    let report = crate::execution::report::Report::default();
    let error = bounded(chat_over_responses(&gate, source, false, report.clone()))
        .await
        .err()
        .unwrap();
    let rendered = error.into_response();
    assert_eq!(rendered.status().as_u16(), 429);
    let bytes = axum::body::to_bytes(rendered.into_body(), 4096)
        .await
        .unwrap();
    assert!(String::from_utf8_lossy(&bytes).contains(r#""code":"rate_limit_exceeded""#));
    assert!(!String::from_utf8_lossy(&bytes).contains("private upstream detail"));
    let observed = report.attempt();
    assert!(observed.source_eof);
    assert!(matches!(
        observed.task_outcome,
        Some(crate::execution::report::TaskOutcome::Failed)
    ));
    assert_eq!(observed.observed_status, Some(200));
    assert_eq!(
        observed.source_failure.unwrap().kind,
        crate::protocol::failure::SourceFailureKind::RateLimit
    );
    // Streamed: published content stays, then the classified failure frame.
    let error = failed["error"].take();
    let mut events = responses_wire::response_events(failed);
    let last = events.last_mut().unwrap();
    last["type"] = serde_json::json!("response.failed");
    last["response"]["error"] = error;
    let wire: Vec<u8> = events
        .iter()
        .flat_map(|event| format!("data: {event}\n\n").into_bytes())
        .collect();
    let gate = gateway(Limits::default());
    let (tx, source, _, _) = upstream(200, "text/event-stream");
    tx.send(Ok(wire.into())).await.unwrap();
    drop(tx);
    let response = bounded(chat_over_responses(&gate, source, true, Default::default()))
        .await
        .unwrap();
    let mut body = response.into_body().into_data_stream();
    let mut seen = vec![];
    while let Some(Ok(bytes)) = bounded(body.next()).await {
        seen.extend_from_slice(&bytes);
    }
    let wire = String::from_utf8_lossy(&seen);
    assert!(wire.contains("partial 🧪"), "{wire}");
    assert!(wire.contains(r#""code":"rate_limit_exceeded""#), "{wire}");
    assert!(!wire.contains("private upstream detail") && !wire.contains("[DONE]"));
}
#[tokio::test]
async fn messages_clients_get_anthropic_shapes_from_an_openai_target() {
    let gate = gateway(Limits::default());
    let entry = gate.state.entries[&(
        crate::protocol::ClientProtocol::Messages,
        "fixture-model".into(),
    )]
        .clone();
    // A failed tool result keeps its text toward an OpenAI target; the flag is named.
    let body = serde_json::json!({"model":"fixture-model","max_tokens":32,
        "tools":[{"name":"lookup","input_schema":{"type":"object","properties":{}}}],"messages":[
        {"role":"user","content":"q"},
        {"role":"assistant","content":[{"type":"tool_use","id":"call-local","name":"lookup","input":{}}]},
        {"role":"user","content":[{"type":"tool_result","tool_use_id":"call-local","content":"boom","is_error":true}]}]});
    let (_, request) = crate::gateway::ingress::admission::prepare(
        &gate.state,
        ClientProtocol::Messages,
        body.to_string().as_bytes(),
        &crate::execution::report::Report::default(),
    )
    .unwrap();
    assert!(request.ignored_fields.is_empty());
    assert!(
        request
            .task
            .semantic
            .items()
            .iter()
            .any(|(_, item)| matches!(
                item, crate::semantic::task::generation::Item::ToolResult(result)
                    if result.is_error == Some(true)
            ))
    );
    // Messages turns regroup as Chat messages for an OpenAI target.
    let (chat, rules) = entry.candidates[0]
        .endpoint
        .encode_request_with_rules(&request)
        .unwrap();
    assert!(rules.contains(&"openai.wrap-tool-result-errors.v1"));
    assert_eq!(
        chat["messages"][1],
        serde_json::json!({"role":"assistant","content":null,"tool_calls":[{"id":"call-local","type":"function","function":{"name":"lookup","arguments":"{}"}}]})
    );
    let (tx, upstream, _, _) = upstream(200, "application/json");
    let mut source = wire::response(1);
    // Messages needs the reported cache-read subset; Chat has no write category.
    source["usage"]["prompt_tokens_details"] = serde_json::json!({"cached_tokens":0});
    tx.send(Ok(Bytes::from(source.to_string()))).await.unwrap();
    drop(tx);
    let response = respond(
        entry.clone(),
        entry.candidates[0].clone(),
        request.clone(),
        upstream,
        &gate.state.limits,
        Instant::now() + Duration::from_secs(2),
        gate.state.shutdown.clone(),
        gate.state.permits.clone().acquire_owned().await.unwrap(),
        super::Trace::new(None),
    )
    .await
    .unwrap();
    let bytes = axum::body::to_bytes(response.into_body(), 4096)
        .await
        .unwrap();
    let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(value["type"], "message");
    assert_eq!(value["model"], "fixture-model");
    assert_eq!(
        value["content"],
        serde_json::json!([{"type":"tool_use","id":"call-local","name":"lookup","input":{"n":1}}])
    );
    assert_eq!(value["stop_reason"], "tool_use");
    assert_eq!(
        value["usage"],
        serde_json::json!({"input_tokens":3,"output_tokens":2,"cache_read_input_tokens":0})
    );
    // Errors use Anthropic's envelope with the wording Claude Code reacts to.
    let error = ApiError::failure(
        400,
        br#"{"error":{"message":"maximum context length is 9 tokens"}}"#,
        None,
    )
    .into_messages_response();
    assert_eq!(error.status(), 400);
    let bytes = axum::body::to_bytes(error.into_body(), 4096).await.unwrap();
    let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(value["type"], "error");
    assert_eq!(value["error"]["type"], "invalid_request_error");
    assert!(
        value["error"]["message"]
            .as_str()
            .unwrap()
            .starts_with("prompt is too long")
    );
}
async fn bounded<T>(future: impl std::future::Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(2), future)
        .await
        .expect("bounded owner check")
}
#[tokio::test]
async fn first_frame_precedes_terminal_and_ack_gates_upstream_progress() {
    let gate = gateway(Limits::default());
    let (tx, upstream, dropped, reads) = upstream(200, "text/event-stream");
    let frames = frames();
    tx.send(Ok(Bytes::from(frames[..2].concat())))
        .await
        .unwrap();
    let response = bounded(response(
        &gate,
        upstream,
        true,
        Instant::now() + Duration::from_secs(3),
    ))
    .await
    .unwrap();
    // The worker is waiting for HTTP-body acknowledgement, not collecting the stream.
    tx.send(Ok(Bytes::from(
        [frames[2..].concat(), b"data: [DONE]\n\n".to_vec()].concat(),
    )))
    .await
    .unwrap();
    drop(tx);
    assert_eq!(reads.load(Ordering::SeqCst), 1);
    assert_eq!(response.headers()["x-accel-buffering"], "no");
    assert_eq!(response.headers()["cache-control"], "no-store");
    assert!(!response.headers().contains_key("content-encoding"));
    let mut body = response.into_body().into_data_stream();
    let first = bounded(body.next()).await.unwrap().unwrap();
    assert!(!String::from_utf8_lossy(&first).contains("[DONE]"));
    let mut all = first.to_vec();
    bounded(async {
        while let Some(chunk) = body.next().await {
            all.extend_from_slice(&chunk.unwrap());
        }
    })
    .await;
    assert!(String::from_utf8_lossy(&all).contains("old "));
    assert_eq!(String::from_utf8_lossy(&all).matches("[DONE]").count(), 1);
    bounded(dropped).await.unwrap();
    let mut measurements = vec![];
    for enabled in [false, true] {
        let directory = crate::test_files::private_directory();
        let path = directory.path().join("events.jsonl");
        let policy = Limits::default().delta_coalescing.unwrap();
        let gate = gateway(Limits {
            delta_coalescing: enabled.then_some(policy),
            ..Limits::default()
        })
        .with_diagnostics(&path)
        .unwrap();
        let event = |value: serde_json::Value| {
            format!(
                "event: {}\ndata: {value}\n\n",
                value["type"].as_str().unwrap()
            )
        };
        let text = |fragment: &str| {
            event(serde_json::json!({
                "type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":fragment}
            }))
        };
        let prefix = [
            event(serde_json::json!({"type":"message_start","message":{
                "id":"native","type":"message","role":"assistant","model":"private-fixture",
                "content":[],"stop_reason":null,"stop_sequence":null,
                "usage":{"input_tokens":11,"output_tokens":0,"cache_creation_input_tokens":0,"cache_read_input_tokens":0}
            }})),
            event(serde_json::json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}})),
            text("start"),
        ].concat();
        let (tx, source, dropped, reads) = self::upstream(200, "text/event-stream");
        let started = Instant::now();
        tx.send(Ok(Bytes::from(prefix))).await.unwrap();
        let response = bounded(observed_response(&gate, source, true))
            .await
            .unwrap();
        assert_eq!(reads.load(Ordering::SeqCst), 1);
        let mut body = response.into_body().into_data_stream();
        let mut all = vec![];
        let mut handoffs = 0;
        let mut fragments = vec![];
        let collect = |chunk: &[u8], fragments: &mut Vec<String>| {
            for line in std::str::from_utf8(chunk).unwrap().lines() {
                if let Some(data) = line.strip_prefix("data: ")
                    && let Ok(value) = serde_json::from_str::<serde_json::Value>(data)
                    && value["type"] == "response.output_text.delta"
                {
                    fragments.push(value["delta"].as_str().unwrap().to_owned());
                }
            }
        };
        while fragments.concat() != "start" {
            let chunk = bounded(body.next()).await.unwrap().unwrap();
            collect(&chunk, &mut fragments);
            all.extend_from_slice(&chunk);
            handoffs += 1;
        }
        let first_ms = started.elapsed().as_secs_f64() * 1000.0;
        let update = Instant::now();
        tx.send(Ok(Bytes::from(text("x").repeat(32))))
            .await
            .unwrap();
        // No further upstream input or EOF: a timer must release this run.
        while fragments.concat().len() < 37 {
            let chunk = bounded(body.next()).await.unwrap().unwrap();
            collect(&chunk, &mut fragments);
            all.extend_from_slice(&chunk);
            handoffs += 1;
        }
        let update_ms = update.elapsed().as_secs_f64() * 1000.0;
        assert_eq!(fragments.concat(), format!("start{}", "x".repeat(32)));
        let suffix = [
            event(serde_json::json!({"type":"content_block_stop","index":0})),
            event(serde_json::json!({"type":"message_delta","delta":{"stop_reason":"max_tokens","stop_sequence":null},"usage":{"output_tokens":7}})),
            event(serde_json::json!({"type":"message_stop"})),
        ].concat();
        tx.send(Ok(Bytes::from(suffix))).await.unwrap();
        drop(tx);
        while let Some(chunk) = bounded(body.next()).await {
            all.extend_from_slice(&chunk.unwrap());
            handoffs += 1;
        }
        bounded(dropped).await.unwrap();
        let wire = std::str::from_utf8(&all).unwrap();
        let mut decoder = crate::protocol::openai::GenerationCodec::new(
            GenerationApi::Responses,
            crate::protocol::openai::Dialect::Standard,
            None,
        )
        .event_decoder();
        for line in wire.lines().filter_map(|line| line.strip_prefix("data: ")) {
            decoder.push(&serde_json::from_str(line).unwrap()).unwrap();
        }
        decoder.finish().unwrap();
        let decoded = decoder.materialize().unwrap();
        assert_eq!(
            decoded.semantic.outcome(),
            crate::semantic::task::generation::Outcome::Incomplete
        );
        assert_eq!(decoded.semantic.usage().unwrap().total_tokens, Some(18));
        gate.flush_diagnostics().await;
        let record: serde_json::Value = std::fs::read_to_string(&path)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
            .find(|row| row["event"] == "request")
            .unwrap();
        let peak = record["coalescing_peak_bytes"].as_u64().unwrap_or(0) as usize;
        assert!(peak <= policy.max_pending_bytes);
        assert_eq!(peak > 0, enabled);
        println!(
            "coalescing={enabled} text_events={} encoded_bytes={} handoffs={handoffs} first_content_ms={first_ms:.3} update_ms={update_ms:.3} peak_pending_bytes={peak}",
            fragments.len(),
            all.len()
        );
        measurements.push((fragments.len(), all.len(), handoffs));
    }
    assert!(measurements[1].0 < measurements[0].0);
    assert!(measurements[1].1 < measurements[0].1);
    assert!(measurements[1].2 < measurements[0].2);
    let gate = gateway(Limits {
        delta_coalescing: Some(crate::gateway::DeltaCoalescing {
            max_delay_ms: 10,
            max_pending_bytes: 16384,
        }),
        ..Limits::default()
    });
    let (tx, source, dropped, _) = self::upstream(200, "text/event-stream");
    let wire = responses_wire::events(2)
        .iter()
        .map(|value| {
            format!(
                "event: {}\ndata: {value}\n\n",
                value["type"].as_str().unwrap()
            )
        })
        .collect::<String>();
    tx.send(Ok(Bytes::from(wire))).await.unwrap();
    drop(tx);
    let response = bounded(observed_response(&gate, source, false))
        .await
        .unwrap();
    assert_eq!(response.headers()["content-type"], "application/json");
    assert!(!response.headers().contains_key("x-accel-buffering"));
    let bytes = bounded(axum::body::to_bytes(response.into_body(), 65536))
        .await
        .unwrap();
    let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(value["output"][0]["content"][0]["text"], "{\"ok\":false}");
    assert_eq!(value["usage"]["total_tokens"], 8);
    bounded(dropped).await.unwrap();
}
#[tokio::test]
async fn late_truncation_and_trailing_payload_never_emit_success_terminal() {
    for trailing in [false, true] {
        let gate = gateway(Limits {
            delta_coalescing: Some(crate::gateway::DeltaCoalescing {
                max_delay_ms: 10,
                max_pending_bytes: 16384,
            }),
            ..Limits::default()
        });
        let (tx, upstream, _, _) = upstream(200, "text/event-stream");
        let mut bytes = if trailing {
            frames().concat()
        } else {
            frames()[..3].concat()
        };
        if trailing {
            bytes.extend_from_slice(b"data: [DONE]\n\ndata: {}\n\n");
        }
        tx.send(Ok(Bytes::from(bytes))).await.unwrap();
        drop(tx);
        let response = bounded(response(
            &gate,
            upstream,
            true,
            Instant::now() + Duration::from_secs(3),
        ))
        .await
        .unwrap();
        let mut body = response.into_body().into_data_stream();
        let mut seen = Vec::new();
        let mut failed = false;
        bounded(async {
            while let Some(next) = body.next().await {
                match next {
                    Ok(bytes) => seen.extend_from_slice(&bytes),
                    Err(_) => {
                        failed = true;
                        break;
                    }
                }
            }
        })
        .await;
        assert!(failed);
        assert!(String::from_utf8_lossy(&seen).contains("🧪"));
        assert!(!String::from_utf8_lossy(&seen).contains("[DONE]"));
    }
    // Six downstream frames fit through pending text; a classified failure's
    // envelope then exceeds the event budget. The pending prefix still arrives.
    let gate = gateway(Limits {
        max_events: 6,
        delta_coalescing: Some(crate::gateway::DeltaCoalescing {
            max_delay_ms: 100,
            max_pending_bytes: 16384,
        }),
        ..Limits::default()
    });
    let event = |value: serde_json::Value| {
        format!(
            "event: {}\ndata: {value}\n\n",
            value["type"].as_str().unwrap()
        )
    };
    let prefix = [
        event(serde_json::json!({"type":"message_start","message":{
            "id":"native","type":"message","role":"assistant","model":"private-fixture",
            "content":[],"stop_reason":null,"stop_sequence":null,
            "usage":{"input_tokens":11,"output_tokens":0,"cache_creation_input_tokens":0,"cache_read_input_tokens":0}
        }})),
        event(serde_json::json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}})),
        event(serde_json::json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"first"}})),
    ].concat();
    let (tx, source, dropped, _) = upstream(200, "text/event-stream");
    tx.send(Ok(Bytes::from(prefix))).await.unwrap();
    let response = bounded(observed_response(&gate, source, true))
        .await
        .unwrap();
    let tail = [
        event(serde_json::json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"pending"}})),
        event(serde_json::json!({"type":"error","error":{"type":"overloaded_error","message":"synthetic"}})),
    ].concat();
    tx.send(Ok(Bytes::from(tail))).await.unwrap();
    drop(tx);
    let mut body = response.into_body().into_data_stream();
    let mut seen = vec![];
    let mut failed = false;
    while let Some(chunk) = bounded(body.next()).await {
        match chunk {
            Ok(bytes) => seen.extend_from_slice(&bytes),
            Err(_) => {
                failed = true;
                break;
            }
        }
    }
    assert!(failed);
    bounded(dropped).await.unwrap();
    let wire = std::str::from_utf8(&seen).unwrap();
    let pending = wire
        .lines()
        .filter_map(|line| line.strip_prefix("data: "))
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|v| v["type"] == "response.output_text.delta" && v["delta"] == "pending")
        .count();
    assert_eq!(pending, 1);
    assert!(!wire.contains("response.completed"));
}
#[tokio::test]
async fn queued_first_frame_timeout_preserves_the_final_public_error() {
    let gate = gateway(Limits {
        concurrency: 1,
        ..Default::default()
    });
    let (tx, upstream, _, _) = upstream(200, "application/json");
    tx.send(Ok(Bytes::from(wire::response(1).to_string())))
        .await
        .unwrap();
    drop(tx);
    let entry = gate.state.entries[&(
        crate::protocol::ClientProtocol::Chat,
        "fixture-model".into(),
    )]
        .clone();
    let request = entry
        .client
        .as_openai()
        .unwrap()
        .0
        .decode_request(
            br#"{"model":"fixture-model","messages":[{"role":"user","content":"hello"}]}"#,
        )
        .unwrap();
    let trace = Trace::generation_trace(None);
    let report = trace.report.clone();
    let permit = gate.state.permits.clone().acquire_owned().await.unwrap();
    let pending = respond(
        entry.clone(),
        entry.candidates[0].clone(),
        request,
        upstream,
        &gate.state.limits,
        Instant::now() + Duration::from_millis(100),
        gate.state.shutdown.clone(),
        permit,
        trace,
    );
    tokio::pin!(pending);
    assert!(futures_util::poll!(&mut pending).is_pending());
    let _released = bounded(gate.state.permits.clone().acquire_owned())
        .await
        .unwrap();
    let snapshot = report.snapshot();
    assert!(
        snapshot.attempts[0].checked_units > 0,
        "must reach queued output"
    );
    assert_eq!(snapshot.attempts[0].handed_off_frames, 0);
    let error = pending.await.unwrap_err();
    assert_eq!(error.status, 504);
    assert_eq!(snapshot.failure_code, Some(error.diagnostic_code()));
    assert_eq!(snapshot.failure_status_hint, Some(504));
}

#[tokio::test]
async fn drop_deadline_and_shutdown_release_upstream_even_when_body_is_unpolled() {
    for mode in ["drop", "deadline", "shutdown", "server-drop"] {
        let gate = gateway(Limits {
            concurrency: 1,
            delta_coalescing: Some(crate::gateway::DeltaCoalescing {
                max_delay_ms: 10,
                max_pending_bytes: 16384,
            }),
            ..Limits::default()
        });
        let (tx, upstream, dropped, _) = upstream(200, "text/event-stream");
        tx.send(Ok(Bytes::from(frames()[..2].concat())))
            .await
            .unwrap();
        let deadline = Instant::now()
            + if mode == "deadline" {
                Duration::from_millis(40)
            } else {
                Duration::from_secs(3)
            };
        let response = bounded(response(&gate, upstream, true, deadline))
            .await
            .unwrap();
        if mode == "drop" {
            drop(response);
        } else {
            if mode == "shutdown" {
                gate.shutdown();
            } else if mode == "server-drop" {
                let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
                let (ready, started) = oneshot::channel();
                let serving = tokio::spawn(gate.clone().serve(listener, async {
                    let _ = ready.send(());
                    std::future::pending::<()>().await;
                }));
                bounded(started).await.unwrap();
                serving.abort();
                let _ = serving.await;
            }
            bounded(dropped).await.unwrap(); // No HTTP-body poll has occurred.
            assert!(
                bounded(response.into_body().into_data_stream().next())
                    .await
                    .unwrap()
                    .is_err()
            );
            let _released = bounded(gate.state.permits.clone().acquire_owned())
                .await
                .unwrap();
            continue;
        }
        bounded(dropped).await.unwrap();
        let _released = bounded(gate.state.permits.clone().acquire_owned())
            .await
            .unwrap();
    }
    // A timer flush blocked on acknowledgement cannot read ahead; dropping the
    // body cancels that flush and releases its upstream and exchange permit.
    let gate = gateway(Limits {
        concurrency: 1,
        delta_coalescing: Some(crate::gateway::DeltaCoalescing {
            max_delay_ms: 10,
            max_pending_bytes: 16384,
        }),
        ..Limits::default()
    });
    let (tx, source, dropped, reads) = upstream(200, "text/event-stream");
    tx.send(Ok(Bytes::from(frames()[..2].concat())))
        .await
        .unwrap();
    let response = bounded(response(
        &gate,
        source,
        true,
        Instant::now() + Duration::from_secs(3),
    ))
    .await
    .unwrap();
    let mut body = response.into_body().into_data_stream();
    let mut prefix = vec![];
    while !String::from_utf8_lossy(&prefix).contains("old ") {
        prefix.extend_from_slice(&bounded(body.next()).await.unwrap().unwrap());
    }
    tx.send(Ok(frames()[2].clone())).await.unwrap();
    tokio::time::sleep(Duration::from_millis(30)).await;
    tx.send(Ok(frames()[2].clone())).await.unwrap();
    assert_eq!(reads.load(Ordering::SeqCst), 2);
    drop(body);
    bounded(dropped).await.unwrap();
    let _released = bounded(gate.state.permits.clone().acquire_owned())
        .await
        .unwrap();
}
#[tokio::test]
async fn source_accounting_survives_interruption_without_becoming_delivery_estimates() {
    use crate::gateway::ingress::auth::AuthenticatedAgent;
    for mode in ["cancel", "cut", "static"] {
        let directory = crate::test_files::private_directory();
        let path = directory.path().join("events.jsonl");
        let mut gate = gateway(Limits {
            concurrency: 1,
            ..Limits::default()
        });
        if mode != "cancel" {
            gate = gate.with_diagnostics(&path).unwrap();
        }
        let entry = gate.state.entries[&(
            crate::protocol::ClientProtocol::Chat,
            "fixture-model".into(),
        )]
            .clone();
        let streaming = mode != "static";
        let request = entry.client.decode_client_request(
            serde_json::json!({"model":"fixture-model","messages":[{"role":"user","content":"hello"}],
                "stream":streaming,"stream_options":if streaming {serde_json::json!({"include_obfuscation":false})} else {serde_json::Value::Null}})
                .to_string().as_bytes()
        ).unwrap();
        let id = gate.state.request_ids.next().unwrap();
        let mut incoming = axum::extract::Request::new(axum::body::Body::empty());
        incoming.extensions_mut().insert(id.clone());
        incoming.extensions_mut().insert(AuthenticatedAgent {
            agent_id: "fixture-agent".into(),
        });
        let mut trace = Trace::for_request(gate.state.diagnostics.as_ref(), &incoming);
        trace.generation();
        let report = trace.report.clone();
        let (observed, ready) = oneshot::channel();
        let (interrupt, interrupted) = oneshot::channel();
        let first = if streaming {
            Bytes::from(frames().concat())
        } else {
            Bytes::from(wire::response(2).to_string())
        };
        let source = futures_util::stream::once(async move { Ok::<_, std::io::Error>(first) })
            .chain(futures_util::stream::once(async move {
                // Asking for another transport unit proves the previous unit and
                // its accounting were processed; no timer or log polling is needed.
                let _ = observed.send(());
                if streaming {
                    let _ = interrupted.await;
                    Err(std::io::Error::other(
                        "synthetic cut after source accounting",
                    ))
                } else {
                    Ok(Bytes::new())
                }
            }));
        let source = reqwest::Response::from(
            axum::http::Response::builder()
                .status(200)
                .header(
                    "content-type",
                    if streaming {
                        "text/event-stream"
                    } else {
                        "application/json"
                    },
                )
                .body(reqwest::Body::wrap_stream(source))
                .unwrap(),
        );
        let response = bounded(respond(
            entry.clone(),
            entry.candidates[0].clone(),
            request,
            source,
            &gate.state.limits,
            Instant::now() + Duration::from_secs(3),
            gate.state.shutdown.clone(),
            gate.state.permits.clone().acquire_owned().await.unwrap(),
            trace,
        ))
        .await
        .unwrap_or_else(|error| panic!("{mode}: {error:?}; {:?}", report.snapshot()));
        let reader =
            tokio::spawn(async move { axum::body::to_bytes(response.into_body(), 65536).await });
        bounded(ready).await.unwrap();
        if mode == "cancel" {
            reader.abort();
            assert!(reader.await.unwrap_err().is_cancelled());
        } else if mode == "cut" {
            interrupt.send(()).unwrap();
            assert!(bounded(reader).await.unwrap().is_err());
        } else {
            bounded(reader).await.unwrap().unwrap();
        }
        let _permit = bounded(gate.state.permits.clone().acquire_owned())
            .await
            .unwrap();
        let snapshot = serde_json::to_value(report.snapshot()).unwrap();
        let attempt = &snapshot["attempts"][0];
        let usage = &attempt["source_usage"]["reports"];
        assert_eq!(usage.as_array().unwrap().len(), 1);
        assert_eq!(usage[0]["counts"]["input_tokens"], 3);
        assert_eq!(usage[0]["counts"]["output_tokens"], 2);
        assert!(usage[0]["counts"]["cached_input_tokens"].is_null());
        assert_eq!(attempt["source_eof"], !streaming);
        if mode != "cancel" {
            gate.flush_diagnostics().await;
            let text = std::fs::read_to_string(&path).unwrap();
            let records: Vec<serde_json::Value> = text
                .lines()
                .map(|l| serde_json::from_str(l).unwrap())
                .filter(|r: &serde_json::Value| r["event"] == "request")
                .collect();
            assert_eq!(records.len(), 1);
            assert_eq!(records[0]["request_id"], id.0.as_ref());
            assert_eq!(records[0]["agent_id"], "fixture-agent");
            let logged = &records[0]["generation"]["attempts"][0]["source_usage"];
            let mut expected = attempt["source_usage"].clone();
            // Native SDK log maps omit unknown optional counters, never fill zeros.
            expected["reports"][0]["counts"]
                .as_object_mut()
                .unwrap()
                .retain(|_, value| !value.is_null());
            assert_eq!(logged, &expected);
        }
    }
}

#[tokio::test]
async fn attempt_wait_attribution_and_hold_deadline_are_explicit() {
    let gate = gateway(Limits::default());
    let entry = gate.state.entries[&(
        crate::protocol::ClientProtocol::Chat,
        "fixture-model".into(),
    )]
        .clone();
    let request = entry.client.decode_client_request(
        br#"{"model":"fixture-model","messages":[{"role":"user","content":"hello"}],"stream":true,"stream_options":{"include_obfuscation":false}}"#,
    ).unwrap();
    for downstream in [true, false] {
        let mut candidate = entry.candidates[0].as_ref().clone();
        candidate.endpoint.execution.timeout_ms = 50;
        let (input, source, dropped, _) = upstream(200, "text/event-stream");
        input
            .send(Ok(Bytes::from(frames()[..2].concat())))
            .await
            .unwrap();
        let state = Status::default();
        let (tx, mut rx) = mpsc::channel(1);
        let mut trace = Trace::new(None);
        tokio::time::pause();
        let work = produce_chain(
            Upstreams::Observed {
                candidate: Arc::new(candidate),
                response: source,
            },
            &entry,
            &request,
            &gate.state.limits,
            Instant::now() + Duration::from_secs(60),
            &tx,
            &state,
            &mut trace,
        );
        tokio::pin!(work);
        assert!(futures_util::poll!(&mut work).is_pending());
        assert!(state.published.load(Ordering::Acquire));
        assert!(!state.waiting_upstream.load(Ordering::Acquire));
        if !downstream {
            // Acknowledgement is not a permanent Delivery stage: after every
            // handed-off frame, awaiting the next upstream chunk is upstream I/O.
            while let Ok(Message::Chunk(chunk)) = rx.try_recv() {
                chunk.ack.send(()).unwrap();
                assert!(futures_util::poll!(&mut work).is_pending());
            }
            assert!(state.waiting_upstream.load(Ordering::Acquire));
        }
        tokio::time::advance(Duration::from_millis(50)).await;
        let error = work.await.unwrap_err();
        assert_eq!(error.code(), "upstream_timeout");
        assert_eq!(error.recovery.affects_health, !downstream);
        tokio::time::resume();
        bounded(dropped).await.unwrap();
    }
    // Lifecycle-only output must be released on a real timer even if the
    // upstream stays silent; there is no fabricated terminal or keepalive.
    let mut held_entry = entry.as_ref().clone();
    held_entry.client = crate::adapter::ClientAdapter::openai(
        crate::protocol::openai::GenerationCodec::new(
            GenerationApi::Responses,
            crate::protocol::openai::Dialect::Standard,
            None,
        ),
        crate::protocol::openai::TargetContract::full(),
    );
    let candidate = &entry.candidates[0];
    let (input, source, dropped, _) = upstream(200, "text/event-stream");
    input.send(Ok(frames()[0].clone())).await.unwrap();
    let state = Status::default();
    let (tx, mut rx) = mpsc::channel(1);
    let lane = Lane {
        tx: &tx,
        state: &state,
    };
    let mut trace = Trace::new(None);
    tokio::time::pause();
    {
        let work = crate::gateway::generation::intake::produce_candidate(
            &held_entry,
            candidate,
            &request,
            source,
            &gate.state.limits,
            &lane,
            true,
            None,
            &mut trace,
        );
        tokio::pin!(work);
        assert!(futures_util::poll!(&mut work).is_pending());
        assert!(rx.try_recv().is_err());
        // Cross the timer wheel's millisecond boundary as well as the deadline.
        tokio::time::advance(Duration::from_millis(20_001)).await;
        assert!(futures_util::poll!(&mut work).is_pending());
        let Message::Chunk(chunk) = rx.try_recv().unwrap() else {
            panic!("held lifecycle frame");
        };
        assert!(!String::from_utf8_lossy(&chunk.bytes).contains("[DONE]"));
        assert!(state.published.load(Ordering::Acquire));
        assert!(!state.waiting_upstream.load(Ordering::Acquire));
    }
    tokio::time::resume();
    bounded(dropped).await.unwrap();
}

#[tokio::test]
async fn invalid_heads_and_static_overflow_fail_without_exposing_provider_data() {
    // Error bodies are read for classification; redirects and bad media are not.
    for (status, media) in [
        (302, "application/json"),
        (401, "application/json"),
        (403, "application/json"),
        (200, "text/plain"),
        (200, "text/event-stream"),
    ] {
        let gate = gateway(Limits::default());
        let (_tx, upstream, _, reads) = upstream(status, media);
        let error = bounded(response(
            &gate,
            upstream,
            false,
            Instant::now() + Duration::from_secs(1),
        ))
        .await
        .unwrap_err();
        assert_eq!(error.status, 502);
        assert_eq!(reads.load(Ordering::SeqCst), 0);
    }
    // The broad media admission succeeds; the decoder rejects this charset.
    // Keep its typed failure instead of only the public upstream presentation.
    {
        let gate = gateway(Limits::default());
        let (_tx, upstream, _, reads) = upstream(200, "text/event-stream; charset=iso-8859-1");
        let entry = gate.state.entries[&(
            crate::protocol::ClientProtocol::Chat,
            "fixture-model".into(),
        )]
            .clone();
        let request=entry.client.as_openai().unwrap().0.decode_request(br#"{"model":"fixture-model","messages":[{"role":"user","content":"hello"}],"stream":true}"#).unwrap();
        let candidate = entry.candidates[0].clone();
        let trace = Trace::generation_trace(None);
        let report = trace.report.clone();
        let error = bounded(respond(
            entry,
            candidate,
            request,
            upstream,
            &gate.state.limits,
            Instant::now() + Duration::from_secs(1),
            gate.state.shutdown.clone(),
            gate.state.permits.clone().acquire_owned().await.unwrap(),
            trace,
        ))
        .await
        .unwrap_err();
        assert_eq!(error.status, 502);
        assert_eq!(reads.load(Ordering::SeqCst), 0);
        let snapshot = report.snapshot();
        assert_eq!(snapshot.failure_code, Some(error.diagnostic_code()));
        assert_eq!(snapshot.failure_status_hint, Some(502));
        assert_eq!(
            snapshot.result,
            crate::execution::report::ResultState::Failed
        );
        let failure = snapshot.attempts[0].failure.as_ref().unwrap();
        assert!(matches!(
            failure.cause,
            crate::execution::AttemptError::Sse(crate::protocol::stream::SseError::Http)
        ));
        assert_eq!(
            failure.boundary,
            Some(crate::protocol::conversion::generation::ConversionBoundary::EventDecode)
        );
    }
    let gate = gateway(Limits {
        // Allow both startup discovery views while keeping the upstream body over budget.
        response_bytes: 4096,
        ..Limits::default()
    });
    let (tx, upstream, _, _) = upstream(200, "application/json");
    tx.send(Ok(Bytes::from(
        "synthetic-private-upstream-detail".repeat(256),
    )))
    .await
    .unwrap();
    drop(tx);
    assert_eq!(
        bounded(response(
            &gate,
            upstream,
            false,
            Instant::now() + Duration::from_secs(1)
        ))
        .await
        .unwrap_err()
        .status,
        502
    );
}
