//! Published tool calls prevent replay; transport EOF owns success and stored publication.
use crate::openai_tool_loop as fixture;
use morphiecore::protocol::openai::Profile;
use serde_json::{Value, json};

#[tokio::test]
async fn tool_publication_withholds_success_until_eof_and_never_replays() {
    use std::time::Duration;
    for profile in [Profile::Chat, Profile::Responses] {
        let chat = profile == Profile::Chat;
        for mode in ["hold", "cut", "cancel"] {
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
