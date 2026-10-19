//! Responses phase/opaque history and stored/stateless continuation through the Router.
use crate::openai_tool_loop as fixture;
use morphiecore::protocol::openai::Profile;
use serde_json::{Value, json};

async fn request(client: &reqwest::Client, base: &str, body: &Value) -> Value {
    let response = client
        .post(format!("{base}/responses"))
        .bearer_auth(fixture::CLIENT_KEY)
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
        if event["type"] == "response.completed" {
            assert!(terminal.replace(event["response"].clone()).is_none());
        }
    }
    terminal.unwrap()
}

#[tokio::test]
async fn stored_and_stateless_tool_history_agree_in_json_and_sse() {
    let fixture = fixture::start(Profile::Responses, false).await;
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
                let response = request(&client, &fixture.base, &body).await;
                assert_eq!(
                    response["usage"]["total_tokens"], 18,
                    "stream={stream}, stored={stored}, turn={turn}: {response}"
                );
                assert_eq!(response["status"], "completed");
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
                    json!({"type":"function_call_output","call_id":"call-lookup","output":fixture::RESULT})
                } else {
                    json!({"role":"user","content":"next"})
                };
                if stored {
                    output_id = response["id"].clone();
                    body["previous_response_id"] = output_id.clone();
                    body["input"] = json!([next]);
                } else {
                    body["input"]
                        .as_array_mut()
                        .unwrap()
                        .extend(output.iter().cloned());
                    body["input"].as_array_mut().unwrap().push(next);
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
