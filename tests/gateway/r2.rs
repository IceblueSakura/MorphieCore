//! R2 native Router loop: changed tools/effort, exact thinking replay and cache TTL reports.
#[path = "../support/r2.rs"]
mod fixture;
use serde_json::{Value, json};

#[tokio::test]
async fn native_r2_keeps_changed_tools_effort_and_final_history() {
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
                .header("x-session-affinity", "synthetic-r2-session")
                // Ingress headers cannot select which beta carriers upstream receives.
                .header("anthropic-beta", "synthetic-do-not-forward")
                .json(&input)
                .send()
                .await
                .unwrap();
            assert_eq!(response.status(), 200);
            let body: Value = if stream {
                super::anthropic::messages_stream_body(&response.text().await.unwrap())
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
                fixture::advance(&mut input, body["content"].clone(), turn);
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
