//! R1 selected-binding JSON/SSE and stateless/stored history handoff.
#[path = "../support/r1.rs"]
mod fixture;
use serde_json::{Value, json};

async fn request(client: &reqwest::Client, base: &str, chat: bool, body: &Value) -> Value {
    let response = client
        .post(format!(
            "{base}/{}",
            if chat {
                "chat/completions"
            } else {
                "responses"
            }
        ))
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
        if !chat {
            if event["type"] == "response.completed" {
                done += 1;
                terminal = Some(event["response"].clone());
            }
            continue;
        }
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
                    terminal = Some(choice["finish_reason"].clone());
                }
            }
        }
    }
    assert_eq!(done, 1);
    if !chat {
        return terminal.unwrap();
    }
    if !calls.is_empty() {
        message["tool_calls"] = json!(calls.into_values().collect::<Vec<_>>());
    }
    json!({"choices":[{"message":message,"finish_reason":terminal.unwrap()}],"usage":usage})
}

#[tokio::test]
async fn selected_r1_bindings_preserve_three_turn_history_in_both_deliveries() {
    let fixture = fixture::start().await;
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap();
    for chat in [true, false] {
        for stream in [false, true] {
            let mut effective = Vec::new();
            for stored in if chat { vec![false] } else { vec![false, true] } {
                let mut body = if chat {
                    json!({"model":"qwen3.8-flash","messages":[{"role":"user","content":"lookup"}],
                        "max_completion_tokens":256,"reasoning_effort":"low","tools":[{"type":"function",
                        "function":{"name":"lookup","description":"Synthetic lookup","parameters":{"type":"object",
                        "properties":{"key":{"type":"string"}},"required":["key"]},"strict":false}}]})
                } else {
                    json!({"model":"gpt-6-luna","input":[{"role":"user","content":"lookup"}],"store":stored,
                        "max_output_tokens":256,"reasoning":{"effort":"low","summary":"auto"},
                        "include":["reasoning.encrypted_content"],"tools":[{"type":"function","name":"lookup",
                        "description":"Synthetic lookup","parameters":{"type":"object","properties":{"key":{"type":"string"}},
                        "required":["key"]},"strict":false}]})
                };
                body["stream"] = json!(stream);
                if chat && stream {
                    body["stream_options"] = json!({"include_usage":true});
                }
                let start = fixture.seen.lock().unwrap().len();
                let mut output_id = Value::Null;
                for turn in 1..=3 {
                    let response = request(&client, &fixture.base, chat, &body).await;
                    assert_eq!(
                        response["usage"]["total_tokens"], 18,
                        "chat={chat}, stream={stream}, stored={stored}, turn={turn}: {response}"
                    );
                    if chat {
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
                            assert_eq!(
                                message["tool_calls"][0]["function"]["arguments"],
                                "{ \"key\": \"A\" }"
                            );
                            body["messages"].as_array_mut().unwrap().extend([message,
                                json!({"role":"tool","tool_call_id":"call-r1","content":fixture::RESULT})]);
                        } else {
                            assert_eq!(message["content"], "cedar-71");
                            body["messages"]
                                .as_array_mut()
                                .unwrap()
                                .extend([message, json!({"role":"user","content":"next"})]);
                        }
                    } else {
                        assert_eq!(response["status"], "completed");
                        assert_eq!(
                            response["usage"]["output_tokens_details"]["reasoning_tokens"],
                            2
                        );
                        let output = response["output"].as_array().unwrap();
                        if turn == 1 {
                            assert!(output.iter().any(|v| v["type"] == "reasoning"
                                && v["summary"][0]["text"] == "Plan 🧪"));
                            assert!(output.iter().any(|v| v["phase"] == "commentary"));
                            assert!(output.iter().any(|v| v["call_id"] == "call-r1"
                                && v["arguments"] == "{ \"key\": \"A\" }"));
                        } else {
                            assert!(output.iter().any(|v| v["phase"] == "final_answer"
                                && v["content"][0]["text"] == "cedar-71"));
                        }
                        let new_input = if turn == 1 {
                            json!({"type":"function_call_output","call_id":"call-r1","output":fixture::RESULT})
                        } else {
                            json!({"role":"user","content":"next"})
                        };
                        if stored {
                            output_id = response["id"].clone();
                            body["previous_response_id"] = output_id.clone();
                            body["input"] = json!([new_input]);
                        } else {
                            body["input"]
                                .as_array_mut()
                                .unwrap()
                                .extend(output.iter().cloned());
                            body["input"].as_array_mut().unwrap().push(new_input);
                        }
                    }
                }
                effective.push(
                    fixture.seen.lock().unwrap()[start..]
                        .iter()
                        .map(|v| v[if chat { "messages" } else { "input" }].clone())
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
            if !chat {
                assert_eq!(
                    effective[0], effective[1],
                    "stored and stateless effective histories differ"
                );
            }
        }
    }
    assert_eq!(fixture.seen.lock().unwrap().len(), 18);
    fixture.gateway.flush_diagnostics().await;
}

#[tokio::test]
async fn r1_tool_publication_withholds_success_until_eof_and_never_replays() {
    use std::time::Duration;
    for chat in [true, false] {
        for mode in ["hold", "cut", "cancel"] {
            let fixture = fixture::start_with_fallback(true).await;
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
            while !String::from_utf8_lossy(&received).contains("call-r1") {
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
                assert_eq!(failed, mode == "cut");
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
