//! One synthetic fixed-target Embedding exchange for raw Router and pinned SDK clients.
use crate::embedding_support as binding;
use axum::{
    Router, body::Body, extract::State, http::HeaderMap, response::Response, routing::post,
};
use morphiecore::{
    adapter::embeddings::Profile,
    gateway::{EmbeddingEntry, Gateway, Limits, TaskEntries},
    lowering::embeddings::EncodingSupport,
    provider::{
        AuthScheme, CredentialBindingId, ProviderDefinition, ProviderId, SecretMaterial,
        TrustedOrigin,
    },
    topology::{self, ModelId, ModelMetadata},
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};
use tokio::net::TcpListener;
pub const CLIENT_KEY: &str = "synthetic-embedding-client-key-0001";
pub struct Fixture {
    pub gateway: Gateway,
    pub base: String,
    pub seen: Arc<Mutex<Vec<Value>>>,
    tasks: Vec<tokio::task::AbortHandle>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        for task in &self.tasks {
            task.abort();
        }
    }
}
#[derive(Clone)]
struct Upstream {
    seen: Arc<Mutex<Vec<Value>>>,
    profile: Profile,
    default_dimensions: usize,
}
async fn answer(
    State(state): State<Upstream>,
    headers: HeaderMap,
    axum::Json(request): axum::Json<Value>,
) -> Response {
    if state.profile == Profile::Google {
        assert_eq!(
            headers["x-goog-api-key"],
            "synthetic-embedding-upstream-key-0001"
        );
        assert!(!headers.contains_key("authorization"));
        assert!(!headers.contains_key("x-never-forward"));
        assert!(request.get("user").is_none() && request.get("encoding_format").is_none());
        let requests = request["requests"].as_array().unwrap();
        for entry in requests {
            assert_eq!(entry["model"], "models/private-vector");
            assert_eq!(entry["embedContentConfig"]["autoTruncate"], false);
            assert!(entry.get("outputDimensionality").is_none());
            assert_eq!(entry["content"]["parts"].as_array().unwrap().len(), 1);
        }
        state.seen.lock().unwrap().push(request.clone());
        let text = requests[0]["content"]["parts"][0]["text"].as_str().unwrap();
        if text == "rate-limit" {
            return Response::builder()
                .status(429)
                .header("content-type", "application/json")
                .header("retry-after", "3")
                .body(Body::from(
                    r#"{"error":{"status":"RESOURCE_EXHAUSTED","message":"private diagnostic"}}"#,
                ))
                .unwrap();
        }
        let vectors: Vec<_> = requests
            .iter()
            .map(|entry| {
                let dimensions = entry["embedContentConfig"]["outputDimensionality"]
                    .as_u64()
                    .map(|v| v as usize)
                    .unwrap_or(state.default_dimensions);
                let mut values = vec![json!(0); dimensions];
                values[0] = json!(0.1);
                if dimensions > 1 {
                    values[1] = json!(-0.125);
                }
                json!({"values":values})
            })
            .collect();
        let mut output = json!({"embeddings":vectors});
        if text != "no-usage" {
            output["usageMetadata"] = if text == "details-only" {
                json!({"promptTokenDetails":[{"modality":"TEXT","tokenCount":7}]})
            } else {
                json!({"promptTokenCount":7,"promptTokenDetails":[{"modality":"TEXT","tokenCount":7}]})
            };
        }
        if text == "malformed-usage" {
            output["usageMetadata"]["promptTokenCount"] = Value::Null;
        }
        if text == "tensor" {
            output["embeddings"][0]["shape"] = json!([1, 2]);
        }
        if text == "partial" {
            output["embeddings"].as_array_mut().unwrap().pop();
        }
        return Response::builder()
            .header("content-type", "application/json")
            .body(Body::from(output.to_string()))
            .unwrap();
    }
    assert_eq!(
        headers["authorization"],
        "Bearer synthetic-embedding-upstream-key-0001"
    );
    assert!(!headers.contains_key("x-never-forward"));
    assert_eq!(request["model"], "private-vector");
    let input = request["input"].as_array().unwrap();
    assert!(request.get("stream").is_none());
    state.seen.lock().unwrap().push(request.clone());
    let binary = request["encoding_format"] == "base64";
    assert!(binary || request["encoding_format"] == "float");
    let dimensions = request["dimensions"]
        .as_u64()
        .map(|n| n as usize)
        .unwrap_or(state.default_dimensions);
    let values = if input[0] == "wrong-width" {
        if binary {
            json!("AAAAPw==")
        } else {
            json!([0.5])
        }
    } else if input[0] == "underflow" {
        serde_json::from_str("[1e-1000,0]").unwrap()
    } else if binary {
        json!("zczMPQAAAL4=")
    } else {
        let mut values = vec![json!(0); dimensions];
        values[0] = json!(0.1);
        if dimensions > 1 {
            values[1] = json!(-0.125);
        }
        json!(values)
    };
    let data: Vec<_> = (0..input.len())
        .rev()
        .map(|index| json!({"object":"embedding","index":index,"embedding":values}))
        .collect();
    let usage = if state.profile == Profile::DashScope {
        json!({"total_tokens":7})
    } else {
        json!({"prompt_tokens":7,"total_tokens":7})
    };
    let mut output = json!({
        "object":"list","model":"reported-vector","id":"synthetic-id","data":data,"usage":usage,"unknown":true
    });
    if input[0].is_array() {
        output.as_object_mut().unwrap().remove("usage");
    }
    Response::builder()
        .header("content-type", "application/json")
        .body(Body::from(output.to_string()))
        .unwrap()
}
impl Fixture {
    pub async fn start(
        encodings: EncodingSupport,
        capture: Option<&std::path::Path>,
        diagnostics: Option<&std::path::Path>,
    ) -> Self {
        Self::start_with_dimensions(encodings, 2, 4, capture, diagnostics).await
    }
    pub async fn start_with_dimensions(
        encodings: EncodingSupport,
        default_dimensions: usize,
        max_inputs: usize,
        capture: Option<&std::path::Path>,
        diagnostics: Option<&std::path::Path>,
    ) -> Self {
        Self::start_profile(
            encodings,
            default_dimensions,
            max_inputs,
            capture,
            diagnostics,
            false,
        )
        .await
    }
    pub async fn start_google(
        capture: Option<&std::path::Path>,
        diagnostics: Option<&std::path::Path>,
    ) -> Self {
        Self::start_profile(EncodingSupport::Float, 2, 4, capture, diagnostics, true).await
    }
    async fn start_profile(
        encodings: EncodingSupport,
        default_dimensions: usize,
        max_inputs: usize,
        capture: Option<&std::path::Path>,
        diagnostics: Option<&std::path::Path>,
        google: bool,
    ) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let provider = ProviderDefinition {
            id: ProviderId::new("fixture").unwrap(),
            origin: TrustedOrigin::parse(&origin).unwrap(),
            endpoints: [].into(),
            credential_pool: None,
            auth: if google {
                AuthScheme::ApiKeyHeader("x-goog-api-key")
            } else {
                AuthScheme::Bearer
            },
        };
        let (operation, mut route) = binding::binding(&provider);
        route.endpoint.contract.encodings = encodings;
        route.endpoint.contract.default_dimensions = default_dimensions;
        route.endpoint.contract.dimensions =
            morphiecore::lowering::embeddings::DimensionSupport::UpTo(default_dimensions.max(16));
        route.endpoint.contract.max_inputs = max_inputs;
        route.endpoint.contract.reports =
            morphiecore::lowering::embeddings::ReportPolicy::Compatible;
        route.endpoint.contract.user = morphiecore::lowering::embeddings::UserPolicy::Omit;
        route.endpoint.contract.tokenizer = if google {
            None
        } else {
            Some(morphiecore::semantic::task::embedding::TokenizerScope::Cl100kBase)
        };
        route.endpoint.contract.max_input_tokens = Some(8192);
        route.endpoint.execution.response_body_limit = 1 << 20;
        route.endpoint.profile = if google {
            Profile::Google
        } else if encodings == EncodingSupport::Float {
            Profile::DashScope
        } else {
            Profile::OpenRouter
        };
        let seen = Arc::new(Mutex::new(vec![]));
        let router = Router::new()
            .route("/embeddings", post(answer))
            .with_state(Upstream {
                seen: seen.clone(),
                profile: route.endpoint.profile,
                default_dimensions,
            });
        let upstream = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        let topology = topology::compile(vec![provider], vec![], vec![], vec![], vec![])
            .unwrap()
            .with_embeddings(vec![operation], vec![route])
            .unwrap()
            .with_model_metadata([(
                ModelId::new("canonical-vector").unwrap(),
                ModelMetadata::new(1, "Synthetic").unwrap(),
            )])
            .unwrap();
        let mut gateway = Gateway::new(
            topology,
            TaskEntries {
                embeddings: vec![EmbeddingEntry {
                    model: "public-vector".into(),
                }],
                ..Default::default()
            },
            BTreeMap::from([(
                CredentialBindingId::new("fixture-key").unwrap(),
                Arc::new(SecretMaterial::new("synthetic-embedding-upstream-key-0001").unwrap()),
            )]),
            std::collections::BTreeMap::from([(
                "fixture-agent".into(),
                SecretMaterial::new(CLIENT_KEY).unwrap(),
            )]),
            Limits::default(),
            None,
        )
        .unwrap();
        if let Some(path) = capture {
            gateway = gateway.with_capture(serde_json::from_value(json!({"path":path,
            "client_request":{"headers":true,"body":true},"upstream_request":{"headers":true,"body":true},
            "upstream_response":{"headers":true,"body":true},"client_response":{"headers":true,"body":true}})).unwrap()).unwrap();
        }
        if let Some(path) = diagnostics {
            gateway = gateway.with_diagnostics(path).unwrap();
        }
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let serving = gateway.clone();
        let server = tokio::spawn(async move {
            serving
                .serve(listener, std::future::pending())
                .await
                .unwrap()
        });
        Self {
            gateway,
            base,
            seen,
            tasks: vec![upstream.abort_handle(), server.abort_handle()],
        }
    }
}
