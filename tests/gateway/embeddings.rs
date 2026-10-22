//! Connected numeric delivery, EOF-gated failures and opt-in observation boundaries.
use super::{
    embedding_loop::{CLIENT_KEY, Fixture},
    filesystem,
};
use morphiecore::lowering::embeddings::EncodingSupport;
use serde_json::{Value, json};
#[tokio::test]
async fn embedding_encodings_receipts_capture_and_effective_width() {
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap();
    for support in [
        EncodingSupport::Float,
        EncodingSupport::Base64,
        EncodingSupport::Both,
    ] {
        let dir = filesystem::private_directory();
        let capture_path = dir.path().join("capture.jsonl");
        let diagnostic_path = dir.path().join("diagnostics.jsonl");
        let fixture = Fixture::start(support, Some(&capture_path), Some(&diagnostic_path)).await;
        let capabilities: Value = client
            .get(format!("{}/v1/morphiecore/capabilities", fixture.base))
            .bearer_auth(CLIENT_KEY)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(capabilities["data"].as_array().unwrap().len(), 1);
        assert_eq!(
            capabilities["data"][0]["embedding"]["default_dimensions"],
            2
        );
        assert_eq!(capabilities["data"][0]["delivery_modes"], json!(["json"]));
        for (ordinal, encoding) in [None, Some("float"), Some("base64")]
            .into_iter()
            .enumerate()
        {
            let mut request = json!({"model":"public-vector","input":["first","second"]});
            if let Some(encoding) = encoding {
                request["encoding_format"] = json!(encoding);
            }
            let response = client
                .post(format!("{}/v1/embeddings", fixture.base))
                .bearer_auth(CLIENT_KEY)
                .header("x-never-forward", "synthetic-private")
                .json(&request)
                .send()
                .await
                .unwrap();
            assert_eq!(response.status(), 200);
            assert_eq!(response.headers()["x-morphiecore-adaptation"], "possible");
            assert_eq!(
                response.headers()["link"],
                "</v1/morphiecore/capabilities>; rel=\"describedby\""
            );
            let value: Value = response.json().await.unwrap();
            let expected = if encoding == Some("base64") {
                json!("zczMPQAAAL4=")
            } else {
                json!([
                    if support != EncodingSupport::Base64 {
                        0.1
                    } else {
                        0.10000000149011612
                    },
                    -0.125
                ])
            };
            assert_eq!(
                value,
                json!({"object":"list","model":"reported-vector","data":[
                {"object":"embedding","index":1,"embedding":expected},
                {"object":"embedding","index":0,"embedding":expected}],"usage":{"prompt_tokens":7,"total_tokens":7}})
            );
            assert_eq!(fixture.seen.lock().unwrap().len(), ordinal + 1);
        }
        // These complete native responses fail before publication, never become empty successes.
        for input in ["wrong-width", "underflow"] {
            if input == "underflow" && support != EncodingSupport::Float {
                continue;
            }
            let response = client
                .post(format!("{}/v1/embeddings", fixture.base))
                .bearer_auth(CLIENT_KEY)
                .json(&json!({"model":"public-vector","input":input,"encoding_format":"base64"}))
                .send()
                .await
                .unwrap();
            assert_eq!(response.status(), 502);
            assert!(!response.headers().contains_key("x-morphiecore-adaptation"));
            let body: Value = response.json().await.unwrap();
            assert_eq!(body["error"]["code"], "upstream_error");
            assert!(body.get("data").is_none());
        }
        let before = fixture.seen.lock().unwrap().len();
        let response = client
            .post(format!("{}/v1/embeddings", fixture.base))
            .bearer_auth(CLIENT_KEY)
            .json(&json!({"model":"public-vector","input":vec!["x";5]}))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 400);
        assert!(!response.headers().contains_key("x-morphiecore-adaptation"));
        assert_eq!(fixture.seen.lock().unwrap().len(), before);
        fixture.gateway.flush_diagnostics().await;
        let diagnostics = std::fs::read_to_string(diagnostic_path).unwrap();
        let records: Vec<Value> = diagnostics
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        let candidates: Vec<_> = records
            .iter()
            .filter_map(|r| r["candidates"].as_array())
            .flatten()
            .collect();
        assert_eq!(
            candidates.len(),
            if support == EncodingSupport::Float {
                5
            } else {
                4
            }
        );
        for (index, candidate) in candidates.iter().take(3).enumerate() {
            let mut expected =
                vec![json!({"rule":"embedding.omit-response-id.v1","direction":"delivery"})];
            if index == 2 && support == EncodingSupport::Float {
                expected.push(
                    json!({"rule":"embedding.float-json-to-binary32.v1","direction":"delivery"}),
                );
            }
            assert_eq!(candidate["embedding_receipts"], json!(expected));
        }
        assert!(
            candidates
                .iter()
                .skip(3)
                .all(|candidate| candidate.get("embedding_receipts").is_none())
        );
        assert!(
            !diagnostics.contains("\"first\"")
                && !diagnostics.contains("zczMPQ")
                && !diagnostics.contains("synthetic-id")
        );
        let capture = std::fs::read_to_string(capture_path).unwrap();
        assert!(capture.contains("[REDACTED]"));
        assert!(
            !capture.contains(CLIENT_KEY)
                && !capture.contains("synthetic-embedding-upstream-key-0001")
        );
        let captures: Vec<Value> = capture
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        let delivery = captures
            .iter()
            .find(|r| {
                r["boundary"] == "client_response"
                    && r["value"]["data"][0]["embedding"] == "zczMPQAAAL4="
            })
            .unwrap();
        let exchange = &delivery["exchange"];
        for boundary in [
            "client_request",
            "upstream_request",
            "upstream_response",
            "client_response",
        ] {
            assert!(captures.iter().any(|r| &r["exchange"] == exchange
                && r["boundary"] == boundary
                && r["status"] == "body_unit"));
        }
        let upstream = captures
            .iter()
            .find(|r| {
                &r["exchange"] == exchange
                    && r["boundary"] == "upstream_request"
                    && r["status"] == "body_unit"
            })
            .unwrap();
        assert_eq!(upstream["attempt"], 1);
        assert_eq!(
            upstream["value"]["encoding_format"],
            if support == EncodingSupport::Float {
                "float"
            } else {
                "base64"
            }
        );
    }
}

#[tokio::test]
async fn embedding_default_vector_budget_rejects_before_transport() {
    let fixture =
        Fixture::start_with_dimensions(EncodingSupport::Float, 1536, 128, None, None).await;
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap();
    for dimensions in [None, Some(1536)] {
        let mut request = json!({"model":"public-vector","input":vec!["x";40]});
        if let Some(d) = dimensions {
            request["dimensions"] = json!(d);
        }
        let response = client
            .post(format!("{}/v1/embeddings", fixture.base))
            .bearer_auth(CLIENT_KEY)
            .json(&request)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 400);
        assert!(fixture.seen.lock().unwrap().is_empty());
    }
    let response = client
        .post(format!("{}/v1/embeddings", fixture.base))
        .bearer_auth(CLIENT_KEY)
        .json(&json!({"model":"public-vector","input":vec!["x";39]}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let output: Value = response.json().await.unwrap();
    assert_eq!(output["data"].as_array().unwrap().len(), 39);
    assert_eq!(
        output["data"][0]["embedding"].as_array().unwrap().len(),
        1536
    );
    assert_eq!(fixture.seen.lock().unwrap().len(), 1);
    assert!(fixture.seen.lock().unwrap()[0].get("dimensions").is_none());
}
