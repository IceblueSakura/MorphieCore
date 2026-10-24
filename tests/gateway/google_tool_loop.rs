//! Ordinary STOP, error feedback, corrective calls and signed final history through the Router.
use crate::google_tool_fixture as fixture;
use morphiecore::protocol::openai::Profile;
use serde_json::{Value, json};
use std::collections::BTreeMap;

async fn complete(client: &reqwest::Client, base: &str, profile: Profile, body: &Value) -> Value {
    let path = if profile == Profile::Chat {
        "chat/completions"
    } else {
        "responses"
    };
    let response = client
        .post(format!("{base}/{path}"))
        .bearer_auth(fixture::KEY)
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
    let mut usage = Value::Null;
    let mut done = 0;
    let mut message = json!({"role":"assistant","content":null});
    let mut calls = BTreeMap::<u64, Value>::new();
    let mut reasoning = BTreeMap::<u64, Value>::new();
    for line in wire.lines().filter_map(|line| line.strip_prefix("data: ")) {
        if line == "[DONE]" {
            done += 1;
            continue;
        }
        let event: Value = serde_json::from_str(line).unwrap();
        if profile == Profile::Responses {
            assert!(!matches!(
                event["type"].as_str(),
                Some("error" | "response.failed" | "response.incomplete")
            ));
            if event["type"] == "response.completed" {
                assert!(terminal.replace(event["response"].clone()).is_none());
            }
            continue;
        }
        if let Some(value) = event.get("usage").filter(|value| !value.is_null()) {
            usage = value.clone();
        }
        for choice in event["choices"].as_array().unwrap() {
            let delta = &choice["delta"];
            for key in ["content", "reasoning_content"] {
                if let Some(text) = delta[key].as_str() {
                    message[key] = json!(format!(
                        "{}{text}",
                        message[key].as_str().unwrap_or_default()
                    ));
                }
            }
            if let Some(details) = delta["reasoning_details"].as_array() {
                for detail in details {
                    let index = detail["index"].as_u64().unwrap_or_else(|| {
                        reasoning.last_key_value().map_or(0, |(index, _)| index + 1)
                    });
                    let entry = reasoning.entry(index).or_insert_with(|| json!({}));
                    for (key, value) in detail.as_object().unwrap() {
                        if matches!(key.as_str(), "text" | "summary" | "data") {
                            entry[key] = json!(format!(
                                "{}{}",
                                entry[key].as_str().unwrap_or_default(),
                                value.as_str().unwrap()
                            ));
                        } else if let Some(previous) = entry.get(key) {
                            assert_eq!(previous, value);
                        } else {
                            entry[key] = value.clone();
                        }
                    }
                }
            }
            if let Some(parts) = delta["tool_calls"].as_array() {
                for part in parts {
                    let call = calls.entry(part["index"].as_u64().unwrap()).or_insert_with(
                        || json!({"type":"function","function":{"name":"","arguments":""}}),
                    );
                    if let Some(id) = part.get("id") {
                        call["id"] = id.clone();
                    }
                    for key in ["name", "arguments"] {
                        if let Some(text) = part["function"][key].as_str() {
                            call["function"][key] =
                                json!(format!("{}{text}", call["function"][key].as_str().unwrap()));
                        }
                    }
                }
            }
            if choice["finish_reason"].is_string() {
                assert!(terminal.replace(choice["finish_reason"].clone()).is_none());
            }
        }
    }
    if profile == Profile::Responses {
        return terminal.unwrap();
    }
    assert_eq!(done, 1);
    if !reasoning.is_empty() {
        message["reasoning_details"] = json!(reasoning.into_values().collect::<Vec<_>>());
    }
    if !calls.is_empty() {
        message["tool_calls"] = json!(calls.into_values().collect::<Vec<_>>());
    }
    json!({"choices":[{"message":message,"finish_reason":terminal.unwrap()}],"usage":usage})
}

async fn exercise_messages(api: fixture::Api) {
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap();
    for stream in [false, true] {
        let fixture = fixture::start_messages(api).await;
        let mut response = client
            .post(format!("{}/messages", fixture.base))
            .bearer_auth(fixture::KEY)
            .header("anthropic-version", "2026-10-21")
            .json(
                &json!({"model":"gemini-3.8-flash","max_tokens":256,"stream":stream,
                "thinking":{"type":"adaptive"},"output_config":{"effort":"low"},
                "tools":[{"name":"read","input_schema":{"type":"object","properties":{
                    "path":{"type":"string"}},"required":["path"]}}],
                "messages":[{"role":"user","content":"Read."}]}),
            )
            .send()
            .await
            .unwrap();
        if stream && response.status() == 200 {
            let mut bytes = vec![];
            let aborted = loop {
                match response.chunk().await {
                    Ok(Some(chunk)) => bytes.extend_from_slice(&chunk),
                    Ok(None) => break false,
                    Err(_) => break true,
                }
            };
            let wire = String::from_utf8(bytes).unwrap();
            assert!(aborted || wire.contains("event: error\n"));
            assert!(!wire.contains("event: message_stop\n"));
            assert!(!wire.contains("mca1."));
        } else {
            assert_eq!(response.status(), 502);
            let error: Value = response.json().await.unwrap();
            assert_eq!(error["type"], "error");
            assert_eq!(error["error"]["type"], "api_error");
        }
        assert_eq!(
            fixture.seen.lock().unwrap().len(),
            1,
            "unrepresentable call signatures cannot be retried or silently discarded"
        );
    }
}
async fn exercise(api: fixture::Api) {
    exercise_messages(api).await;
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap();
    for profile in [Profile::Chat, Profile::Responses] {
        for stream in [false, true] {
            let fixture = fixture::start(profile, api).await;
            let function = json!({"name":"read","strict":false,"parameters":{
                "type":"object","properties":{"path":{"type":"string"}},"required":["path"]}});
            let mut body = if profile == Profile::Chat {
                json!({"model":"gemini-3.8-flash","messages":[{"role":"user","content":"Read."}],
                    "max_completion_tokens":256,"reasoning_effort":"low",
                    "tools":[{"type":"function","function":function}],"stream":stream})
            } else {
                let mut tool = function;
                tool["type"] = json!("function");
                json!({"model":"gemini-3.8-flash","input":[{"role":"user","content":"Read."}],
                    "store":true,"max_output_tokens":256,"reasoning":{"effort":"low","summary":"auto"},
                    "include":["reasoning.encrypted_content"],"tools":[tool],"stream":stream})
            };
            if stream && profile == Profile::Chat {
                body["stream_options"] = json!({"include_usage":true});
            }
            for turn in 1..=4 {
                let response = complete(&client, &fixture.base, profile, &body).await;
                if api == fixture::Api::GenerateContent {
                    assert_eq!(response["usage"]["total_tokens"], 15);
                } else {
                    // Source accounting stays sparse; the delivery estimates
                    // six visible reasoning bytes only for the first three turns.
                    assert_eq!(
                        response["usage"]["total_tokens"],
                        if turn <= 3 { 17 } else { 15 }
                    );
                }
                if profile == Profile::Chat {
                    // STOP stays stop; Interactions explicitly requires tool results.
                    let reason = if api == fixture::Api::Interactions && turn <= 2 {
                        "tool_calls"
                    } else {
                        "stop"
                    };
                    assert_eq!(response["choices"][0]["finish_reason"], reason);
                    let message = response["choices"][0]["message"].clone();
                    if turn <= 2 {
                        let call = &message["tool_calls"][0];
                        assert_eq!(message["tool_calls"].as_array().unwrap().len(), 1);
                        assert_eq!(call["id"], format!("c{turn}"));
                        assert_eq!(call["function"]["name"], "read");
                        let arguments: Value =
                            serde_json::from_str(call["function"]["arguments"].as_str().unwrap())
                                .unwrap();
                        assert_eq!(
                            arguments,
                            json!({"path":if turn == 1 {"missing"} else {"found"}})
                        );
                        body["messages"].as_array_mut().unwrap().extend([
                            message,
                            json!({"role":"tool","tool_call_id":format!("c{turn}"),
                                "content":if turn == 1 {fixture::ERROR} else {fixture::RESULT}}),
                        ]);
                    } else {
                        assert_eq!(
                            message["content"],
                            if turn == 3 {
                                fixture::RESULT
                            } else {
                                "continued"
                            }
                        );
                        body["messages"]
                            .as_array_mut()
                            .unwrap()
                            .extend([message, json!({"role":"user","content":"Continue."})]);
                    }
                } else {
                    assert_eq!(response["status"], "completed");
                    let output = response["output"].as_array().unwrap();
                    if turn <= 2 {
                        let calls: Vec<_> = output
                            .iter()
                            .filter(|item| item["type"] == "function_call")
                            .collect();
                        assert_eq!(calls.len(), 1);
                        assert_eq!(calls[0]["call_id"], format!("c{turn}"));
                        assert_eq!(calls[0]["name"], "read");
                        let arguments: Value =
                            serde_json::from_str(calls[0]["arguments"].as_str().unwrap()).unwrap();
                        assert_eq!(
                            arguments,
                            json!({"path":if turn == 1 {"missing"} else {"found"}})
                        );
                    } else {
                        assert!(output.iter().any(|item| item["type"] == "message"
                            && item["content"][0]["text"]
                                == if turn == 3 {
                                    fixture::RESULT
                                } else {
                                    "continued"
                                }));
                    }
                    assert!(!response.to_string().contains("mca1."));
                    assert_eq!(
                        output
                            .iter()
                            .filter(|item| item["type"] == "reasoning")
                            .count(),
                        usize::from(api == fixture::Api::Interactions && turn <= 3)
                    );
                    body["previous_response_id"] = response["id"].clone();
                    body["input"] = json!([if turn <= 2 {
                        json!({"type":"function_call_output","call_id":format!("c{turn}"),
                            "output":if turn == 1 {fixture::ERROR} else {fixture::RESULT}})
                    } else {
                        json!({"role":"user","content":"Continue."})
                    }]);
                }
            }
            assert_eq!(fixture.seen.lock().unwrap().len(), 4);
            fixture.gateway.flush_diagnostics().await;
        }
    }
}

#[tokio::test]
async fn ordinary_stop_tool_error_correction_and_final_history() {
    exercise(fixture::Api::GenerateContent).await;
}

#[tokio::test]
async fn interactions_tool_correction_and_signed_final_history() {
    exercise(fixture::Api::Interactions).await;
}

#[tokio::test]
async fn stateless_responses_do_not_publish_success_or_store_signed_calls() {
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap();
    for api in [fixture::Api::GenerateContent, fixture::Api::Interactions] {
        for stream in [false, true] {
            let fixture = fixture::start(Profile::Responses, api).await;
            let mut response = client.post(format!("{}/responses", fixture.base))
                .bearer_auth(fixture::KEY)
                .json(&json!({"model":"gemini-3.8-flash","input":"Read.","store":false,
                    "stream":stream,"max_output_tokens":256,"reasoning":{"effort":"low","summary":"auto"},
                    "tools":[{"type":"function","name":"read","strict":false,"parameters":{
                        "type":"object","properties":{"path":{"type":"string"}},"required":["path"]}}]}))
                .send().await.unwrap();
            // A signature in the first intake batch can reject before publication.
            // A later signature must fail an already published stream, never succeed.
            if stream && response.status() == 200 {
                let mut bytes = vec![];
                let aborted = loop {
                    match response.chunk().await {
                        Ok(Some(chunk)) => bytes.extend_from_slice(&chunk),
                        Ok(None) => break false,
                        Err(_) => break true,
                    }
                };
                let body = String::from_utf8(bytes).unwrap();
                assert!(aborted || body.contains("event: error\n"));
                assert!(!body.contains("event: response.completed\n"));
                assert!(!body.contains("mca1."));
                for line in body.lines().filter_map(|line| line.strip_prefix("data: ")) {
                    let event: Value = serde_json::from_str(line).unwrap();
                    if let Some(id) = event["response"]["id"].as_str() {
                        let stored = client
                            .get(format!("{}/responses/{id}", fixture.base))
                            .bearer_auth(fixture::KEY)
                            .send()
                            .await
                            .unwrap();
                        assert_eq!(stored.status(), 404);
                    }
                }
            } else {
                assert_eq!(response.status(), 502);
                assert_eq!(
                    response.json::<Value>().await.unwrap()["error"]["code"],
                    "upstream_error"
                );
            }
            assert_eq!(
                fixture.seen.lock().unwrap().len(),
                1,
                "no retry after projection failure"
            );
        }
    }
}
