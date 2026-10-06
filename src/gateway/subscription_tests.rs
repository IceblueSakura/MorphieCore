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
    let original = gateway.state.entries[&(family(Profile::Chat), "deepseek-flash".into())].clone();
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
        .insert((family(Profile::Responses), "deepseek-flash".into()), entry);
    let (_, request) = admission::prepare(
        &gateway.state,
        Profile::Responses,
        br#"{"model":"deepseek-flash","input":"hello"}"#,
    )
    .unwrap();
    assert_eq!(request.task.semantic.controls().max_output_tokens, None);
    assert!(
        admission::prepare(
            &gateway.state,
            Profile::Responses,
            br#"{"model":"deepseek-flash","input":"hello","max_output_tokens":8}"#
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

#[tokio::test]
async fn forced_sse_projects_json_and_rejects_missing_terminal_or_conflicting_media() {
    for (dialect, missing_media) in [
        (crate::adapter::Dialect::Codex, true),
        (crate::adapter::Dialect::Siwc, false),
    ] {
        for (media, complete, accepted) in [
            (None, true, true),
            (Some("text/event-stream"), true, true),
            (Some("application/json"), true, false),
            (None, false, false),
        ] {
            let accepted = accepted && (media.is_some() || missing_media);
            for stream in [false, true] {
                let gate = tests::gateway(Limits::default());
                let original =
                    gate.state.entries[&(family(Profile::Chat), "deepseek-flash".into())].clone();
                let base = &original.candidates[0];
                let adapter = Adapter::new(Profile::Responses, dialect, None);
                let mut endpoint = base.endpoint.clone();
                endpoint.protocol = crate::topology::ProtocolProfile::OpenAiResponses;
                endpoint.representation = adapter.contract(
                    &crate::lowering::generation::GenerationRepresentationContract::full(),
                );
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
                    downstream: crate::lowering::generation::GenerationRepresentationContract::full(
                    ),
                    policy: original.policy.clone(),
                    candidates: vec![candidate.clone()],
                });
                let request = entry.client.decode_request(json!({"model":"deepseek-flash","input":"hello","stream":stream,"stream_options":if stream {json!({"include_obfuscation":false})} else {serde_json::Value::Null}}).to_string().as_bytes()).unwrap();
                let mut events = wire::events(2);
                if !complete {
                    events.pop();
                }
                let bytes: String = events
                    .iter()
                    .map(|event| {
                        format!(
                            "event: {}\ndata: {event}\n\n",
                            event["type"].as_str().unwrap()
                        )
                    })
                    .collect();
                let mut builder = axum::http::Response::builder().status(200);
                if let Some(media) = media {
                    builder = builder.header("content-type", media);
                }
                let upstream =
                    reqwest::Response::from(builder.body(reqwest::Body::from(bytes)).unwrap());
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
                let consumed = match result {
                    Ok(response) => to_bytes(response.into_body(), 1 << 20).await.ok(),
                    Err(_) => None,
                };
                assert_eq!(
                    consumed.is_some(),
                    accepted,
                    "dialect={dialect:?} media={media:?} complete={complete} stream={stream}"
                );
                if let Some(bytes) = consumed {
                    if stream {
                        assert!(
                            std::str::from_utf8(&bytes)
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
