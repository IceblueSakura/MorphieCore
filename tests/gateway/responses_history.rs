//! Responses phase/opaque history and stored/stateless continuation through the Router.
use crate::openai_tool_loop as fixture;
use morphiecore::protocol::openai::GenerationApi;
use serde_json::{Value, json};

async fn request(client: &reqwest::Client, base: &str, key: &str, body: &Value) -> Value {
    let response = client
        .post(format!("{base}/responses"))
        .bearer_auth(key)
        .json(body)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    if body["stream"] != true {
        return response.json().await.unwrap();
    }
    let wire = response.text().await.unwrap();
    let mut terminal = None;
    for line in wire.lines().filter_map(|l| l.strip_prefix("data: ")) {
        let event: Value = serde_json::from_str(line).unwrap();
        if matches!(
            event["type"].as_str(),
            Some("response.created" | "response.in_progress")
        ) {
            assert_eq!(
                event["response"]["reasoning"],
                json!({"effort":"low","summary":"auto"})
            );
            assert_eq!(event["response"]["service_tier"], "auto");
            assert_eq!(event["response"].get("text"), Some(&Value::Null));
        }
        if event["type"] == "response.completed" {
            assert!(terminal.replace(event["response"].clone()).is_none());
        }
    }
    terminal.unwrap()
}

async fn bounded_history_service(
    origin: &str,
) -> (String, tokio::sync::oneshot::Sender<()>, super::Guard) {
    let gateway = super::support::gateway(
        origin,
        morphiecore::gateway::Limits {
            history_entries: 1,
            history_bytes: 64 << 10,
            ..Default::default()
        },
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}/v1", listener.local_addr().unwrap());
    let (shutdown, stopped) = tokio::sync::oneshot::channel();
    let server = gateway.serve(listener, async {
        let _ = stopped.await;
    });
    let serving = tokio::spawn(async move { server.await.unwrap() });
    (base, shutdown, super::Guard(serving))
}

#[tokio::test]
async fn stored_history_capacity_delete_and_restart_release_without_hidden_dispatch() {
    use std::{
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
        time::Duration,
    };
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = calls.clone();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let _upstream = super::Guard(tokio::spawn(async move {
        axum::serve(listener, axum::Router::new().route("/responses", axum::routing::post(move || {
            let ordinal = observed.fetch_add(1, Ordering::SeqCst);
            async move { axum::Json(json!({"id":format!("fixture-response-{ordinal}"),"object":"response",
                "created_at":1,"status":"completed","model":"private-model","output":[{"type":"message",
                    "id":format!("fixture-message-{ordinal}"),"role":"assistant","status":"completed",
                    "content":[{"type":"output_text","text":"done","annotations":[]}]}],
                "usage":{"input_tokens":1,"output_tokens":1,"total_tokens":2}})) }
        }))).await.unwrap();
    }));
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(3))
        .build()
        .unwrap();
    let (base, shutdown, mut serving) = bounded_history_service(&origin).await;
    let body = |input: Value| json!({"model":"public-model","input":input,"store":true,"max_output_tokens":32});
    let oversized = client
        .post(format!("{base}/responses"))
        .bearer_auth(super::support::CLIENT_KEY)
        .json(&body(json!("x".repeat(96 << 10))))
        .send()
        .await
        .unwrap();
    assert_eq!(oversized.status(), 429);
    assert_eq!(
        oversized.json::<Value>().await.unwrap()["error"]["code"],
        "history_capacity_exceeded"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let empty_parts = client
        .post(format!("{base}/responses"))
        .bearer_auth(super::support::CLIENT_KEY)
        .json(&body(json!([{"id":"msg_1","type":"message","status":"completed","role":"assistant","content":vec![
            json!({"type":"output_text","text":"","annotations":[]});1024
        ]}])))
        .send()
        .await
        .unwrap();
    assert_eq!(
        empty_parts.status(),
        429,
        "empty parts still occupy retained memory"
    );
    assert_eq!(
        empty_parts.json::<Value>().await.unwrap()["error"]["code"],
        "history_capacity_exceeded"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    let small = body(json!("small"));
    let a = request(&client, &base, super::support::CLIENT_KEY, &small).await;
    let id = a["id"].as_str().unwrap();
    let get = client
        .get(format!("{base}/responses/{id}"))
        .bearer_auth(super::support::CLIENT_KEY)
        .send()
        .await
        .unwrap();
    assert_eq!(get.status(), 200);
    assert_eq!(get.json::<Value>().await.unwrap(), a);
    let full = client
        .post(format!("{base}/responses"))
        .bearer_auth(super::support::CLIENT_KEY)
        .json(&small)
        .send()
        .await
        .unwrap();
    assert_eq!(full.status(), 429);
    assert_eq!(
        full.json::<Value>().await.unwrap()["error"]["code"],
        "history_capacity_exceeded"
    );
    let deleted = client
        .delete(format!("{base}/responses/{id}"))
        .bearer_auth(super::support::CLIENT_KEY)
        .send()
        .await
        .unwrap();
    assert_eq!(deleted.status(), 200);
    assert_eq!(deleted.json::<Value>().await.unwrap()["deleted"], true);
    assert_eq!(
        client
            .get(format!("{base}/responses/{id}"))
            .bearer_auth(super::support::CLIENT_KEY)
            .send()
            .await
            .unwrap()
            .status(),
        404
    );
    let b = request(&client, &base, super::support::CLIENT_KEY, &small).await;
    assert_eq!(calls.load(Ordering::SeqCst), 5);
    shutdown.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(3), &mut serving.0)
        .await
        .unwrap()
        .unwrap();
    let (base, shutdown, mut fresh) = bounded_history_service(&origin).await;
    let old = b["id"].as_str().unwrap();
    assert_eq!(
        client
            .get(format!("{base}/responses/{old}"))
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    assert_eq!(
        client
            .get(format!("{base}/responses/{old}"))
            .bearer_auth(super::support::CLIENT_KEY)
            .send()
            .await
            .unwrap()
            .status(),
        404
    );
    let continued = client
        .post(format!("{base}/responses"))
        .bearer_auth(super::support::CLIENT_KEY)
        .json(
            &json!({"model":"public-model","input":"next","store":true,"previous_response_id":old,
                    "max_output_tokens":32}),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(continued.status(), 404);
    assert_eq!(
        continued.json::<Value>().await.unwrap()["error"]["code"],
        "response_not_found"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 5);
    request(&client, &base, super::support::CLIENT_KEY, &small).await;
    assert_eq!(calls.load(Ordering::SeqCst), 6);
    shutdown.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(3), &mut fresh.0)
        .await
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn stored_and_stateless_tool_history_agree_in_json_and_sse() {
    let fixture = fixture::start(GenerationApi::Responses, false).await;
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap();
    for stream in [false, true] {
        let mut effective = Vec::new();
        for stored in [false, true] {
            let mut body = json!({"model":"gpt-6-luna","input":[{"role":"user","content":"lookup"}],"store":stored,
                "max_output_tokens":256,"reasoning":{"effort":"low","summary":"auto"},
                "include":["reasoning.encrypted_content"],"tools":[{"type":"function","name":"lookup",
                "description":"Synthetic lookup","parameters":{"type":"object","properties":{"key":{"type":"string"}},
                "required":["key"]},"strict":false}],"stream":stream});
            let start = fixture.seen.lock().unwrap().len();
            let mut output_id = Value::Null;
            for turn in 1..=3 {
                let response = request(&client, &fixture.base, fixture::CLIENT_KEY, &body).await;
                assert_eq!(
                    response["usage"]["total_tokens"], 18,
                    "stream={stream}, stored={stored}, turn={turn}: {response}"
                );
                assert_eq!(response["status"], "completed");
                assert_eq!(
                    response["reasoning"],
                    json!({"effort":"low","summary":"detailed","context":"all_turns","mode":"standard"})
                );
                assert_eq!(response["service_tier"], "default");
                assert_eq!(response.get("text"), Some(&Value::Null));
                if stored {
                    let retrieved = client
                        .get(format!(
                            "{}/responses/{}",
                            fixture.base,
                            response["id"].as_str().unwrap()
                        ))
                        .bearer_auth(fixture::CLIENT_KEY)
                        .send()
                        .await
                        .unwrap();
                    assert_eq!(retrieved.status(), 200);
                    let mut retrieved = retrieved.json::<Value>().await.unwrap();
                    let mut terminal = response.clone();
                    retrieved.as_object_mut().unwrap().remove("morphiecore");
                    terminal.as_object_mut().unwrap().remove("morphiecore");
                    assert_eq!(
                        retrieved, terminal,
                        "GET retains the delivered terminal view"
                    );
                }
                assert_eq!(
                    response["usage"]["output_tokens_details"]["reasoning_tokens"],
                    2
                );
                let output = response["output"].as_array().unwrap();
                if turn == 1 {
                    assert!(
                        output
                            .iter()
                            .any(|v| v["type"] == "reasoning"
                                && v["summary"][0]["text"] == "Plan 🧪")
                    );
                    assert!(output.iter().any(|v| v["phase"] == "commentary"));
                    assert!(output.iter().any(|v| v["call_id"] == "call-lookup"
                        && v["name"] == "lookup"
                        && v["arguments"] == "{ \"key\": \"A\" }"));
                } else {
                    assert!(
                        output.iter().any(|v| v["phase"] == "final_answer"
                            && v["content"][0]["text"] == "cedar-71")
                    );
                }
                let next = if turn == 1 {
                    json!([
                        {"type":"function_call_output","call_id":"call-second","output":"second-result"},
                        {"type":"function_call_output","call_id":"call-lookup","output":fixture::RESULT}
                    ])
                } else {
                    json!([{"role":"user","content":"next"}])
                };
                if stored {
                    output_id = response["id"].clone();
                    body["previous_response_id"] = output_id.clone();
                    body["input"] = next;
                } else {
                    let returned = output.clone();
                    body["input"].as_array_mut().unwrap().extend(returned);
                    body["input"]
                        .as_array_mut()
                        .unwrap()
                        .extend(next.as_array().unwrap().iter().cloned());
                }
            }
            effective.push(
                fixture.seen.lock().unwrap()[start..]
                    .iter()
                    .map(|v| v["input"].clone())
                    .collect::<Vec<_>>(),
            );
            if stored {
                let response = client
                    .get(format!(
                        "{}/responses/{}",
                        fixture.base,
                        output_id.as_str().unwrap()
                    ))
                    .bearer_auth(fixture::CLIENT_KEY)
                    .send()
                    .await
                    .unwrap();
                assert_eq!(response.status(), 200);
                assert_eq!(
                    response.json::<Value>().await.unwrap()["status"],
                    "completed"
                );
            }
        }
        assert_eq!(
            effective[0], effective[1],
            "stored and stateless effective histories differ"
        );
    }
    assert_eq!(fixture.seen.lock().unwrap().len(), 12);
    fixture.gateway.flush_diagnostics().await;
}
