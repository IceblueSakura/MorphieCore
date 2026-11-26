//! Decision SDK discovery and wire shapes through file-configured startup.
use axum::{Json, Router, http::HeaderMap, routing::post};
use morphiecore::gateway::startup::bootstrap::test_support::{Fixture, service};
use serde_json::{Value, json};
use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::net::TcpListener;

#[tokio::test]
#[ignore = "requires pinned OpenAI and TypeSafe Python SDKs; synthetic upstreams"]
async fn configured_decision_sdk_discovery_and_bound_calls() {
    let upstream = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let upstream_address = upstream.local_addr().unwrap();
    let upstream_root = format!("http://{upstream_address}/v1");
    let gemini_root = format!("http://{upstream_address}/v1beta");
    let calls = Arc::new(AtomicUsize::new(0));
    let openai = calls.clone();
    let typesafe = calls.clone();
    let gemini = calls.clone();
    let authority = Router::new()
        .route("/v1/decisions", post(move |headers: HeaderMap, Json(body): Json<Value>| {
            let calls = openai.clone();
            async move {
                calls.fetch_add(1, Ordering::Relaxed);
                assert_eq!(headers["authorization"], "Bearer synthetic-openai-decision");
                assert_eq!(body, json!({
                    "model":"gpt-6-luna",
                    "input":[{"type":"message","role":"user","content":[
                        {"type":"input_json","data":{"role":"system","item":"parcel"}}
                    ]}],
                    "questions":[{"type":"choice","name":"action","instructions":"Pick an action.",
                                  "choices":[{"value":true},{"value":"true"}]}]
                }));
                Json(json!({"model":"gpt-6-luna","answers":[{
                    "type":"choice","name":"action","choice":"true","confidence":0.41,
                    "probabilities":[{"value":"true","probability":0.8},{"value":true,"probability":0.2}]
                }],"usage":{"input_tokens":24,"output_tokens":0,"total_tokens":24,
                    "input_tokens_details":{"cached_tokens":0,"cache_write_tokens":0},
                    "output_tokens_details":{"reasoning_tokens":0}}}))
            }
        }))
        .route("/v1/systemone", post(move |headers: HeaderMap, Json(body): Json<Value>| {
            let calls = typesafe.clone();
            async move {
                calls.fetch_add(1, Ordering::Relaxed);
                assert_eq!(headers["authorization"], "Bearer synthetic-typesafe-decision");
                assert!(!headers.contains_key("content-encoding"));
                assert_eq!(body, json!({"model":"jev-1.14.0","state":{"record":"Synthetic parcel. ".repeat(100)},
                    "questions":{"priority":{"type":"score","criteria":["routine","urgent"]}}}));
                Json(json!({"model":"jev-1.14.0","answers":{"priority":{
                    "type":"score","score":0.8,"confidence":0.6,
                    "legend":{"1":"urgent","0":"routine"},"probabilities":{"1":0.8,"0":0.2}
                }},"usage":{"input_tokens":29,"output_tokens":0}}))
            }
        }))
        .route("/v1beta/decisions", post(move |headers: HeaderMap, Json(body): Json<Value>| {
            let calls = gemini.clone();
            async move {
                calls.fetch_add(1, Ordering::Relaxed);
                assert_eq!(headers["x-goog-api-key"], "synthetic-google-decision");
                assert!(!headers.contains_key("authorization"));
                assert!(!headers.contains_key("content-encoding"));
                assert_eq!(body["model"], "gemini-decision-1");
                if body["input"] == "blocked" {
                    return Json(json!({"answers":{},"prompt_feedback":{
                        "block_reason":"safety","block_reason_message":"must-not-reach-client"
                    }}));
                }
                assert_eq!(body, json!({
                    "model":"gemini-decision-1", "input":"Synthetic parcel. ".repeat(100),
                    "questions":{
                        "0":{"type":"noul","instructions":"Did it arrive?"},
                        "1":{"type":"noul","instructions":"Was it damaged?"}
                    }
                }));
                Json(json!({
                    "answers":{
                        "1":{"type":"noul","noul":0.08,"confidence":0.92},
                        "0":{"type":"noul","noul":0.95,"confidence":0.95}
                    },
                    "usage":{"total_input_tokens":71,"total_output_tokens":0,
                             "total_tokens":71,"input_tokens_by_modality":[{"modality":"text","tokens":71}]}
                }))
            }
        }));
    let _upstream = super::ServerGuard(
        tokio::spawn(async move { axum::serve(upstream, authority).await.unwrap() }).abort_handle(),
    );

    let fixture = Fixture::with_accounts(service(), json!({"models":{}, "decisions":{
        "gpt-6-luna":{"account":"primary"},
        "jev-1.14.0":{"account":"primary"},
        "gemini-decision-1":{"account":"primary"}
    }}), &[
        ("openai", "metered", "synthetic-openai-decision"),
        ("typesafe", "jev", "synthetic-typesafe-decision"),
        ("google", "gemini", "synthetic-google-decision"),
    ], json!({"version":1,"providers":{
        "openai":{"accounts":[{"name":"primary","enabled":true,"base_url":upstream_root,"credential":{"alias":"metered"}}]},
        "typesafe":{"accounts":[{"name":"primary","enabled":true,"base_url":upstream_root,"credential":{"alias":"jev"}}]},
        "google":{"accounts":[{"name":"primary","enabled":true,"base_url":gemini_root,"credential":{"alias":"gemini"}}]}
    }})).unwrap();
    let gateway = fixture.gateway.clone();
    let listener = TcpListener::bind(fixture.listen).await.unwrap();
    let root = format!("http://{}", listener.local_addr().unwrap());
    let _gateway = super::ServerGuard(
        tokio::spawn(async move {
            gateway
                .serve(listener, std::future::pending())
                .await
                .unwrap();
        })
        .abort_handle(),
    );

    let home = tempfile::tempdir().unwrap();
    let mut command = super::isolated_sdk_python(home.path());
    command.args(["tests/sdk/decisions.py", &root]);
    let output = super::child_process::run(&mut command, b"", Duration::from_secs(20), 8192)
        .await
        .unwrap();
    assert!(
        output.status.success(),
        "SDK failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        calls.load(Ordering::Relaxed),
        4,
        "discovery and intake rejections must not contact any upstream; blocked requests send once"
    );
}
