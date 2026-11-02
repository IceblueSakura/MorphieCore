//! SSE-only upstream delivery is independent of client JSON/SSE selection.
use super::responses_wire as wire;
use crate::{
    gateway::{generation::exchange, *},
    protocol::openai::GenerationApi,
};
use axum::body::to_bytes;
use serde_json::json;
use std::time::Duration;

fn sse(complete: bool) -> Vec<u8> {
    let mut events = wire::events(2);
    if !complete {
        events.pop();
    }
    events
        .iter()
        .map(|event| {
            format!(
                "event: {}\ndata: {event}\n\n",
                event["type"].as_str().unwrap()
            )
        })
        .collect::<String>()
        .into_bytes()
}

async fn consume(
    dialect: crate::protocol::openai::Dialect,
    media: &[&[u8]],
    bytes: Vec<u8>,
    stream: bool,
    eligible: bool,
) -> Option<Vec<u8>> {
    use futures_util::StreamExt;
    let gate = crate::gateway::tests::support::gateway(Limits::default());
    let original = gate.state.entries[&(
        crate::protocol::ClientProtocol::Chat,
        "fixture-model".into(),
    )]
        .clone();
    let base = &original.candidates[0];
    let adapter = GenerationCodec::new(GenerationApi::Responses, dialect, None);
    let mut endpoint = base.endpoint.clone();
    endpoint.protocol = crate::topology::GenerationProtocol::OpenAiResponses;
    endpoint.representation = adapter
        .contract(&crate::protocol::openai::TargetContract::full())
        .into();
    let candidate = Arc::new(BoundCandidate {
        endpoint,
        provider: base.provider.clone(),
        secret: base.secret.clone(),
        account: None,
        health: base.health.clone(),
        cache_scope: base.cache_scope.clone(),
    });
    let entry = Arc::new(BoundEntry {
        public: original.public.clone(),
        client: crate::adapter::ClientAdapter::openai(
            GenerationCodec::new(
                GenerationApi::Responses,
                crate::protocol::openai::Dialect::MorphieCore,
                None,
            ),
            crate::protocol::openai::TargetContract::full(),
        ),
        policy: original.policy.clone(),
        candidates: vec![candidate.clone()],
    });
    let mut request = entry.client.as_openai().unwrap().0.decode_request(json!({"model":"fixture-model","input":"hello","stream":stream,"stream_options":if stream {json!({"include_obfuscation":false})} else {serde_json::Value::Null}}).to_string().as_bytes()).unwrap();
    request.conversation = Some(
        crate::semantic::context::ConversationContext::conversation("synthetic-subscription")
            .unwrap(),
    );
    let mut headers = axum::http::HeaderMap::new();
    headers.insert(
        "authorization",
        format!("Bearer {}", super::support::KEY).parse().unwrap(),
    );
    let agent = gate.state.auth.authenticate(&headers).unwrap();
    let mut http = axum::extract::Request::new(axum::body::Body::empty());
    http.extensions_mut().insert(agent);
    let trace = diagnostics::Trace::for_request(None, &http);
    let earlier = tokio::time::Instant::now() - Duration::from_secs(2);
    let prior = gate.state.executors.acquire(&candidate, earlier).unwrap();
    prior
        .failed(ApiError::failure(503, b"", Some(1)), earlier)
        .unwrap();
    drop(prior);
    let mut builder = axum::http::Response::builder().status(200);
    for media in media {
        builder = builder.header(
            "content-type",
            axum::http::HeaderValue::from_bytes(media).unwrap(),
        );
    }
    let upstream = reqwest::Response::from(builder.body(reqwest::Body::from(bytes)).unwrap());
    let delivered = tokio::time::timeout(Duration::from_secs(4), async {
        let result = exchange::respond_source(
            entry,
            request,
            exchange::Upstreams::Observed {
                runtime: Some(gate.state.clone()),
                candidate: candidate.clone(),
                response: upstream,
            },
            gate.state.limits.clone(),
            tokio::time::Instant::now() + Duration::from_secs(3),
            gate.state.shutdown.clone(),
            gate.state.permits.clone().acquire_owned().await.unwrap(),
            trace,
        )
        .await;
        let response = result.ok()?;
        assert_eq!(
            response.headers()["content-type"],
            if stream {
                "text/event-stream"
            } else {
                "application/json"
            }
        );
        if !stream {
            return to_bytes(response.into_body(), 1 << 20)
                .await
                .ok()
                .map(|b| b.to_vec());
        }
        let mut body = response.into_body().into_data_stream();
        let mut delivered = Vec::new();
        while let Some(chunk) = body.next().await {
            match chunk {
                Ok(chunk) => {
                    assert!(delivered.len() + chunk.len() <= 1 << 20);
                    delivered.extend_from_slice(&chunk);
                }
                Err(_) => {
                    assert!(!String::from_utf8_lossy(&delivered).contains("response.completed"));
                    return None;
                }
            }
        }
        Some(delivered)
    })
    .await
    .expect("bounded synthetic response consumption");
    if delivered.is_some() {
        let later = tokio::time::Instant::now() + Duration::from_secs(3600);
        let next = gate.state.executors.acquire(&candidate, later).unwrap();
        assert_eq!(
            next.probing, !eligible,
            "only validated successful tasks recover health"
        );
        assert!(next.completed(None, later).0);
        drop(next);
        let mut choices = [candidate];
        let now = tokio::time::Instant::now();
        let binding = gate.state.executors.observe(
            Some("fixture-agent"),
            "fixture-model",
            Some("synthetic-subscription"),
            now,
        );
        let order = gate
            .state
            .executors
            .order(binding.as_ref(), &mut choices, now);
        assert_eq!(
            order == "affinity",
            eligible,
            "failed and non-limit incomplete tasks never commit affinity"
        );
    }
    delivered
}

#[tokio::test]
async fn missing_media_does_not_relax_sse_terminal_or_eof_validation() {
    let complete = sse(true);
    let mut truncated = complete.clone();
    truncated.pop();
    let mut trailing = complete.clone();
    trailing.extend_from_slice(b"data: {\"type\":\"unexpected\"}\n\n");
    let mut invalid_json = sse(false);
    invalid_json.extend_from_slice(b"data: not-json\n\n");
    let mut done_then_trailing = complete.clone();
    done_then_trailing.extend_from_slice(b"data: [DONE]\n\ndata: {}\n\n");
    let cases = [
        sse(false),
        truncated,
        trailing,
        invalid_json,
        done_then_trailing,
        b"<html>not a response</html>".to_vec(),
        serde_json::to_vec(&wire::response(2)).unwrap(),
    ];
    let dialect = crate::protocol::openai::Dialect::Siwc;
    for (index, bytes) in cases.iter().enumerate() {
        for stream in [false, true] {
            assert!(
                consume(dialect, &[], bytes.clone(), stream, false)
                    .await
                    .is_none(),
                "{dialect:?} case={index} stream={stream}"
            );
        }
    }
}

#[tokio::test]
async fn subscription_sse_materializes_json_or_sse_without_losing_calls_or_faking_success() {
    use crate::protocol::openai::Dialect;
    for dialect in [Dialect::Grok, Dialect::Siwc] {
        for stream in [false, true] {
            for (outcome, incomplete, eligible) in [
                ("completed", None, true),
                ("incomplete", Some("max_output_tokens"), true),
                ("failed", None, false),
                ("incomplete", Some("content_filter"), false),
            ] {
                let mut response = wire::subscription_response(dialect == Dialect::Siwc);
                let mut events = wire::response_events(response.clone());
                response["status"] = json!(outcome);
                if outcome == "failed" {
                    response["error"] =
                        json!({"code":"rate_limit_exceeded","message":"synthetic limit"});
                }
                if outcome == "incomplete" {
                    response["incomplete_details"] = json!({"reason":incomplete.unwrap()});
                }
                let last = events.last_mut().unwrap();
                last["type"] = json!(format!("response.{outcome}"));
                last["response"] = response.clone();
                let bytes = events
                    .iter()
                    .map(|e| format!("event: {}\ndata: {e}\n\n", e["type"].as_str().unwrap()))
                    .collect::<String>()
                    .into_bytes();
                let delivered = consume(
                    dialect,
                    &[b"text/event-stream"],
                    bytes.clone(),
                    stream,
                    eligible,
                )
                .await
                .unwrap_or_else(|| panic!("{dialect:?} stream={stream} outcome={outcome}"));
                let terminal = if stream {
                    let frames = String::from_utf8(delivered).unwrap();
                    let values = frames
                        .lines()
                        .filter_map(|l| l.strip_prefix("data: "))
                        .map(|l| serde_json::from_str::<serde_json::Value>(l).unwrap())
                        .collect::<Vec<_>>();
                    assert_eq!(
                        values.last().unwrap()["type"],
                        format!("response.{outcome}")
                    );
                    if outcome != "completed" {
                        assert!(!values.iter().any(|e| e["type"] == "response.completed"));
                    }
                    values.last().unwrap()["response"].clone()
                } else {
                    serde_json::from_slice(&delivered).unwrap()
                };
                assert_eq!(terminal["status"], outcome);
                assert_eq!(terminal["output"][1]["call_id"], "c");
                assert_eq!(
                    terminal["output"][1]["arguments"],
                    "{ \"n\":9007199254740993 }"
                );
                assert_eq!(terminal["output"][2]["phase"], "commentary");
                if dialect == Dialect::Siwc {
                    assert!(
                        consume(dialect, &[], bytes, stream, eligible)
                            .await
                            .is_some()
                    );
                }
            }
        }
    }
}

#[tokio::test]
async fn missing_media_is_not_enabled_for_standard_or_other_responses_profiles() {
    for dialect in [
        crate::protocol::openai::Dialect::Standard,
        crate::protocol::openai::Dialect::MorphieCore,
        crate::protocol::openai::Dialect::Grok,
    ] {
        for stream in [false, true] {
            assert!(
                consume(dialect, &[], sse(true), stream, false)
                    .await
                    .is_none()
            );
        }
        assert!(
            consume(dialect, &[b"text/event-stream"], sse(true), true, true)
                .await
                .is_some()
        );
    }
    assert!(
        !GenerationCodec::new(
            GenerationApi::Chat,
            crate::protocol::openai::Dialect::Standard,
            None
        )
        .adaptation
        .rules
        .responses_sse_without_content_type
    );
}
