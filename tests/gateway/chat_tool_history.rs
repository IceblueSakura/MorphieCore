//! DashScope Chat tool-result, readable reasoning and final-answer history through the Router.
use crate::openai_tool_loop as fixture;
use morphiecore::protocol::openai::Profile;
use serde_json::{Value, json};

async fn request(client: &reqwest::Client, base: &str, body: &Value) -> Value {
    let response = client
        .post(format!("{base}/chat/completions"))
        .bearer_auth(fixture::CLIENT_KEY)
        .json(body)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    if body["stream"] != true {
        return response.json().await.unwrap();
    }
    let bytes = response.text().await.unwrap();
    let mut message = json!({"role":"assistant","content":"","reasoning_content":""});
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
                for key in ["content", "reasoning_content"] {
                    if let Some(text) = delta[key].as_str() {
                        message[key] = json!(format!("{}{text}", message[key].as_str().unwrap()));
                    }
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
async fn tool_result_and_reasoning_history_survive_json_and_sse() {
    let fixture = fixture::start(Profile::Chat, false).await;
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap();
    for stream in [false, true] {
        let mut body = json!({"model":"qwen3.8-flash","messages":[{"role":"user","content":"lookup"}],
            "max_completion_tokens":256,"reasoning_effort":"low","tools":[{"type":"function",
            "function":{"name":"lookup","description":"Synthetic lookup","parameters":{"type":"object",
            "properties":{"key":{"type":"string"}},"required":["key"]},"strict":false}}],"stream":stream});
        if stream {
            body["stream_options"] = json!({"include_usage":true});
        }
        for turn in 1..=3 {
            let response = request(&client, &fixture.base, &body).await;
            assert_eq!(
                response["usage"]["total_tokens"], 18,
                "stream={stream}, turn={turn}: {response}"
            );
            assert_eq!(
                response["choices"][0]["finish_reason"],
                if turn == 1 { "tool_calls" } else { "stop" }
            );
            let message = response["choices"][0]["message"].clone();
            assert_eq!(
                message["reasoning_content"],
                if turn == 1 { "Plan 🧪" } else { "Check" }
            );
            if turn == 1 {
                assert_eq!(message["tool_calls"].as_array().unwrap().len(), 1);
                assert_eq!(
                    message["tool_calls"][0],
                    json!({"id":"call-lookup","type":"function",
                    "function":{"name":"lookup","arguments":"{ \"key\": \"A\" }"}})
                );
                body["messages"].as_array_mut().unwrap().extend([
                    message,
                    json!({"role":"tool","tool_call_id":"call-lookup","content":fixture::RESULT}),
                ]);
            } else {
                assert_eq!(message["content"], "cedar-71");
                body["messages"]
                    .as_array_mut()
                    .unwrap()
                    .extend([message, json!({"role":"user","content":"next"})]);
            }
        }
    }
    assert_eq!(fixture.seen.lock().unwrap().len(), 6);
    fixture.gateway.flush_diagnostics().await;
}
