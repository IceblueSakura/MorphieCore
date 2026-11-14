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
            assert!(event["response"].get("text").is_none());
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
async fn stateless_standard_history_survives_a_fresh_gateway_without_stored_response() {
    use axum::{Json, Router, routing::post};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();
    for stream in [false, true] {
        let seen = Arc::new(Mutex::new(Vec::<Value>::new()));
        let received = seen.clone();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let _upstream = super::Guard(tokio::spawn(async move {
            axum::serve(listener, Router::new().route("/responses", post(move |Json(body): Json<Value>| {
                let received = received.clone();
                async move {
                    let mut seen = received.lock().unwrap();
                    assert_eq!(body["store"], false);
                    assert!(body.get("previous_response_id").is_none());
                    let first = seen.is_empty();
                    seen.push(body);
                    let output = if first {
                        json!([
                            {"id":"reason","type":"reasoning","status":"completed",
                             "summary":[{"type":"summary_text","text":"Read the value"}],
                             "encrypted_content":"synthetic-native-replay"},
                            {"id":"call-item","type":"function_call","status":"completed",
                             "call_id":"call-read","name":"read","arguments":"{ \"key\": \"A\" }"}
                        ])
                    } else {
                        json!([{"id":"answer","type":"message","role":"assistant","status":"completed",
                            "content":[{"type":"output_text","text":"value-A","annotations":[],"logprobs":[]}]}])
                    };
                    let wire = crate::responses_wire::response_stream(json!({
                        "id":if first {"first"} else {"second"}, "object":"response","created_at":1,
                        "model":"private-model","status":"completed","output":output,
                        "reasoning":{"effort":"low","summary":"auto"},"service_tier":"auto"
                    }));
                    ([("content-type","text/event-stream")], wire)
                }
            }))).await.unwrap();
        }));
        let (base, shutdown, mut server) = bounded_history_service(&origin).await;
        let mut body = json!({"model":"public-model","store":false,"stream":stream,
            "input":[{"role":"user","content":"Read A"}],
            "reasoning":{"effort":"low","summary":"auto"},"service_tier":"auto",
            "include":["reasoning.encrypted_content"],"tools":[{"type":"function","name":"read",
                "parameters":{"type":"object","properties":{"key":{"type":"string"}}},"strict":false}]});
        let first = request(&client, &base, super::support::CLIENT_KEY, &body).await;
        assert_eq!(
            client
                .get(format!(
                    "{base}/responses/{}",
                    first["id"].as_str().unwrap()
                ))
                .bearer_auth(super::support::CLIENT_KEY)
                .send()
                .await
                .unwrap()
                .status(),
            400
        );
        assert!(
            first["output"][0]["encrypted_content"]
                .as_str()
                .unwrap()
                .starts_with("mc1.")
        );
        body["input"]
            .as_array_mut()
            .unwrap()
            .extend(first["output"].as_array().unwrap().iter().cloned());
        body["input"].as_array_mut().unwrap().push(json!({
            "type":"function_call_output","call_id":"call-read","output":"value-A"
        }));
        shutdown.send(()).unwrap();
        tokio::time::timeout(Duration::from_secs(3), &mut server.0)
            .await
            .unwrap()
            .unwrap();
        let (base, shutdown, mut server) = bounded_history_service(&origin).await;
        let second = request(&client, &base, super::support::CLIENT_KEY, &body).await;
        assert_eq!(second["output"][0]["content"][0]["text"], "value-A");
        {
            let sent = seen.lock().unwrap();
            assert_eq!(sent.len(), 2);
            let input = sent[1]["input"].as_array().unwrap();
            assert_eq!(input.len(), 4);
            assert_eq!(input[1]["encrypted_content"], "synthetic-native-replay");
            assert_eq!(input[2]["call_id"], "call-read");
            assert_eq!(input[2]["arguments"], "{ \"key\": \"A\" }");
            assert_eq!(
                input[3],
                json!({"type":"function_call_output","call_id":"call-read","output":"value-A"})
            );
        }
        shutdown.send(()).unwrap();
        tokio::time::timeout(Duration::from_secs(3), &mut server.0)
            .await
            .unwrap()
            .unwrap();
    }
}

#[tokio::test]
#[ignore = "experimental stateful API; awaiting explicit activation"]
async fn experimental_stored_history_capacity_delete_and_restart_release_without_hidden_dispatch() {
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
            async move { ([("content-type","text/event-stream")], super::responses_wire::response_stream(json!({"id":format!("fixture-response-{ordinal}"),"object":"response",
                "created_at":1,"status":"completed","model":"private-model","output":[{"type":"message",
                    "id":format!("fixture-message-{ordinal}"),"role":"assistant","status":"completed",
                    "content":[{"type":"output_text","text":"done","annotations":[]}]}],
                "usage":{"input_tokens":1,"output_tokens":1,"total_tokens":2}}))) }
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

async fn tool_history(include_stored: bool) {
    let fixture = fixture::start(GenerationApi::Responses, false).await;
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap();
    for stream in [false, true] {
        let mut effective = Vec::new();
        for &stored in if include_stored {
            &[false, true][..]
        } else {
            &[false][..]
        } {
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
                assert!(response.get("text").is_none());
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
        if include_stored {
            assert_eq!(
                effective[0], effective[1],
                "stored and stateless effective histories differ"
            );
        }
    }
    assert_eq!(
        fixture.seen.lock().unwrap().len(),
        if include_stored { 12 } else { 6 }
    );
    fixture.gateway.flush_diagnostics().await;
}

#[tokio::test]
async fn stateless_tool_history_in_json_and_sse() {
    tool_history(false).await;
}

#[tokio::test]
#[ignore = "experimental stateful API; awaiting explicit activation"]
async fn experimental_stored_and_stateless_tool_history_agree_in_json_and_sse() {
    tool_history(true).await;
}
