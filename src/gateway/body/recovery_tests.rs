//! Same-target recovery through real HTTP, with a controlled response body and clock.
use super::*;
use axum::{Router, http::StatusCode, response::IntoResponse, routing::post};
use serde_json::Value;

#[tokio::test]
async fn recovery_clock_stops_at_content_not_headers_or_lifecycle_and_keeps_health() {
    for mode in [
        "content",
        "silent",
        "lifecycle",
        "deadline",
        "quota",
        "malformed",
        "cancel",
        "twice",
        "zero",
        "hint",
        "longhint",
        "dayhint",
        "datehint",
        "overflowhint",
        "inband",
        "inbandhint",
        "inbandlonghint",
        "empty",
    ] {
        let retry_after = match mode {
            "hint" | "inbandhint" => Some("1".to_owned()),
            "longhint" | "inbandlonghint" => Some("2".to_owned()),
            "dayhint" => Some("86401".to_owned()),
            "datehint" => Some(httpdate::fmt_http_date(
                std::time::SystemTime::now() + Duration::from_secs(172800),
            )),
            "overflowhint" => Some("18446744073709551616".to_owned()),
            _ => None,
        };
        let no_retry = matches!(
            mode,
            "zero" | "longhint" | "dayhint" | "datehint" | "overflowhint" | "inbandlonghint"
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let (calls_tx, mut calls_rx) = mpsc::channel(4);
        let (wire_tx, wire_rx) = mpsc::channel::<Result<Bytes, std::io::Error>>(4);
        let wire_rx = Arc::new(Mutex::new(Some(wire_rx)));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let observed = requests.clone();
        let router = Router::new().route(
            "/chat/completions",
            post(move |headers: axum::http::HeaderMap, body: Bytes| {
                let calls = calls_tx.clone();
                let wire = wire_rx.clone();
                let requests = observed.clone();
                let retry_after = retry_after.clone();
                async move {
                    let ordinal = {
                        let mut requests = requests.lock().unwrap();
                        requests.push((headers["authorization"].clone(), body));
                        requests.len()
                    };
                    calls.send(ordinal).await.unwrap();
                    if ordinal == 1 && matches!(mode, "inband" | "inbandhint" | "inbandlonghint") {
                        let mut response = ([("content-type", "text/event-stream")],
                            "data: {\"id\":\"discarded-id\",\"object\":\"chat.completion.chunk\",\"created\":1,\"model\":\"fixture-model\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\"},\"finish_reason\":null}]}\n\ndata: {\"error\":{\"code\":503,\"message\":\"synthetic\"}}\n\n").into_response();
                        if let Some(hint) = retry_after {
                            response.headers_mut().insert("retry-after", hint.parse().unwrap());
                        }
                        return response;
                    }
                    if ordinal == 1 || mode == "twice" {
                        if let Some(hint) = retry_after {
                            return (
                                StatusCode::SERVICE_UNAVAILABLE,
                                [("retry-after", hint)],
                            )
                                .into_response();
                        }
                        return StatusCode::SERVICE_UNAVAILABLE.into_response();
                    }
                    assert_eq!(ordinal, 2, "no hidden third send");
                    if mode == "quota" {
                        return StatusCode::PAYMENT_REQUIRED.into_response();
                    }
                    if mode == "malformed" {
                        return ([("content-type", "text/html")], "invalid").into_response();
                    }
                    let rx = wire.lock().unwrap().take().unwrap();
                    let stream = futures_util::stream::unfold(rx, |mut rx| async move {
                        rx.recv().await.map(|chunk| (chunk, rx))
                    });
                    (
                        [("content-type", "text/event-stream")],
                        Body::from_stream(stream),
                    )
                        .into_response()
                }
            }),
        );
        let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        let mut limits = Limits {
            force_upstream_stream: true,
            ..Default::default()
        };
        let mut gate = gateway(limits.clone());
        let entry = gate.state.entries[&(ClientProtocol::Chat, "fixture-model".into())].clone();
        let provider = entry.candidates[0].provider.id.as_str().to_owned();
        limits.provider_recovery.insert(
            provider,
            crate::provider::RecoveryPolicy {
                recovery_timeout_ms: match mode {
                    "zero" => 0,
                    "hint" | "inbandhint" => 2000,
                    _ => 1000,
                },
                ..Default::default()
            },
        );
        // Reconstruct with the same policy owner used by production bootstrap.
        gate = gateway(limits.clone());
        Arc::get_mut(&mut gate.state).unwrap().transport =
            crate::transport::http::HttpTransport::with_environment_proxy(None, false)
                .unwrap()
                .with_loopback_authority(&origin);
        let mut entry = gate.state.entries[&(ClientProtocol::Chat, "fixture-model".into())]
            .as_ref()
            .clone();
        entry.candidates = entry
            .candidates
            .iter()
            .map(|candidate| {
                let mut candidate = candidate.as_ref().clone();
                candidate.endpoint.execution.timeout_ms = 120_000;
                Arc::new(candidate)
            })
            .collect();
        let request = entry
            .client
            .decode_client_request(
                br#"{"model":"fixture-model","messages":[{"role":"user","content":"hello"}]}"#,
            )
            .unwrap();
        let candidates = entry.eligible(&request, &gate.state.affinity).unwrap();
        let state = Status::default();
        let (tx, mut rx) = mpsc::channel(1);
        let mut trace = Trace::new(None);
        let deadline =
            Instant::now() + Duration::from_secs(if mode == "deadline" { 1 } else { 120 });
        let result;
        {
            let lane = Lane {
                tx: &tx,
                state: &state,
            };
            let producer = produce_chain(
                Upstreams::Http {
                    runtime: gate.state.clone(),
                    candidates,
                    history: None,
                    binding: None,
                },
                &entry,
                &request,
                &limits,
                deadline,
                &lane,
                &mut trace,
            );
            tokio::pin!(producer);
            assert_eq!(
                tokio::select! { result = &mut producer => panic!("early result: {:?}", result.err()), call = calls_rx.recv() => call },
                Some(1)
            );
            if !no_retry {
                assert_eq!(
                    tokio::select! { result = &mut producer => panic!("early result: {:?}", result.err()), call = calls_rx.recv() => call },
                    Some(2)
                );
                if matches!(mode, "hint" | "inbandhint") {
                    let observed_at = state.failures.lock().unwrap().previous[0].at;
                    assert!(
                        observed_at.elapsed() >= Duration::from_secs(1),
                        "hint waiting starts at failure observation, not request startup"
                    );
                }
            }
            if no_retry || matches!(mode, "quota" | "malformed" | "twice") {
                result = Some(producer.await);
            } else if matches!(mode, "hint" | "inband" | "inbandhint") {
                // The second send starts after the exact hint, inside the same budget.
                wire_tx
                    .send(Ok([frames().concat(), b"data: [DONE]\n\n".to_vec()]
                        .concat()
                        .into()))
                    .await
                    .unwrap();
                drop(wire_tx);
                let chunk = tokio::select! {
                    result = &mut producer => panic!("handoff required: {:?}", result.err()),
                    chunk = rx.recv() => chunk.unwrap(),
                };
                let Message::Chunk(chunk) = chunk else {
                    panic!("expected JSON")
                };
                assert!(!String::from_utf8_lossy(&chunk.bytes).contains("discarded-id"));
                chunk.ack.send(()).unwrap();
                result = Some(producer.await);
            } else if mode == "cancel" {
                // Drop the in-flight second attempt; the first 503 must survive.
                assert!(futures_util::poll!(&mut producer).is_pending());
                result = None;
            } else {
                if mode == "lifecycle" {
                    let opening = wire::events(2)[0].clone();
                    wire_tx
                        .send(Ok(Bytes::from(format!("data: {opening}\n\n"))))
                        .await
                        .unwrap();
                }
                if matches!(mode, "content" | "empty") {
                    let events = frames();
                    let prefix = if mode == "empty" {
                        [
                            events[0].clone(),
                            events[3].clone(),
                            events[4].clone(),
                            Bytes::from_static(b"data: [DONE]\n\n"),
                        ]
                        .concat()
                    } else {
                        events[..3].concat()
                    };
                    wire_tx.send(Ok(prefix.into())).await.unwrap();
                    tokio::select! {
                        result = &mut producer => panic!("content cannot finalize JSON: {:?}", result.err()),
                        _ = state.readiness.notified() => {}
                    }
                    assert!(state.recovery_ready.load(Ordering::Acquire));
                    assert!(
                        !state.published.load(Ordering::Acquire),
                        "buffered JSON stays private"
                    );
                    tokio::time::pause();
                    tokio::time::advance(Duration::from_secs(31)).await;
                    assert!(
                        futures_util::poll!(&mut producer).is_pending(),
                        "content removes only the recovery clock"
                    );
                    tokio::time::resume();
                    if mode == "content" {
                        wire_tx.send(Ok(events[3..].concat().into())).await.unwrap();
                        wire_tx
                            .send(Ok(Bytes::from_static(b"data: [DONE]\n\n")))
                            .await
                            .unwrap();
                    }
                    drop(wire_tx);
                    let chunk = tokio::select! {
                        result = &mut producer => panic!("handoff required: {:?}", result.err()),
                        chunk = rx.recv() => chunk.unwrap(),
                    };
                    let Message::Chunk(chunk) = chunk else {
                        panic!("expected final JSON")
                    };
                    let value: Value = serde_json::from_slice(&chunk.bytes).unwrap();
                    if mode == "content" {
                        assert!(
                            value["choices"][0]["message"]["content"]
                                .as_str()
                                .unwrap()
                                .contains('🧪')
                        );
                    }
                    chunk.ack.send(()).unwrap();
                    result = Some(producer.await);
                } else {
                    assert!(futures_util::poll!(&mut producer).is_pending());
                    tokio::time::pause();
                    tokio::time::advance(Duration::from_secs(2)).await;
                    result = Some(producer.await);
                    tokio::time::resume();
                }
            }
        }
        if let Some(result) = result {
            if matches!(mode, "content" | "hint" | "inband" | "inbandhint" | "empty") {
                result.unwrap();
            } else {
                let error = result.unwrap_err();
                assert_eq!(
                    error.status.as_u16(),
                    match mode {
                        "quota" => 429,
                        "malformed" => 502,
                        _ if no_retry || mode == "twice" => 503,
                        _ => 504,
                    },
                    "{mode}"
                );
            }
        }
        {
            let requests = requests.lock().unwrap();
            assert_eq!(requests.len(), if no_retry { 1 } else { 2 }, "{mode}");
            if requests.len() == 2 {
                assert_eq!(requests[0], requests[1], "same credential and exact body");
            }
        }
        let candidate = &entry.candidates[0];
        let probe = gate.state.executors.acquire(candidate, Instant::now());
        if matches!(mode, "content" | "hint" | "inband" | "inbandhint" | "empty") {
            assert!(
                !probe.unwrap().probing,
                "recovery success discards the first penalty"
            );
        } else if matches!(mode, "silent" | "lifecycle") {
            assert!(
                probe.unwrap().probing,
                "expired first failure remains probe-gated: {mode}"
            );
        } else {
            assert!(probe.is_err(), "first failure remains cooling: {mode}");
        }
        gate.shutdown();
        server.abort();
        let _ = server.await;
    }
}
