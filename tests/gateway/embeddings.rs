//! Connected numeric delivery, EOF-gated failures and opt-in observation boundaries.
use super::embedding_loop::{CLIENT_KEY, Fixture};
use morphiecore::protocol::embedding::projection::EncodingSupport;
use serde_json::{Value, json};
#[tokio::test]
async fn embedding_encodings_and_effective_width() {
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
        let fixture = Fixture::start(support).await;
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
        assert_eq!(
            capabilities["data"][0]["embedding"]["usage_may_be_estimated"],
            true
        );
        assert!(
            capabilities["data"][0]["embedding"]
                .get("usage_may_be_omitted")
                .is_none()
        );
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
                .header("agent_id", "forged")
                .header("x-morphiecore-agent-id", "forged")
                .header("x-request-id", "forged")
                .header(
                    "traceparent",
                    "00-11111111111111111111111111111111-2222222222222222-01",
                )
                .header("tracestate", "synthetic=forged")
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
    }
}

#[tokio::test]
async fn embedding_default_vector_budget_rejects_before_transport() {
    let fixture = Fixture::start_with_dimensions(EncodingSupport::Float, 1536, 128).await;
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

#[tokio::test]
async fn google_embedding_projection_reports_errors_and_one_attempt() {
    let fixture = Fixture::start_google().await;
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap();
    let url = format!("{}/v1/embeddings", fixture.base);
    for encoding in ["float", "base64"] {
        let response = client.post(&url).bearer_auth(CLIENT_KEY).json(&json!({
            "model":"public-vector","input":["first","second"],"encoding_format":encoding,"user":"private-user"
        })).send().await.unwrap();
        assert_eq!(response.status(), 200);
        let result: Value = response.json().await.unwrap();
        assert_eq!(result["model"], "public-vector");
        assert_eq!(result["data"].as_array().unwrap().len(), 2);
        assert_eq!(result["data"][0]["index"], 0);
        assert_eq!(result["data"][1]["index"], 1);
        assert_eq!(result["usage"], json!({"prompt_tokens":7,"total_tokens":7}));
        assert_eq!(
            result["data"][0]["embedding"],
            if encoding == "float" {
                json!([0.1, -0.125])
            } else {
                json!("zczMPQAAAL4=")
            }
        );
    }
    for (text, tokens) in [("no-usage", 2), ("details-only", 3)] {
        let response = client
            .post(&url)
            .bearer_auth(CLIENT_KEY)
            .json(&json!({"model":"public-vector","input":text}))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let result: Value = response.json().await.unwrap();
        assert_eq!(
            result["usage"],
            json!({"prompt_tokens":tokens,"total_tokens":tokens})
        );
        assert_eq!(result["data"][0]["embedding"], json!([0.1, -0.125]));
    }
    for text in ["malformed-usage", "tensor", "partial"] {
        let response = client
            .post(&url)
            .bearer_auth(CLIENT_KEY)
            .json(&json!({"model":"public-vector","input":[text,"second"]}))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 502);
        let result: Value = response.json().await.unwrap();
        assert_eq!(result["error"]["code"], "upstream_error");
        assert!(result.get("data").is_none());
    }
    let response = client
        .post(&url)
        .bearer_auth(CLIENT_KEY)
        .json(&json!({"model":"public-vector","input":"rate-limit"}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 429);
    assert_eq!(response.headers()["retry-after"], "3");
    assert!(
        !response
            .text()
            .await
            .unwrap()
            .contains("private diagnostic")
    );
    let before = fixture.seen.lock().unwrap().len();
    assert_eq!(before, 8);
    let response = client
        .post(&url)
        .bearer_auth(CLIENT_KEY)
        .json(&json!({"model":"public-vector","input":[1,2]}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 400);
    assert_eq!(fixture.seen.lock().unwrap().len(), before);
}
