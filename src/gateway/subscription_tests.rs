//! SSE-only upstream delivery is independent of client JSON/SSE selection.
use super::*;
use crate::protocol::openai::Profile;
use axum::body::to_bytes;
use serde_json::json;
use std::time::Duration;
#[allow(dead_code)]
#[path = "../../tests/support/responses_profile.rs"]
mod wire;

#[test]
fn siwc_public_admission_does_not_invent_an_unsupported_output_token_limit() {
    let mut gateway = tests::gateway(Limits::default());
    let original = gateway.state.entries[&(family(Profile::Chat), "fixture-model".into())].clone();
    let mut public = original.public.clone();
    public.contract.max_output_tokens = false;
    let entry = Arc::new(BoundEntry {
        public,
        client: Adapter::new(Profile::Responses, crate::adapter::Dialect::Standard, None),
        downstream: crate::lowering::generation::GenerationRepresentationContract::full(),
        policy: original.policy.clone(),
        candidates: original.candidates.clone(),
    });
    Arc::get_mut(&mut gateway.state)
        .unwrap()
        .entries
        .insert((family(Profile::Responses), "fixture-model".into()), entry);
    let (_, request) = admission::prepare(
        &gateway.state,
        Profile::Responses,
        br#"{"model":"fixture-model","input":"hello"}"#,
    )
    .unwrap();
    assert_eq!(request.task.semantic.controls().max_output_tokens, None);
    assert!(
        admission::prepare(
            &gateway.state,
            Profile::Responses,
            br#"{"model":"fixture-model","input":"hello","max_output_tokens":8}"#
        )
        .is_err()
    );
    let adapter = Adapter::new(Profile::Responses, crate::adapter::Dialect::Siwc, None);
    let wire = adapter
        .encode_request(
            &request,
            "selected-slug",
            &crate::lowering::generation::GenerationRepresentationContract::full(),
        )
        .unwrap();
    assert!(wire.get("max_output_tokens").is_none());
    assert_eq!(wire["store"], false);
    assert_eq!(wire["stream"], true);
}

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
    dialect: crate::adapter::Dialect,
    media: &[&[u8]],
    bytes: Vec<u8>,
    stream: bool,
) -> Option<Vec<u8>> {
    use futures_util::StreamExt;
    let gate = tests::gateway(Limits::default());
    let original = gate.state.entries[&(family(Profile::Chat), "fixture-model".into())].clone();
    let base = &original.candidates[0];
    let adapter = Adapter::new(Profile::Responses, dialect, None);
    let mut endpoint = base.endpoint.clone();
    endpoint.protocol = crate::topology::ProtocolProfile::OpenAiResponses;
    endpoint.representation =
        adapter.contract(&crate::lowering::generation::GenerationRepresentationContract::full());
    let candidate = Arc::new(BoundCandidate {
        endpoint,
        provider: base.provider.clone(),
        secret: base.secret.clone(),
        credential_fallback: false,
    });
    let entry = Arc::new(BoundEntry {
        public: original.public.clone(),
        client: Adapter::new(
            Profile::Responses,
            crate::adapter::Dialect::MorphieCore,
            None,
        ),
        downstream: crate::lowering::generation::GenerationRepresentationContract::full(),
        policy: original.policy.clone(),
        candidates: vec![candidate.clone()],
    });
    let request = entry.client.decode_request(json!({"model":"fixture-model","input":"hello","stream":stream,"stream_options":if stream {json!({"include_obfuscation":false})} else {serde_json::Value::Null}}).to_string().as_bytes()).unwrap();
    let mut builder = axum::http::Response::builder().status(200);
    for media in media {
        builder = builder.header(
            "content-type",
            axum::http::HeaderValue::from_bytes(media).unwrap(),
        );
    }
    let upstream = reqwest::Response::from(builder.body(reqwest::Body::from(bytes)).unwrap());
    tokio::time::timeout(Duration::from_secs(4), async {
        let result = body::respond_source(
            entry,
            request,
            exchange::Upstreams::Observed {
                candidate,
                response: upstream,
            },
            gate.state.limits.clone(),
            tokio::time::Instant::now() + Duration::from_secs(3),
            gate.state.shutdown.clone(),
            gate.state.permits.clone().acquire_owned().await.unwrap(),
            diagnostics::Trace::new(None, &axum::http::HeaderMap::new()),
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
    .expect("bounded synthetic response consumption")
}

#[tokio::test]
async fn forced_sse_projects_json_and_rejects_conflicting_or_invalid_media() {
    for dialect in [
        crate::adapter::Dialect::Codex,
        crate::adapter::Dialect::Siwc,
    ] {
        for (media, accepted) in [
            (vec![], true),
            (vec![b"text/event-stream".as_slice()], true),
            (vec![b"text/event-stream; charset=utf-8".as_slice()], true),
            (vec![b"application/json".as_slice()], false),
            (vec![b"text/html".as_slice()], false),
            (vec![b"".as_slice()], false),
            (vec![b"invalid".as_slice()], false),
            (vec![b"\xff".as_slice()], false),
            (
                vec![
                    b"text/event-stream".as_slice(),
                    b"application/json".as_slice(),
                ],
                false,
            ),
            (
                vec![
                    b"text/event-stream".as_slice(),
                    b"text/event-stream".as_slice(),
                ],
                false,
            ),
        ] {
            for stream in [false, true] {
                let result = consume(dialect, &media, sse(true), stream).await;
                assert_eq!(
                    result.is_some(),
                    accepted,
                    "{dialect:?} {media:?} stream={stream}"
                );
                if let Some(bytes) = result {
                    if stream {
                        assert!(
                            String::from_utf8(bytes)
                                .unwrap()
                                .contains("response.completed")
                        );
                    } else {
                        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                        assert_eq!(value["status"], "completed");
                        assert_eq!(value["output"][0]["content"][0]["text"], "{\"ok\":false}");
                    }
                }
            }
        }
    }
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
    for dialect in [
        crate::adapter::Dialect::Codex,
        crate::adapter::Dialect::Siwc,
    ] {
        for (index, bytes) in cases.iter().enumerate() {
            for stream in [false, true] {
                assert!(
                    consume(dialect, &[], bytes.clone(), stream).await.is_none(),
                    "{dialect:?} case={index} stream={stream}"
                );
            }
        }
    }
}

#[tokio::test]
async fn missing_media_is_not_enabled_for_standard_or_other_responses_profiles() {
    for dialect in [
        crate::adapter::Dialect::Standard,
        crate::adapter::Dialect::MorphieCore,
        crate::adapter::Dialect::Grok,
    ] {
        for stream in [false, true] {
            assert!(consume(dialect, &[], sse(true), stream).await.is_none());
        }
        assert!(
            consume(dialect, &[b"text/event-stream"], sse(true), true)
                .await
                .is_some()
        );
    }
    assert!(
        !Adapter::new(Profile::Chat, crate::adapter::Dialect::Standard, None)
            .adaptation
            .rules
            .responses_sse_without_content_type
    );
}
