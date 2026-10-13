//! Stored Responses over the real Router and a synthetic, stateless upstream.
use crate::{
    gateway::{Entry, Gateway, Limits, tests::support::KEY},
    protocol::openai::Profile,
    provider::{CredentialBindingId, SecretMaterial},
    topology::EndpointId,
};
use axum::{
    Json, Router,
    body::{Body, to_bytes},
    http::Request,
    response::{IntoResponse, Response},
    routing::post,
};
use futures_util::StreamExt;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::Duration,
};
use tower::ServiceExt;

fn gateway(origin: &str) -> Gateway {
    Gateway::new(
        super::topology::topology_at(origin),
        vec![Entry {
            model: "fixture-model".into(),
            protocol: Profile::Responses,
            endpoint: EndpointId::new("fixture-responses").unwrap(),
        }],
        BTreeMap::from([(
            CredentialBindingId::new("fixture-key").unwrap(),
            Arc::new(SecretMaterial::new("synthetic-upstream-credential-0001").unwrap()),
        )]),
        SecretMaterial::new(KEY).unwrap(),
        Limits {
            concurrency: 1,
            ..Default::default()
        },
        None,
    )
    .unwrap()
}
async fn send(gateway: &Gateway, method: &str, path: &str, body: Value) -> Response {
    tokio::time::timeout(
        Duration::from_secs(3),
        gateway.router().oneshot(
            Request::builder()
                .method(method)
                .uri(path)
                .header("authorization", format!("Bearer {KEY}"))
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        ),
    )
    .await
    .unwrap()
    .unwrap()
}
async fn value(response: Response) -> Value {
    assert_eq!(response.status(), 200);
    serde_json::from_slice(&to_bytes(response.into_body(), 1 << 20).await.unwrap()).unwrap()
}
fn created() -> Value {
    json!({"id":"wire-response","object":"response","model":"private-fixture","created_at":1,"status":"in_progress","output":[]})
}
fn frame(kind: &str, sequence: u32, field: &str, body: Value) -> String {
    let value = json!({"type":kind,"sequence_number":sequence,field:body});
    format!("event: {kind}\ndata: {value}\n\n")
}
fn completed_stream() -> String {
    // Independent wire oracle. Snapshot-only empty output is a valid completed response.
    let mut done = created();
    done["status"] = json!("completed");
    frame("response.created", 0, "response", created())
        + &frame("response.completed", 1, "response", done)
}

#[tokio::test]
async fn stored_json_and_sse_support_tool_continuation_retrieval_and_independent_branches() {
    let calls = Arc::new(Mutex::new(Vec::<Value>::new()));
    let observed = calls.clone();
    let upstream = Router::new().route("/responses", post(move |Json(body): Json<Value>| {
        let observed = observed.clone();
        async move {
            assert_eq!(body["store"], false);
            assert!(body.get("previous_response_id").is_none());
            let count = {
                let mut calls = observed.lock().unwrap();
                calls.push(body.clone());
                calls.len()
            };
            assert!(count <= 4);
            if body["stream"] == true {
                return ([("content-type", "text/event-stream")], completed_stream()).into_response();
            }
            let output = if count == 1 {
                json!([
                    {"type":"reasoning","id":"reasoning-one","summary":[{"type":"summary_text","text":"checking"}],"encrypted_content":"synthetic-opaque"},
                    {"type":"function_call","id":"call-one","call_id":"c","name":"lookup","arguments":"{}","status":"completed"}
                ])
            } else { json!([]) };
            Json(json!({"object":"response","id":"wire-response","model":"private-fixture",
                "created_at":1,"status":"completed","output":output})).into_response()
        }
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let gate = gateway(&format!("http://{}", listener.local_addr().unwrap()));
    let server = tokio::spawn(async move { axum::serve(listener, upstream).await.unwrap() });
    let first = value(
        send(
            &gate,
            "POST",
            "/v1/responses",
            json!({
                "model":"fixture-model","input":"first","instructions":"old"
            }),
        )
        .await,
    )
    .await;
    let parent = first["id"].as_str().unwrap();
    assert!(parent.starts_with("resp_mc_"));
    assert_eq!(first["store"], true);
    assert!(
        first["output"][0]["encrypted_content"]
            .as_str()
            .unwrap()
            .starts_with("mc1.")
    );
    assert_eq!(
        value(
            send(
                &gate,
                "GET",
                &format!("/v1/responses/{parent}"),
                Value::Null
            )
            .await
        )
        .await,
        first
    );

    let result = json!([{"type":"function_call_output","call_id":"c","output":"found"}]);
    let second = send(
        &gate,
        "POST",
        "/v1/responses",
        json!({
            "model":"fixture-model","previous_response_id":parent,"input":result,
            "instructions":"new","stream":true,
            "stream_options":{"include_obfuscation":false}
        }),
    )
    .await;
    assert_eq!(second.status(), 200);
    let wire = to_bytes(second.into_body(), 1 << 20).await.unwrap();
    let events: Vec<Value> = std::str::from_utf8(&wire)
        .unwrap()
        .lines()
        .filter_map(|line| line.strip_prefix("data: "))
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let terminal = &events.last().unwrap()["response"];
    let child = terminal["id"].as_str().unwrap();
    assert_eq!(events.last().unwrap()["type"], "response.completed");
    assert_ne!(parent, child);
    assert!(
        events
            .iter()
            .filter_map(|v| v.get("response"))
            .all(|v| v["id"] == child)
    );
    assert_eq!(terminal["previous_response_id"], parent);
    assert_eq!(
        value(send(&gate, "GET", &format!("/v1/responses/{child}"), Value::Null).await).await,
        *terminal
    );
    let page = value(
        send(
            &gate,
            "GET",
            &format!("/v1/responses/{child}/input_items?order=asc&limit=2"),
            Value::Null,
        )
        .await,
    )
    .await;
    assert_eq!(page["data"][0]["content"][0]["text"], "first");
    assert_eq!(page["data"][1]["type"], "reasoning");
    assert_eq!(page["has_more"], true);
    let tail = value(
        send(
            &gate,
            "GET",
            &format!(
                "/v1/responses/{child}/input_items?order=asc&after={}",
                page["last_id"].as_str().unwrap()
            ),
            Value::Null,
        )
        .await,
    )
    .await;
    assert_eq!(tail["data"][0]["call_id"], "c");
    assert_eq!(tail["data"][1]["output"], "found");

    let branch = value(
        send(
            &gate,
            "POST",
            "/v1/responses",
            json!({
                "model":"fixture-model","previous_response_id":parent,"input":result,"store":false
            }),
        )
        .await,
    )
    .await;
    assert_eq!(branch["store"], false);
    assert_eq!(
        send(
            &gate,
            "GET",
            &format!("/v1/responses/{}", branch["id"].as_str().unwrap()),
            Value::Null
        )
        .await
        .status(),
        404
    );
    let deleted = value(
        send(
            &gate,
            "DELETE",
            &format!("/v1/responses/{parent}"),
            Value::Null,
        )
        .await,
    )
    .await;
    assert_eq!(
        deleted,
        json!({"id":parent,"object":"response.deleted","deleted":true})
    );
    assert_eq!(
        send(
            &gate,
            "POST",
            "/v1/responses",
            json!({
                "model":"fixture-model","previous_response_id":parent,"input":"missing"
            })
        )
        .await
        .status(),
        404
    );
    value(
        send(
            &gate,
            "POST",
            "/v1/responses",
            json!({
                "model":"fixture-model","previous_response_id":child,"input":"next"
            }),
        )
        .await,
    )
    .await;
    let calls = calls.lock().unwrap();
    assert_eq!(calls.len(), 4);
    assert_eq!(
        calls[1]["input"][1]["encrypted_content"],
        "synthetic-opaque"
    );
    assert_eq!(calls[1]["input"][3]["output"], "found");
    assert_eq!(calls[1]["instructions"], "new");
    assert!(calls[2].get("instructions").is_none());
    assert_eq!(calls[2]["input"], calls[1]["input"]);
    assert!(calls[3].get("instructions").is_none());
    assert_eq!(calls[3]["input"].as_array().unwrap().len(), 5);
    drop(calls);
    gate.shutdown();
    server.abort();
}

#[tokio::test]
async fn abandoned_stored_stream_is_not_retrievable_and_auth_precedes_lookup() {
    let upstream = Router::new().route(
        "/responses",
        post(|| async {
            let opening = frame("response.created", 0, "response", created());
            let stream = futures_util::stream::once(async {
                Ok::<_, std::io::Error>(bytes::Bytes::from(opening))
            })
            .chain(futures_util::stream::pending());
            (
                [("content-type", "text/event-stream")],
                Body::from_stream(stream),
            )
                .into_response()
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let gate = gateway(&format!("http://{}", listener.local_addr().unwrap()));
    let server = tokio::spawn(async move { axum::serve(listener, upstream).await.unwrap() });
    let response = send(
        &gate,
        "POST",
        "/v1/responses",
        json!({
            "model":"fixture-model","input":"first","store":true,"stream":true
        }),
    )
    .await;
    assert_eq!(response.status(), 200);
    let mut stream = response.into_body().into_data_stream();
    let opening = stream.next().await.unwrap().unwrap();
    let event: Value = serde_json::from_str(
        std::str::from_utf8(&opening)
            .unwrap()
            .lines()
            .find_map(|line| line.strip_prefix("data: "))
            .unwrap(),
    )
    .unwrap();
    let id = event["response"]["id"].as_str().unwrap();
    drop(stream);
    let permit = tokio::time::timeout(
        Duration::from_secs(2),
        gate.state.permits.clone().acquire_owned(),
    )
    .await
    .unwrap()
    .unwrap();
    drop(permit);
    assert_eq!(
        send(&gate, "GET", &format!("/v1/responses/{id}"), Value::Null)
            .await
            .status(),
        404
    );
    assert_eq!(
        send(
            &gate,
            "POST",
            "/v1/responses",
            json!({
                "model":"fixture-model","previous_response_id":id,"input":"next"
            })
        )
        .await
        .status(),
        404
    );
    for method in ["GET", "DELETE"] {
        let response = gate
            .router()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(format!("/v1/responses/{id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), 401);
    }
    gate.shutdown();
    server.abort();
}
