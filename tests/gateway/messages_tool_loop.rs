//! Messages native Router loop: changed tools/effort, exact thinking replay and cache TTL reports.
use crate::messages_tool_fixture as fixture;
use serde_json::{Value, json};

#[tokio::test]
async fn native_messages_keep_changed_tools_effort_and_final_history() {
    let fixture = fixture::start().await;
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap();
    for stream in [false, true] {
        let mut input = fixture::input(stream);
        for turn in 1..=5 {
            let response = client
                .post(format!("{}/v1/messages", fixture.base))
                .header("x-api-key", fixture::KEY)
                .header("x-session-affinity", "synthetic-messages-session")
                // Ingress headers cannot select which beta carriers upstream receives.
                .header("anthropic-beta", "synthetic-do-not-forward")
                .json(&input)
                .send()
                .await
                .unwrap();
            assert_eq!(response.status(), 200);
            let body: Value = if stream {
                crate::messages_stream::messages_stream_body(&response.text().await.unwrap())
            } else {
                response.json().await.unwrap()
            };
            assert_eq!(
                body["usage"],
                json!({"input_tokens":11,"output_tokens":7,
                "cache_read_input_tokens":13,"cache_creation_input_tokens":17,
                "cache_creation":{"ephemeral_5m_input_tokens":0,"ephemeral_1h_input_tokens":17}})
            );
            let snapshot = fixture::snapshot(turn);
            assert_eq!(body["stop_reason"], snapshot["stop_reason"]);
            if turn == 1 || turn == 3 {
                assert_eq!(body["content"][0]["type"], "thinking");
                assert_eq!(
                    body["content"][0]["thinking"],
                    if turn == 1 {
                        "plan lookup"
                    } else {
                        "plan inspect"
                    }
                );
                let (_, _, signature) = morphiecore::protocol::replay_envelope::open(
                    body["content"][0]["signature"].as_str().unwrap(),
                )
                .unwrap();
                assert_eq!(
                    signature.as_str(),
                    if turn == 1 {
                        "native-lookup-signature"
                    } else {
                        "native-inspect-signature"
                    }
                );
                assert_eq!(
                    body["content"].as_array().unwrap().last(),
                    snapshot["content"].as_array().unwrap().last()
                );
                if turn == 1 {
                    let (_, _, data) = morphiecore::protocol::replay_envelope::open(
                        body["content"][1]["data"].as_str().unwrap(),
                    )
                    .unwrap();
                    assert_eq!(data.as_str(), "native-redacted");
                }
            } else {
                assert_eq!(body["content"], snapshot["content"]);
            }
            if turn < 5 {
                advance(&mut input, body["content"].clone(), turn);
            }
        }
    }
    let seen = fixture.seen.lock().unwrap();
    assert_eq!(seen.len(), 10);
    for (static_body, streamed_body) in seen[..5].iter().zip(&seen[5..]) {
        let mut static_body = static_body.clone();
        static_body["stream"] = json!(true);
        assert_eq!(&static_body, streamed_body);
    }
}

/// Client history is built only from delivered blocks, never restored native signatures.
fn advance(body: &mut Value, content: Value, turn: usize) {
    let messages = body["messages"].as_array_mut().unwrap();
    for message in messages.iter_mut() {
        if let Some(blocks) = message["content"].as_array_mut() {
            for block in blocks {
                block.as_object_mut().unwrap().remove("cache_control");
            }
        }
    }
    messages.push(json!({"role":"assistant","content":content}));
    let next = match turn {
        1 => {
            json!({"role":"user","content":[{"type":"tool_result","tool_use_id":"call-lookup","content":"A","is_error":false}]})
        }
        2 => json!({"role":"user","content":[{"type":"text","text":"inspect"}]}),
        3 => {
            json!({"role":"user","content":[{"type":"tool_result","tool_use_id":"call-inspect","content":"B","is_error":true}]})
        }
        4 => json!({"role":"user","content":[{"type":"text","text":"repeat"}]}),
        _ => panic!("client continuation"),
    };
    let mut next = next;
    next["content"][0]["cache_control"] = json!({"type":"ephemeral","ttl":"1h"});
    messages.push(next);
    if turn == 2 {
        messages.push(json!({"role":"system","content":[
            {"type":"text","text":"Inspect mode."},
            {"type":"tool_removal","tool":{"type":"tool_reference","name":"lookup"}},
            {"type":"tool_addition","tool":{"type":"tool_definition","definition":{
                "name":"inspect","input_schema":{"type":"object"},"eager_input_streaming":true}}}
        ],"output_config":{"effort":"low"}}));
    } else {
        messages.push(json!({"role":"system","content":[],"output_config":{"effort":if turn < 2 {"high"}else{"low"}}}));
    }
}
