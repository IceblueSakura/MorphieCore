//! Stored Responses over the real Router and a synthetic, stateless upstream.
use crate::{
    gateway::{Entry, Gateway, Limits, tests::support::KEY},
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
    gateway_with_limits(
        origin,
        Limits {
            concurrency: 1,
            ..Default::default()
        },
    )
}
fn gateway_with_limits(origin: &str, limits: Limits) -> Gateway {
    Gateway::new(
        super::topology::topology_at(origin),
        crate::gateway::TaskEntries {
            generation: vec![Entry {
                model: "fixture-model".into(),
                protocol: crate::protocol::ClientProtocol::Responses,
                endpoint: EndpointId::new("fixture-responses").unwrap(),
            }],
            ..Default::default()
        },
        BTreeMap::from([(
            CredentialBindingId::new("fixture-key").unwrap(),
            Arc::new(SecretMaterial::new("synthetic-upstream-credential-0001").unwrap()),
        )]),
        std::collections::BTreeMap::from([(
            "fixture-agent".into(),
            SecretMaterial::new(KEY).unwrap(),
        )]),
        limits,
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
async fn stateful_requests_reject_without_dispatch_or_history_access() {
    use crate::protocol::openai::{Dialect, GenerationApi, GenerationCodec};
    use std::sync::atomic::{AtomicUsize, Ordering};
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = calls.clone();
    let upstream = Router::new().route(
        "/responses",
        post(move |Json(body): Json<Value>| {
            assert_eq!(body["store"], false);
            assert!(body.get("previous_response_id").is_none());
            observed.fetch_add(1, Ordering::SeqCst);
            async { ([("content-type", "text/event-stream")], completed_stream()) }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let gate = gateway(&format!("http://{}", listener.local_addr().unwrap()));
    let server = tokio::spawn(async move { axum::serve(listener, upstream).await.unwrap() });
    let codec = GenerationCodec::new(GenerationApi::Responses, Dialect::Standard, None);
    let mut snapshot = created();
    snapshot["id"] = json!("resp_seeded");
    snapshot["status"] = json!("completed");
    let body = bytes::Bytes::from(snapshot.to_string());
    gate.state
        .history
        .stage(
            crate::gateway::generation::history::Snapshot {
                id: "resp_seeded".into(),
                input: codec
                    .decode_client_request(br#"{"model":"fixture-model","input":"seed"}"#)
                    .unwrap()
                    .task,
                response: codec.decode_response(&body).unwrap(),
                weight: body.len(),
                body,
            },
            tokio::time::Instant::now(),
        )
        .unwrap()
        .publish(tokio::time::Instant::now());
    for stream in [false, true] {
        for controls in [
            json!({"store":true}),
            json!({"previous_response_id":"resp_missing"}),
            json!({"store":false,"previous_response_id":"resp_missing"}),
            json!({"store":false,"previous_response_id":"resp_seeded"}),
        ] {
            let mut body = json!({"model":"fixture-model","input":"next","stream":stream});
            body.as_object_mut()
                .unwrap()
                .extend(controls.as_object().unwrap().clone());
            let response = send(&gate, "POST", "/v1/responses", body).await;
            assert_eq!(response.status(), 400);
            let error: Value =
                serde_json::from_slice(&to_bytes(response.into_body(), 4096).await.unwrap())
                    .unwrap();
            assert_eq!(error["error"]["code"], "stateful_api_disabled");
        }
    }
    for id in ["resp_missing", "resp_seeded"] {
        for (method, path) in [
            ("GET", format!("/v1/responses/{id}")),
            ("HEAD", format!("/v1/responses/{id}")),
            ("DELETE", format!("/v1/responses/{id}")),
            ("GET", format!("/v1/responses/{id}/input_items?limit=1")),
            ("HEAD", format!("/v1/responses/{id}/input_items")),
        ] {
            let unauthorized = gate
                .router()
                .oneshot(
                    Request::builder()
                        .method(method)
                        .uri(&path)
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(unauthorized.status(), 401);
            let rejected = send(&gate, method, &path, Value::Null).await;
            assert_eq!(rejected.status(), 400);
            if method != "HEAD" {
                let error: Value =
                    serde_json::from_slice(&to_bytes(rejected.into_body(), 4096).await.unwrap())
                        .unwrap();
                assert_eq!(error["error"]["code"], "stateful_api_disabled");
            }
        }
    }
    assert!(
        gate.state
            .history
            .get("resp_seeded", tokio::time::Instant::now())
            .is_ok()
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    for stream in [false, true] {
        for controls in [
            json!({}),
            json!({"store":false}),
            json!({"store":null,"previous_response_id":null}),
        ] {
            let mut body = json!({"model":"fixture-model","input":"next","stream":stream});
            body.as_object_mut()
                .unwrap()
                .extend(controls.as_object().unwrap().clone());
            let response = send(&gate, "POST", "/v1/responses", body).await;
            assert_eq!(response.status(), 200);
            let bytes = to_bytes(response.into_body(), 1 << 20).await.unwrap();
            let response: Value = if stream {
                std::str::from_utf8(&bytes)
                    .unwrap()
                    .lines()
                    .filter_map(|line| line.strip_prefix("data: "))
                    .map(|data| serde_json::from_str::<Value>(data).unwrap())
                    .find(|event| event["type"] == "response.completed")
                    .unwrap()["response"]
                    .clone()
            } else {
                serde_json::from_slice(&bytes).unwrap()
            };
            assert_eq!(response["store"], false);
            assert!(response["previous_response_id"].is_null());
            assert!(
                gate.state
                    .history
                    .get(
                        response["id"].as_str().unwrap(),
                        tokio::time::Instant::now()
                    )
                    .is_err()
            );
        }
    }
    assert_eq!(calls.load(Ordering::SeqCst), 6);
    gate.shutdown();
    server.abort();
}

#[tokio::test]
#[ignore = "experimental stateful API; awaiting explicit activation"]
async fn experimental_unpolled_snapshot_operations_do_not_hold_inference_capacity() {
    let upstream = Router::new().route(
        "/responses",
        post(|| async { ([("content-type", "text/event-stream")], completed_stream()) }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let gate = gateway(&format!("http://{}", listener.local_addr().unwrap()));
    let server = tokio::spawn(async move { axum::serve(listener, upstream).await.unwrap() });
    let stored = value(
        send(
            &gate,
            "POST",
            "/v1/responses",
            json!({"model":"fixture-model","input":"first","store":true}),
        )
        .await,
    )
    .await;
    let id = stored["id"].as_str().unwrap();
    let mut held = Vec::new();
    for (method, path) in [
        ("GET", format!("/v1/responses/{id}")),
        ("GET", format!("/v1/responses/{id}/input_items")),
        ("DELETE", format!("/v1/responses/{id}")),
    ] {
        let response = send(&gate, method, &path, Value::Null).await;
        assert_eq!(response.status(), 200);
        held.push(response);
        assert_eq!(gate.state.permits.available_permits(), 1);
    }
    assert_eq!(
        send(
            &gate,
            "POST",
            "/v1/responses",
            json!({"model":"fixture-model","input":"next"})
        )
        .await
        .status(),
        200
    );
    gate.shutdown();
    server.abort();
}

#[tokio::test(start_paused = true)]
async fn total_deadline_includes_upload_and_route_tightening_without_dispatch() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = calls.clone();
    let upstream = Router::new().route(
        "/responses",
        post(move || {
            observed.fetch_add(1, Ordering::SeqCst);
            async { ([("content-type", "text/event-stream")], completed_stream()) }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move { axum::serve(listener, upstream).await.unwrap() });
    for (upload_ms, route_ms) in [(200, None), (60, Some(50))] {
        let mut gate = gateway_with_limits(
            &origin,
            Limits {
                concurrency: 1,
                exchange_timeout: Duration::from_millis(100),
                body_timeout: Duration::from_secs(5),
                ..Default::default()
            },
        );
        let state = Arc::get_mut(&mut gate.state).unwrap();
        for entry in state.entries.values_mut() {
            Arc::make_mut(entry).policy.total_timeout_ms = route_ms;
        }
        let entered = Arc::new(tokio::sync::Notify::new());
        let release = Arc::new(tokio::sync::Notify::new());
        let upload_entered = entered.clone();
        let upload_release = release.clone();
        let body = Body::from_stream(futures_util::stream::once(async move {
            upload_entered.notify_one();
            upload_release.notified().await;
            Ok::<_, std::io::Error>(bytes::Bytes::from_static(
                br#"{"model":"fixture-model","input":"first"}"#,
            ))
        }));
        let router = gate.router();
        let request = tokio::spawn(async move {
            router
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/v1/responses")
                        .header("authorization", format!("Bearer {KEY}"))
                        .header("content-type", "application/json")
                        .body(body)
                        .unwrap(),
                )
                .await
                .unwrap()
        });
        entered.notified().await;
        tokio::time::advance(Duration::from_millis(upload_ms)).await;
        release.notify_one();
        let response = request.await.unwrap();
        assert_eq!(response.status(), 504);
        assert_eq!(gate.state.permits.available_permits(), 1);
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        gate.shutdown();
    }
    server.abort();
}

#[tokio::test]
#[ignore = "experimental stateful API; awaiting explicit activation"]
async fn experimental_stored_json_and_sse_support_tool_continuation_retrieval_and_independent_branches()
 {
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
            assert_eq!(body["stream"], true);
            let output = if count == 1 {
                json!([
                    {"type":"reasoning","id":"reasoning-one","summary":[{"type":"summary_text","text":"checking"}],"encrypted_content":"synthetic-opaque"},
                    {"type":"function_call","id":"call-one","call_id":"c","name":"lookup","arguments":"{}","status":"completed"}
                ])
            } else { json!([]) };
            ([("content-type", "text/event-stream")], super::responses_wire::response_stream(json!({"object":"response","id":"wire-response","model":"private-fixture",
                "created_at":1,"status":"completed","output":output}))).into_response()
        }
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let gate = gateway(&format!("http://{}", listener.local_addr().unwrap()));
    let server = tokio::spawn(async move { axum::serve(listener, upstream).await.unwrap() });
    let first = send(
        &gate,
        "POST",
        "/v1/responses",
        json!({
            "model":"fixture-model","input":"first","instructions":"old"
        }),
    )
    .await;
    assert_eq!(first.status(), 200);
    let first_body = to_bytes(first.into_body(), 1 << 20).await.unwrap();
    let first: Value = serde_json::from_slice(&first_body).unwrap();
    let parent = first["id"].as_str().unwrap();
    assert!(parent.starts_with("resp_mc_"));
    assert_eq!(first["store"], true);
    assert!(
        first["output"][0]["encrypted_content"]
            .as_str()
            .unwrap()
            .starts_with("mc1.")
    );
    let retrieved = send(
        &gate,
        "GET",
        &format!("/v1/responses/{parent}"),
        Value::Null,
    )
    .await;
    assert_eq!(retrieved.status(), 200);
    let retrieved_body = to_bytes(retrieved.into_body(), 1 << 20).await.unwrap();
    assert_eq!(retrieved_body, first_body);
    assert_eq!(
        serde_json::from_slice::<Value>(&retrieved_body).unwrap(),
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
    let mut retrieved =
        value(send(&gate, "GET", &format!("/v1/responses/{child}"), Value::Null).await).await;
    let mut standard_terminal = terminal.clone();
    retrieved.as_object_mut().unwrap().remove("morphiecore");
    standard_terminal
        .as_object_mut()
        .unwrap()
        .remove("morphiecore");
    assert_eq!(retrieved, standard_terminal);
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
async fn abandoned_stateless_stream_releases_capacity_and_auth_precedes_disabled_resources() {
    let upstream = Router::new().route(
        "/responses",
        post(|| async {
            let opening = [
                frame("response.created", 0, "response", created()),
                "data: {\"type\":\"response.output_item.added\",\"sequence_number\":1,\"output_index\":0,\"item\":{\"type\":\"message\",\"id\":\"msg\",\"role\":\"assistant\",\"status\":\"in_progress\",\"content\":[]}}\n\n".into(),
                "data: {\"type\":\"response.content_part.added\",\"sequence_number\":2,\"output_index\":0,\"item_id\":\"msg\",\"content_index\":0,\"part\":{\"type\":\"output_text\",\"text\":\"\",\"annotations\":[]}}\n\n".into(),
                "data: {\"type\":\"response.output_text.delta\",\"sequence_number\":3,\"output_index\":0,\"item_id\":\"msg\",\"content_index\":0,\"delta\":\"visible\",\"logprobs\":[]}\n\n".into()
            ].concat();
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
            "model":"fixture-model","input":"first","store":false,"stream":true
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
        400
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
        400
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

#[tokio::test]
async fn timed_out_unpolled_body_releases_capacity_without_consumer_cleanup() {
    let upstream = Router::new().route(
        "/responses",
        post(|| async { ([("content-type", "text/event-stream")], completed_stream()) }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let gate = gateway_with_limits(
        &format!("http://{}", listener.local_addr().unwrap()),
        Limits {
            concurrency: 1,
            history_entries: 1,
            exchange_timeout: Duration::from_millis(200),
            ..Default::default()
        },
    );
    let server = tokio::spawn(async move { axum::serve(listener, upstream).await.unwrap() });
    let request = json!({"model":"fixture-model","input":"first","store":false});
    let unpolled = send(&gate, "POST", "/v1/responses", request.clone()).await;
    assert_eq!(unpolled.status(), 200);
    // Producer completion, not a body poll, must release admission capacity.
    let permit = tokio::time::timeout(
        Duration::from_secs(2),
        gate.state.permits.clone().acquire_owned(),
    )
    .await
    .unwrap()
    .unwrap();
    drop(permit);
    let next = send(&gate, "POST", "/v1/responses", request).await;
    assert_eq!(next.status(), 200);
    let mut body = next.into_body().into_data_stream();
    let bytes = body.next().await.unwrap().unwrap();
    // Final handoff wins: abort immediately, before the producer resumes.
    drop(body);
    let next: Value = serde_json::from_slice(&bytes).unwrap();
    let id = next["id"].as_str().unwrap();
    assert!(
        gate.state
            .history
            .get(id, tokio::time::Instant::now())
            .is_err()
    );
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
        400
    );
    assert!(to_bytes(unpolled.into_body(), 1 << 20).await.is_err());
    // Neither completed nor timed-out delivery activates experimental storage.
    assert_eq!(
        send(&gate, "GET", &format!("/v1/responses/{id}"), Value::Null)
            .await
            .status(),
        400
    );
    gate.shutdown();
    server.abort();
}
