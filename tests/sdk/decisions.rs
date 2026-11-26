//! Native SDK discovery and differing wire shapes through file-configured startup.
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
#[ignore = "requires pinned OpenAI and TypeSafe Python SDKs; synthetic native upstreams"]
async fn configured_decision_sdk_discovery_and_native_calls() {
    let upstream = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let upstream_root = format!("http://{}/v1", upstream.local_addr().unwrap());
    let calls = Arc::new(AtomicUsize::new(0));
    let openai = calls.clone();
    let typesafe = calls.clone();
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
        }));
    let _upstream = super::ServerGuard(
        tokio::spawn(async move { axum::serve(upstream, authority).await.unwrap() }).abort_handle(),
    );

    let fixture = Fixture::with_accounts(service(), json!({"models":{}, "decisions":{
        "gpt-6-luna":{"account":"primary"},
        "jev-1.14.0":{"account":"primary"}
    }}), &[
        ("openai", "metered", "synthetic-openai-decision"),
        ("typesafe", "jev", "synthetic-typesafe-decision"),
    ], json!({"version":1,"providers":{
        "openai":{"accounts":[{"name":"primary","enabled":true,"base_url":upstream_root,"credential":{"alias":"metered"}}]},
        "typesafe":{"accounts":[{"name":"primary","enabled":true,"base_url":upstream_root,"credential":{"alias":"jev"}}]}
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
        2,
        "discovery must not contact the upstream"
    );
}
