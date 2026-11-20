//! Ordinary STOP, error feedback, corrective calls and signed final history through the Router.
use crate::google_tool_fixture as fixture;
use morphiecore::protocol::openai::GenerationApi;
use serde_json::{Value, json};

async fn exercise_rejected(api: fixture::Api, messages: bool) {
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap();
    for stream in [false, true] {
        let fixture = if messages {
            fixture::start_messages(api).await
        } else {
            fixture::start(GenerationApi::Chat, api, false).await
        };
        let (path, body) = if messages {
            (
                "messages",
                json!({"model":"gemini-3.8-flash","max_tokens":256,"stream":stream,
                "thinking":{"type":"adaptive"},"output_config":{"effort":"low"},
                "tools":[{"name":"read","input_schema":{"type":"object","properties":{
                    "path":{"type":"string"}},"required":["path"]}}],
                "messages":[{"role":"user","content":"Read."}]}),
            )
        } else {
            (
                "chat/completions",
                json!({"model":"gemini-3.8-flash","max_completion_tokens":256,"stream":stream,
                "reasoning_effort":"low","messages":[{"role":"user","content":"Read."}],
                "tools":[{"type":"function","function":{"name":"read","strict":false,"parameters":{
                    "type":"object","properties":{"path":{"type":"string"}},"required":["path"]}}}]}),
            )
        };
        let mut response = client
            .post(format!("{}/{path}", fixture.base))
            .bearer_auth(fixture::KEY)
            .header("anthropic-version", "2026-10-21")
            .json(&body)
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
            assert!(aborted || wire.contains("\"error\""));
            assert!(!wire.contains("[DONE]"));
            assert!(!wire.contains("\"finish_reason\":\""));
            assert!(!wire.contains("event: message_stop\n"));
            assert!(!wire.contains("mca1."));
        } else {
            assert_eq!(response.status(), 502);
            let error: Value = response.json().await.unwrap();
            if messages {
                assert_eq!(error["type"], "error");
                assert_eq!(error["error"]["type"], "api_error");
            } else {
                assert_eq!(error["error"]["code"], "upstream_error");
            }
        }
        assert_eq!(
            fixture.seen.lock().unwrap().len(),
            1,
            "unrepresentable call signatures cannot be retried or silently discarded"
        );
        fixture.gateway.flush_diagnostics().await;
    }
}

#[tokio::test]
async fn ordinary_stop_tool_error_correction_and_final_history() {
    exercise_rejected(fixture::Api::GenerateContent, true).await;
    exercise_rejected(fixture::Api::GenerateContent, false).await;
}

#[tokio::test]
async fn interactions_tool_correction_and_signed_final_history() {
    exercise_rejected(fixture::Api::Interactions, true).await;
    exercise_rejected(fixture::Api::Interactions, false).await;
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
            let fixture = fixture::start(GenerationApi::Responses, api, false).await;
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
                        assert_eq!(stored.status(), 400);
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
            fixture.gateway.flush_diagnostics().await;
        }
    }
}
