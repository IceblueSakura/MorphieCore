//! Responses phase/opaque history and stored/stateless continuation through the Router.
use crate::openai_tool_loop as fixture;
use morphiecore::protocol::openai::GenerationApi;
use serde_json::{Value, json};

async fn request(
    client: &reqwest::Client,
    base: &str,
    key: &str,
    body: &Value,
    opening_reasoning: &Value,
) -> Value {
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
            assert_eq!(&event["response"]["reasoning"], opening_reasoning);
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
    let gateway = super::support::gateway(origin, morphiecore::gateway::Limits::default());
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
        let opening = json!({"effort":"low","summary":"auto"});
        let first = request(&client, &base, super::support::CLIENT_KEY, &body, &opening).await;
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
        let second = request(&client, &base, super::support::CLIENT_KEY, &body, &opening).await;
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
async fn stateless_tool_history_in_json_and_sse() {
    let fixture = fixture::start(GenerationApi::Responses, false).await;
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap();
    let opening =
        json!({"effort":"low","summary":"detailed","context":"all_turns","mode":"standard"});
    for stream in [false, true] {
        let mut body = json!({"model":"gpt-6-luna","input":[{"role":"user","content":"lookup"}],"store":false,
            "max_output_tokens":256,"reasoning":{"effort":"low","summary":"auto"},
            "include":["reasoning.encrypted_content"],"tools":[{"type":"function","name":"lookup",
            "description":"Synthetic lookup","parameters":{"type":"object","properties":{"key":{"type":"string"}},
            "required":["key"]},"strict":false}],"stream":stream});
        for turn in 1..=3 {
            let response =
                request(&client, &fixture.base, fixture::CLIENT_KEY, &body, &opening).await;
            assert_eq!(
                response["usage"]["total_tokens"], 18,
                "stream={stream}, turn={turn}: {response}"
            );
            assert_eq!(response["status"], "completed");
            assert_eq!(
                response["reasoning"],
                json!({"effort":"low","summary":"detailed","context":"all_turns","mode":"standard"})
            );
            assert_eq!(response["service_tier"], "default");
            assert!(response.get("text").is_none());
            assert_eq!(
                response["usage"]["output_tokens_details"]["reasoning_tokens"],
                2
            );
            let output = response["output"].as_array().unwrap();
            if turn == 1 {
                assert!(
                    output
                        .iter()
                        .any(|v| v["type"] == "reasoning" && v["summary"][0]["text"] == "Plan 🧪")
                );
                assert!(output.iter().any(|v| v["phase"] == "commentary"));
                assert!(output.iter().any(|v| v["call_id"] == "call-lookup"
                    && v["name"] == "lookup"
                    && v["arguments"] == "{ \"key\": \"A\" }"));
            } else {
                assert!(
                    output
                        .iter()
                        .any(|v| v["phase"] == "final_answer"
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
            let returned = output.clone();
            body["input"].as_array_mut().unwrap().extend(returned);
            body["input"]
                .as_array_mut()
                .unwrap()
                .extend(next.as_array().unwrap().iter().cloned());
        }
    }
    assert_eq!(fixture.seen.lock().unwrap().len(), 6);
    fixture.gateway.flush_diagnostics().await;
}
