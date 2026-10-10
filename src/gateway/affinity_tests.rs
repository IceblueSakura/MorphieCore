//! One bounded Router gate for exact prefixes, final handoff and both cache carriers.
use super::*;
use crate::{
    adapter::Dialect, lowering::generation::GenerationRepresentationContract as Contract,
    provider::*, topology::*,
};
use axum::{
    Json, Router,
    body::{Body, to_bytes},
    http::{HeaderMap, Request as HttpRequest},
    response::IntoResponse,
    routing::post,
};
use serde_json::{Value, json};
use std::{sync::Mutex, time::Duration};
use tower::ServiceExt;

struct Listener(tokio::task::JoinHandle<()>);
impl Drop for Listener {
    fn drop(&mut self) {
        self.0.abort();
    }
}
fn response(body: &Value, chat: bool) -> axum::response::Response {
    if body["metadata"]["mode"] == "error" {
        return axum::http::StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    let call = body.get("tools").is_some()
        && !body.to_string().contains("function_call_output")
        && !body.to_string().contains("tool_call_id");
    let output = if call {
        json!({"type":"function_call","id":"fc","call_id":"call","name":"lookup","arguments":"{\"n\":1}","status":"completed"})
    } else {
        json!({"type":"message","id":"msg","role":"assistant","status":"completed","content":[{"type":"output_text","text":"ok","annotations":[],"logprobs":[]}]})
    };
    let message = if call {
        json!({"role":"assistant","content":null,"tool_calls":[{"type":"function","id":"call","function":{"name":"lookup","arguments":"{\"n\":1}"}}]})
    } else {
        json!({"role":"assistant","content":"ok"})
    };
    if chat {
        let finish = if call { "tool_calls" } else { "stop" };
        if body["stream"] == true {
            let chunk = |delta: Value, finish: Value| json!({"object":"chat.completion.chunk","id":"r","model":"upstream","created":1,"choices":[{"index":0,"delta":delta,"finish_reason":finish}]});
            let delta = if call {
                json!({"tool_calls":[{"index":0,"type":"function","id":"call","function":{"name":"lookup","arguments":"{\"n\":1}"}}]})
            } else {
                json!({"content":"ok"})
            };
            let chunks = [
                chunk(json!({"role":"assistant","content":null}), Value::Null),
                chunk(delta, Value::Null),
                chunk(json!({}), json!(finish)),
            ];
            let mut wire: String = chunks.iter().map(|v| format!("data: {v}\n\n")).collect();
            if body["metadata"]["mode"] != "truncated" {
                wire.push_str("data: [DONE]\n\n");
            }
            return ([("content-type", "text/event-stream")], wire).into_response();
        }
        return Json(json!({"object":"chat.completion","id":"r","model":"upstream","created":1,"choices":[{"index":0,"message":message,"finish_reason":finish}]})).into_response();
    }
    let final_value = json!({"object":"response","id":"r","model":"upstream","created_at":1,"status":"completed","output":[output]});
    if body["stream"] != true {
        return Json(final_value).into_response();
    }
    let mut opening = final_value.clone();
    opening["status"] = json!("in_progress");
    opening["output"] = json!([]);
    let mut added = output.clone();
    added["status"] = json!("in_progress");
    if call {
        added["arguments"] = json!("");
    } else {
        added["content"] = json!([]);
    }
    let id = output["id"].as_str().unwrap();
    let mut events = vec![
        json!({"type":"response.created","response":opening}),
        json!({"type":"response.output_item.added","output_index":0,"item":added}),
    ];
    if call {
        events.extend([
            json!({"type":"response.function_call_arguments.delta","output_index":0,"item_id":id,"delta":"{\"n\":1}"}),
            json!({"type":"response.function_call_arguments.done","output_index":0,"item_id":id,"arguments":"{\"n\":1}"}),
        ]);
    } else {
        events.extend([
            json!({"type":"response.content_part.added","output_index":0,"item_id":id,"content_index":0,"part":{"type":"output_text","text":"","annotations":[]}}),
            json!({"type":"response.output_text.delta","output_index":0,"item_id":id,"content_index":0,"delta":"ok","logprobs":[]}),
            json!({"type":"response.output_text.done","output_index":0,"item_id":id,"content_index":0,"text":"ok","logprobs":[]}),
            json!({"type":"response.content_part.done","output_index":0,"item_id":id,"content_index":0,"part":output["content"][0]}),
        ]);
    }
    events.push(json!({"type":"response.output_item.done","output_index":0,"item":output}));
    if body["metadata"]["mode"] != "truncated" {
        events.push(json!({"type":"response.completed","response":final_value}));
    }
    let wire: String = events
        .into_iter()
        .enumerate()
        .map(|(n, mut v)| {
            v["sequence_number"] = json!(n);
            format!("event: {}\ndata: {v}\n\n", v["type"].as_str().unwrap())
        })
        .collect();
    ([("content-type", "text/event-stream")], wire).into_response()
}
fn gateway(origin: &str, profile: Profile, dialect: Dialect) -> Gateway {
    let provider = ProviderDefinition {
        id: ProviderId::new("fixture").unwrap(),
        origin: TrustedOrigin::parse(origin).unwrap(),
        chat_completions: Some(EndpointPath::new("/upstream").unwrap()),
        responses: Some(EndpointPath::new("/upstream").unwrap()),
        messages: None,
        auth: AuthScheme::Bearer,
    };
    let endpoint = Endpoint {
        id: EndpointId::new("endpoint").unwrap(),
        provider: provider.id.clone(),
        target: EndpointTarget {
            origin: provider.origin.clone(),
            path: provider.responses.clone().unwrap(),
        },
        task: TaskKind::Generation,
        protocol: if profile == Profile::Chat {
            ProtocolProfile::OpenAiChat
        } else {
            ProtocolProfile::OpenAiResponses
        },
        upstream_model: "upstream".into(),
        canonical_model: ModelId::new("canonical").unwrap(),
        representation: Adapter::new(profile, dialect, None).contract(&Contract::full()),
        execution: ExecutionContract {
            streaming: true,
            retry_before_commit: false,
            request_body_limit: 256 << 10,
            response_body_limit: 1 << 20,
            timeout_ms: 2000,
            credential_kind: CredentialKind::ApiKey,
        },
        credential: CredentialBindingId::new("key").unwrap(),
    };
    let topology = compile(
        vec![provider],
        vec![endpoint],
        vec![Route {
            id: RouteId::new("route").unwrap(),
            task: TaskKind::Generation,
            endpoints: vec![EndpointId::new("endpoint").unwrap()],
            policy: RoutePolicy::default(),
        }],
        vec![PublicModel::new(
            ModelId::new("public").unwrap(),
            ModelId::new("canonical").unwrap(),
            TaskKind::Generation,
            RouteId::new("route").unwrap(),
            GenerationSemanticContract::full(),
        )],
        vec![CanonicalModel {
            id: ModelId::new("canonical").unwrap(),
            task: TaskKind::Generation,
            contract: GenerationSemanticContract::full(),
        }],
    )
    .unwrap()
    .with_model_metadata([(
        ModelId::new("canonical").unwrap(),
        ModelMetadata::new(1, "Synthetic Developer").unwrap(),
    )])
    .unwrap();
    Gateway::new(
        topology,
        vec![Entry {
            model: "public".into(),
            protocol: profile,
            endpoint: EndpointId::new("endpoint").unwrap(),
        }],
        BTreeMap::from([(
            CredentialBindingId::new("key").unwrap(),
            Arc::new(SecretMaterial::new("synthetic-upstream").unwrap()),
        )]),
        SecretMaterial::new(tests::KEY).unwrap(),
        Limits::default(),
        None,
    )
    .unwrap()
}
async fn send(gate: &Gateway, profile: Profile, body: &Value) -> Result<bytes::Bytes, axum::Error> {
    let mut body = body.clone();
    if body["stream"] != true {
        body.as_object_mut().unwrap().shift_remove("stream_options");
    }
    let path = if profile == Profile::Chat {
        "/v1/chat/completions"
    } else {
        "/v1/responses"
    };
    let response = gate
        .router()
        .oneshot(
            HttpRequest::builder()
                .method("POST")
                .uri(path)
                .header("authorization", format!("Bearer {}", tests::KEY))
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    to_bytes(response.into_body(), 1 << 20).await
}

#[tokio::test]
async fn implicit_affinity_preserves_full_requests_and_only_publishes_complete_turns() {
    tokio::time::timeout(Duration::from_secs(30), async {
        let observed = Arc::new(Mutex::new(Vec::<(HeaderMap,Value)>::new()));
        let copy = observed.clone();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}",listener.local_addr().unwrap());
        let router = Router::new().route("/upstream",post(move |headers:HeaderMap,Json(body):Json<Value>| {
            copy.lock().unwrap().push((headers,body.clone()));
            async move { response(&body,body.get("messages").is_some()) }
        }));
        let _listener = Listener(tokio::spawn(async move { axum::serve(listener,router).await.unwrap(); }));
        for (profile,dialect) in [(Profile::Responses,Dialect::Standard),(Profile::Chat,Dialect::OpenCodeGo)] {
            let gate = gateway(&origin,profile,dialect);
            let field = if profile == Profile::Chat { "messages" } else { "input" };
            for (tools,first_stream) in [(false,false),(true,true)] {
                let mut body = json!({"model":"public","stream":first_stream,
                    "stream_options":{"include_obfuscation":false},"max_output_tokens":32});
                if profile == Profile::Chat {
                    body.as_object_mut().unwrap().shift_remove("max_output_tokens");
                    body["max_completion_tokens"] = json!(32);
                }
                body[field] = json!([{"role":"user","content":if tools { "use lookup" } else { "hello" }}]);
                if tools { body["tools"] = if profile == Profile::Chat {
                    json!([{"type":"function","function":{"name":"lookup","parameters":{"type":"object"},"strict":false}}])
                } else { json!([{"type":"function","name":"lookup","parameters":{"type":"object"},"strict":false}]) }; }
                send(&gate,profile,&body).await.unwrap();
                let (head,wire) = observed.lock().unwrap().last().unwrap().clone();
                let carrier = if profile == Profile::Chat { json!(head["x-opencode-session"].to_str().unwrap()) } else { wire["prompt_cache_key"].clone() };
                let output = if tools {
                    if profile == Profile::Chat { json!({"role":"assistant","content":null,"tool_calls":[{"id":"call","type":"function","function":{"name":"lookup","arguments":"{\"n\":1}"}}]}) }
                    else { json!({"type":"function_call","id":"fc","call_id":"call","name":"lookup","arguments":"{\"n\":1}","status":"completed"}) }
                } else if profile == Profile::Chat { json!({"role":"assistant","content":"ok"}) }
                    else { json!({"type":"message","id":"msg","role":"assistant","status":"completed","content":[{"type":"output_text","text":"ok","annotations":[],"logprobs":[]}]}) };
                body[field].as_array_mut().unwrap().push(output);
                body[field].as_array_mut().unwrap().push(if tools {
                    if profile == Profile::Chat { json!({"role":"tool","tool_call_id":"call","content":"1"}) }
                    else { json!({"type":"function_call_output","call_id":"call","output":"1"}) }
                } else { json!({"role":"user","content":"continue"}) });
                body["stream"] = json!(!first_stream);
                send(&gate,profile,&body).await.unwrap();
                let (next_head,next) = observed.lock().unwrap().last().unwrap().clone();
                assert!(next_head.get("x-morphiecore-conversation-id").is_none());
                assert_eq!(next[field].as_array().unwrap().len(), 3);
                assert_eq!(next["tools"],body["tools"]);
                assert_eq!(next["model"],"upstream");
                assert!(next.get("session_id").is_none());
                assert_eq!(carrier,if profile == Profile::Chat { json!(next_head["x-opencode-session"].to_str().unwrap()) } else { next["prompt_cache_key"].clone() });
                let mut edited = body.clone();
                edited[field][0]["content"] = json!("edited prefix");
                send(&gate,profile,&edited).await.unwrap();
                let (last_head,last) = observed.lock().unwrap().last().unwrap().clone();
                assert_ne!(carrier,if profile == Profile::Chat { json!(last_head["x-opencode-session"].to_str().unwrap()) } else { last["prompt_cache_key"].clone() });
            }
            let mut broken = json!({"model":"public","stream":true,"stream_options":{"include_obfuscation":false},"metadata":{"mode":"truncated"}});
            broken[field] = json!([{"role":"user","content":"new failed prefix"}]);
            assert!(send(&gate,profile,&broken).await.is_err());
            let (first_head,first) = observed.lock().unwrap().last().unwrap().clone();
            assert!(send(&gate,profile,&broken).await.is_err());
            let (second_head,second) = observed.lock().unwrap().last().unwrap().clone();
            if profile == Profile::Chat {
                assert_ne!(first_head["x-opencode-session"], second_head["x-opencode-session"]);
            } else {
                assert_ne!(first["prompt_cache_key"],second["prompt_cache_key"]);
            }
        }
    }).await.unwrap();
}
