//! DashScope Chat tool-result and final-answer history through the Router.
use crate::openai_tool_loop as fixture;
use morphiecore::protocol::openai::GenerationApi;
use serde_json::{Value, json};

async fn request(client: &reqwest::Client, base: &str, body: &Value, session: &str) -> Value {
    let response = client
        .post(format!("{base}/chat/completions"))
        .bearer_auth(fixture::CLIENT_KEY)
        .header("x-session-affinity", session)
        .json(body)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    if body["stream"] != true {
        return response.json().await.unwrap();
    }
    let bytes = response.text().await.unwrap();
    if body.get("preserve_thinking") != Some(&json!(true)) {
        for hidden in ["reasoning_content", "reasoning_details", "Plan", "mc1."] {
            assert!(!bytes.contains(hidden), "{hidden}");
        }
    }
    let mut message = json!({"role":"assistant","content":""});
    let mut calls = std::collections::BTreeMap::<u64, Value>::new();
    let mut terminal = None;
    let mut usage = Value::Null;
    let mut done = 0;
    for line in bytes.lines().filter_map(|l| l.strip_prefix("data: ")) {
        if line == "[DONE]" {
            done += 1;
            continue;
        }
        let event: Value = serde_json::from_str(line).unwrap();
        if let Some(value) = event.get("usage")
            && !value.is_null()
        {
            usage = value.clone();
        }
        if let Some(choices) = event["choices"].as_array() {
            for choice in choices {
                let delta = &choice["delta"];
                if let Some(text) = delta["content"].as_str() {
                    message["content"] =
                        json!(format!("{}{text}", message["content"].as_str().unwrap()));
                }
                if let Some(text) = delta["reasoning_content"].as_str() {
                    message["reasoning_content"] = json!(format!(
                        "{}{text}",
                        message["reasoning_content"].as_str().unwrap_or("")
                    ));
                }
                if let Some(values) = delta["tool_calls"].as_array() {
                    for value in values {
                        let call = calls
                            .entry(value["index"].as_u64().unwrap())
                            .or_insert_with(
                                || json!({"type":"function","function":{"name":"","arguments":""}}),
                            );
                        if let Some(id) = value.get("id") {
                            call["id"] = id.clone();
                        }
                        for key in ["name", "arguments"] {
                            if let Some(text) = value["function"][key].as_str() {
                                call["function"][key] = json!(format!(
                                    "{}{text}",
                                    call["function"][key].as_str().unwrap()
                                ));
                            }
                        }
                    }
                }
                if choice["finish_reason"].is_string() {
                    assert!(terminal.replace(choice["finish_reason"].clone()).is_none());
                }
            }
        }
    }
    assert_eq!(done, 1);
    if !calls.is_empty() {
        message["tool_calls"] = json!(calls.into_values().collect::<Vec<_>>());
    }
    json!({"choices":[{"message":message,"finish_reason":terminal.unwrap()}],"usage":usage})
}

#[tokio::test]
async fn tool_result_and_final_history_survive_json_and_sse() {
    history_case(false).await;
}

#[tokio::test]
async fn tool_feedback_migrates_credentials_and_sticks_after_primary_recovery() {
    history_case(true).await;
}

#[tokio::test]
async fn cooling_chat_pool_returns_retry_after_without_dispatch() {
    let fixture = fixture::start_recovery().await;
    let unavailable = fixture
        .recovery
        .as_ref()
        .unwrap()
        .assert_all_cooling(
            &format!("{}/chat/completions", fixture.base),
            fixture::CLIENT_KEY,
            &input(false),
            503,
        )
        .await;
    assert_eq!(unavailable["error"]["code"], "upstream_unavailable");
}

fn input(stream: bool) -> Value {
    let mut body = json!({"model":"qwen3.8-flash","messages":[{"role":"user","content":"lookup"}],
        "max_completion_tokens":256,"reasoning_effort":"low","tools":[{"type":"function",
        "function":{"name":"lookup","description":"Synthetic lookup","parameters":{"type":"object",
        "properties":{"key":{"type":"string"}},"required":["key"]},"strict":false}}],"stream":stream});
    if stream {
        body["stream_options"] = json!({"include_usage":true});
    }
    body
}

#[tokio::test]
async fn readable_reasoning_survives_tool_feedback_and_next_turn() {
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    for stream in [false, true] {
        let fixture = fixture::start_readable().await;
        let mut body = input(stream);
        body["messages"][0]["content"] = json!("reasoning");
        body["preserve_thinking"] = json!(true);
        for turn in 1..=3 {
            let result = request(&client, &fixture.base, &body, "readable").await;
            let message = result["choices"][0]["message"].clone();
            assert_eq!(
                message["reasoning_content"],
                if turn == 1 { "Plan 🧪" } else { "Check" }
            );
            assert!(message.get("reasoning_details").is_none());
            assert!(!message.to_string().contains("mca1."));
            let history = body["messages"].as_array_mut().unwrap();
            history.push(message);
            if turn == 1 {
                history.extend([
                    json!({"role":"tool","tool_call_id":"call-second","content":"second-result"}),
                    json!({"role":"tool","tool_call_id":"call-lookup","content":fixture::RESULT}),
                ]);
            } else {
                history.push(json!({"role":"user","content":"next"}));
            }
        }
        let seen = fixture.seen.lock().unwrap();
        assert_eq!(seen.len(), 3);
        assert!(seen.iter().all(|body| body["preserve_thinking"] == true));
    }
}

#[tokio::test]
async fn standard_chat_rejects_required_tool_history_before_dispatch() {
    let fixture = fixture::start_required_history().await;
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    let response = client
        .post(format!("{}/chat/completions", fixture.base))
        .bearer_auth(fixture::CLIENT_KEY)
        .json(&input(false))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 400);
    assert!(fixture.seen.lock().unwrap().is_empty());
}

#[tokio::test]
async fn standard_chat_omits_generated_reasoning_and_preserves_tool_calls() {
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap();
    for stream in [false, true] {
        let fixture = fixture::start(GenerationApi::Chat, false).await;
        let mut body = input(stream);
        body["messages"][0]["content"] = json!("reasoning");
        let result = request(&client, &fixture.base, &body, "standard").await;
        assert_eq!(result["choices"][0]["finish_reason"], "tool_calls");
        assert_eq!(result["usage"]["total_tokens"], 18);
        let message = &result["choices"][0]["message"];
        for hidden in ["reasoning_content", "reasoning_details", "Plan", "mc1."] {
            assert!(!message.to_string().contains(hidden), "{hidden}");
        }
        assert_eq!(
            message["tool_calls"],
            json!([
                {"id":"call-lookup","type":"function",
                    "function":{"name":"lookup","arguments":"{ \"key\": \"A\" }"}},
                {"id":"call-second","type":"function",
                    "function":{"name":"lookup","arguments":"{\"key\":\"B\"}"}}
            ])
        );
        assert_eq!(fixture.seen.lock().unwrap().len(), 1);
    }
}

async fn history_case(recover: bool) {
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap();
    for stream in [false, true] {
        let fixture = if recover {
            fixture::start_recovery().await
        } else {
            fixture::start(GenerationApi::Chat, false).await
        };
        let mut body = input(stream);
        let initial = body.clone();
        for turn in 1..=3 {
            let response = request(&client, &fixture.base, &body, "synthetic-chat-session").await;
            assert_eq!(
                response["usage"]["total_tokens"], 18,
                "stream={stream}, turn={turn}: {response}"
            );
            assert_eq!(
                response["choices"][0]["finish_reason"],
                if turn == 1 { "tool_calls" } else { "stop" }
            );
            let message = response["choices"][0]["message"].clone();
            assert!(message.get("reasoning_content").is_none());
            assert!(message.get("reasoning_details").is_none());
            if turn == 1 {
                assert_eq!(message["tool_calls"].as_array().unwrap().len(), 2);
                assert_eq!(
                    message["tool_calls"][0],
                    json!({"id":"call-lookup","type":"function",
                    "function":{"name":"lookup","arguments":"{ \"key\": \"A\" }"}})
                );
                body["messages"].as_array_mut().unwrap().extend([
                    message,
                    json!({"role":"tool","tool_call_id":"call-second","content":"second-result"}),
                    json!({"role":"tool","tool_call_id":"call-lookup","content":fixture::RESULT}),
                ]);
            } else {
                assert_eq!(message["content"], "cedar-71");
                if recover && turn == 2 {
                    let recovered =
                        request(&client, &fixture.base, &initial, "other-chat-session").await;
                    assert_eq!(recovered["choices"][0]["finish_reason"], "tool_calls");
                }
                body["messages"]
                    .as_array_mut()
                    .unwrap()
                    .extend([message, json!({"role":"user","content":"next"})]);
            }
        }
        assert_eq!(
            fixture.seen.lock().unwrap().len(),
            if recover { 5 } else { 3 }
        );
        if let Some(recovery) = &fixture.recovery {
            recovery.assert_complete(3, false);
        }
        fixture.gateway.flush_diagnostics().await;
    }
}
