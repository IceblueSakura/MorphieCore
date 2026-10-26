//! Published tool calls prevent replay; transport EOF owns success and stored publication.
use crate::openai_tool_loop as fixture;
use morphiecore::protocol::openai::GenerationApi;
use serde_json::{Value, json};

#[tokio::test]
async fn tool_publication_withholds_success_until_eof_and_never_replays() {
    use std::time::Duration;
    for profile in [GenerationApi::Chat, GenerationApi::Responses] {
        let chat = profile == GenerationApi::Chat;
        for mode in ["hold", "cut", "cancel", "terminal-cut"] {
            let fixture = fixture::start(profile, true).await;
            let client = reqwest::Client::builder()
                .no_proxy()
                .timeout(Duration::from_secs(5))
                .build()
                .unwrap();
            let function = json!({"name":"lookup","description":"Synthetic lookup","strict":false,
                "parameters":{"type":"object","properties":{"key":{"type":"string"}},"required":["key"]}});
            let body = if chat {
                json!({"model":"qwen3.8-flash","messages":[{"role":"user","content":mode}],
                "max_completion_tokens":256,"reasoning_effort":"low","stream":true,
                "tools":[{"type":"function","function":function}]})
            } else {
                json!({"model":"gpt-6-luna","input":mode,"max_output_tokens":256,
                    "reasoning":{"effort":"low"},"stream":true,"store":true,
                    "tools":[{"type":"function","name":"lookup","description":"Synthetic lookup","strict":false,
                        "parameters":{"type":"object","properties":{"key":{"type":"string"}},"required":["key"]}}]})
            };
            let mut response = client
                .post(format!(
                    "{}/{}",
                    fixture.base,
                    if chat {
                        "chat/completions"
                    } else {
                        "responses"
                    }
                ))
                .bearer_auth(fixture::CLIENT_KEY)
                .json(&body)
                .send()
                .await
                .unwrap();
            assert_eq!(response.status(), 200);
            let mut received = Vec::new();
            while !String::from_utf8_lossy(&received).contains("call-lookup") {
                let chunk = tokio::time::timeout(Duration::from_secs(3), response.chunk())
                    .await
                    .unwrap()
                    .unwrap()
                    .unwrap();
                received.extend_from_slice(&chunk);
            }
            let mut response_id = None;
            if !chat {
                for line in String::from_utf8_lossy(&received)
                    .lines()
                    .filter_map(|l| l.strip_prefix("data: "))
                {
                    if let Ok(event) = serde_json::from_str::<Value>(line)
                        && event["type"] == "response.created"
                    {
                        response_id = event["response"]["id"].as_str().map(str::to_owned);
                    }
                }
                assert!(response_id.is_some());
            }
            if mode == "cancel" {
                drop(response);
            } else {
                // Drain any remaining nonterminal frames. A protocol terminal alone
                // cannot release success while the upstream HTTP body remains open.
                loop {
                    match tokio::time::timeout(Duration::from_millis(100), response.chunk()).await {
                        Ok(Ok(Some(chunk))) => received.extend_from_slice(&chunk),
                        Err(_) => break,
                        other => panic!("premature body end: {other:?}"),
                    }
                }
                let early = String::from_utf8_lossy(&received);
                assert!(!early.contains("[DONE]") && !early.contains("response.completed"));
                // Withholding [DONE] is insufficient if a Chat finish reason escaped.
                for data in early.lines().filter_map(|l| l.strip_prefix("data: ")) {
                    if let Ok(event) = serde_json::from_str::<Value>(data)
                        && let Some(choices) = event["choices"].as_array()
                    {
                        assert!(choices.iter().all(|c| c["finish_reason"].is_null()));
                    }
                }
                if let Some(id) = &response_id {
                    assert_eq!(
                        client
                            .get(format!("{}/responses/{id}", fixture.base))
                            .bearer_auth(fixture::CLIENT_KEY)
                            .send()
                            .await
                            .unwrap()
                            .status(),
                        404
                    );
                }
                fixture.control.release.notify_one();
                let mut failed = false;
                loop {
                    match response.chunk().await {
                        Ok(Some(chunk)) => received.extend_from_slice(&chunk),
                        Ok(None) => break,
                        Err(_) => {
                            failed = true;
                            break;
                        }
                    }
                }
                assert_eq!(failed, matches!(mode, "cut" | "terminal-cut"));
                let body = String::from_utf8_lossy(&received);
                assert_eq!(
                    body.contains(if chat { "[DONE]" } else { "response.completed" }),
                    mode == "hold"
                );
            }
            tokio::time::timeout(Duration::from_secs(3), fixture.control.dropped.notified())
                .await
                .unwrap();
            assert_eq!(
                fixture.seen.lock().unwrap().len(),
                1,
                "an eligible secondary must not receive a replay after publication"
            );
            if let Some(id) = response_id {
                assert_eq!(
                    client
                        .get(format!("{}/responses/{id}", fixture.base))
                        .bearer_auth(fixture::CLIENT_KEY)
                        .send()
                        .await
                        .unwrap()
                        .status(),
                    if mode == "hold" { 200 } else { 404 }
                );
            }
        }
    }
}
struct UpstreamDrop(std::sync::Arc<tokio::sync::Notify>);
impl Drop for UpstreamDrop {
    fn drop(&mut self) {
        self.0.notify_one();
    }
}

#[tokio::test]
async fn stalled_consumer_holds_capacity_then_cancellation_releases_one_permit() {
    use axum::{
        Router,
        body::{Body, Bytes},
        response::Response,
        routing::post,
    };
    use futures_util::StreamExt;
    use std::{
        convert::Infallible,
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
        time::Duration,
    };
    let dropped = Arc::new(tokio::sync::Notify::new());
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = calls.clone();
    let notify = dropped.clone();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let _upstream = super::Guard(tokio::spawn(async move {
        axum::serve(listener, Router::new().route("/responses", post(move |headers: axum::http::HeaderMap| {
            assert_eq!(headers["authorization"], "Bearer synthetic-upstream-credential-0001");
            let ordinal = observed.fetch_add(1, Ordering::SeqCst);
            let notify = notify.clone();
            async move {
                let events = super::responses_wire::response_events(json!({"id":"source","object":"response",
                    "created_at":1,"model":"private-model","status":"completed","output":[{"id":"text",
                        "type":"message","role":"assistant","status":"completed","content":[
                            {"type":"output_text","text":"visible","annotations":[],"logprobs":[]}]}],
                    "usage":{"input_tokens":1,"output_tokens":1,"total_tokens":2}}));
                let selected: Vec<_> = if ordinal == 0 {
                    events.into_iter().take_while(|e| e["type"] != "response.output_text.done").collect()
                } else { events };
                let bytes = Bytes::from(selected.iter().map(|event| format!(
                    "event: {}\ndata: {event}\n\n", event["type"].as_str().unwrap())).collect::<String>());
                let body = if ordinal == 0 {
                    let guard = UpstreamDrop(notify);
                    let held = futures_util::stream::iter([Ok::<_, Infallible>(bytes)])
                        .chain(futures_util::stream::pending()).inspect(move |_| { let _ = &guard; });
                    Body::from_stream(held)
                } else { Body::from(bytes) };
                Response::builder().header("content-type", "text/event-stream").body(body).unwrap()
            }
        }))).await.unwrap();
    }));
    let gateway = super::support::gateway(
        &origin,
        morphiecore::gateway::Limits {
            concurrency: 1,
            ..Default::default()
        },
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}/v1", listener.local_addr().unwrap());
    let (shutdown, stopped) = tokio::sync::oneshot::channel();
    let server = gateway.serve(listener, async {
        let _ = stopped.await;
    });
    let mut serving = super::Guard(tokio::spawn(async move { server.await.unwrap() }));
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();
    let body = json!({"model":"public-model","input":"hold","store":false,"stream":true,"max_output_tokens":32});
    let post = || {
        client
            .post(format!("{base}/responses"))
            .bearer_auth(super::support::CLIENT_KEY)
            .json(&body)
    };
    let mut response = post().send().await.unwrap();
    assert_eq!(response.status(), 200);
    let mut received = Vec::new();
    while !String::from_utf8_lossy(&received).contains("\"delta\":\"visible\"") {
        received.extend_from_slice(&response.chunk().await.unwrap().unwrap());
    }
    // Deliberately stop consuming. No terminal/EOF is available and the sole permit stays owned.
    let busy = post().send().await.unwrap();
    assert_eq!(busy.status(), 429);
    assert_eq!(
        busy.json::<Value>().await.unwrap()["error"]["code"],
        "gateway_busy"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        client
            .get(format!("{base}/responses/missing"))
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    drop(response);
    tokio::time::timeout(Duration::from_secs(3), dropped.notified())
        .await
        .unwrap();
    let mut recovered = false;
    for _ in 0..8 {
        let response = post().send().await.unwrap();
        if response.status() == 429 {
            assert_eq!(
                response.json::<Value>().await.unwrap()["error"]["code"],
                "gateway_busy"
            );
            continue;
        }
        assert_eq!(response.status(), 200);
        assert!(
            response
                .text()
                .await
                .unwrap()
                .contains("response.completed")
        );
        recovered = true;
        break;
    }
    assert!(
        recovered,
        "bounded, explicitly counted admission probes must recover"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    shutdown.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(3), &mut serving.0)
        .await
        .unwrap()
        .unwrap();
}
