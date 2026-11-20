//! Test-owned OTLP/HTTP JSON receiver; application success reports are not an oracle.
use axum::{Json, Router, extract::DefaultBodyLimit, routing::post};
use serde_json::Value;
use std::sync::{Arc, Mutex};
use tokio::{sync::Notify, task::JoinHandle};

pub struct Collector {
    pub endpoint: String,
    records: Arc<Mutex<Vec<Value>>>,
    spans: Arc<Mutex<Vec<Value>>>,
    metadata: Arc<Mutex<Vec<Value>>>,
    changed: Arc<Notify>,
    server: JoinHandle<()>,
}
impl Collector {
    pub async fn start() -> Self {
        let records = Arc::new(Mutex::new(Vec::new()));
        let spans = Arc::new(Mutex::new(Vec::new()));
        let metadata = Arc::new(Mutex::new(Vec::new()));
        let changed = Arc::new(Notify::new());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let output = records.clone();
        let ready = changed.clone();
        let trace_output = spans.clone();
        let log_metadata = metadata.clone();
        let span_metadata = metadata.clone();
        let server = tokio::spawn(async move {
            let router = Router::new()
                .route(
                    "/v1/logs",
                    post(move |Json(payload): Json<Value>| {
                        let output = output.clone();
                        let ready = ready.clone();
                        let metadata = log_metadata.clone();
                        async move {
                            let mut records = output.lock().unwrap();
                            for resource in payload["resourceLogs"].as_array().unwrap() {
                                remember(&metadata, &resource["resource"]);
                                for scope in resource["scopeLogs"].as_array().unwrap() {
                                    remember(&metadata, &scope["scope"]);
                                    for record in scope["logRecords"].as_array().unwrap() {
                                        assert!(
                                            records.len() < 1024,
                                            "bounded synthetic telemetry"
                                        );
                                        records.push(record.clone());
                                    }
                                }
                            }
                            ready.notify_waiters();
                            Json(serde_json::json!({}))
                        }
                    }),
                )
                .route(
                    "/v1/traces",
                    post(move |Json(payload): Json<Value>| {
                        let output = trace_output.clone();
                        let metadata = span_metadata.clone();
                        async move {
                            let mut spans = output.lock().unwrap();
                            for resource in payload["resourceSpans"].as_array().unwrap() {
                                remember(&metadata, &resource["resource"]);
                                for scope in resource["scopeSpans"].as_array().unwrap() {
                                    remember(&metadata, &scope["scope"]);
                                    for span in scope["spans"].as_array().unwrap() {
                                        assert!(spans.len() < 1024, "bounded synthetic traces");
                                        spans.push(span.clone());
                                    }
                                }
                            }
                            Json(serde_json::json!({}))
                        }
                    }),
                )
                .layer(DefaultBodyLimit::max(1 << 20));
            axum::serve(listener, router).await.unwrap();
        });
        Self {
            endpoint,
            records,
            spans,
            metadata,
            changed,
            server,
        }
    }
    pub fn records(&self) -> Vec<Value> {
        self.records.lock().unwrap().clone()
    }
    pub fn spans(&self) -> Vec<Value> {
        self.spans.lock().unwrap().clone()
    }
    pub fn metadata(&self) -> Vec<Value> {
        self.metadata.lock().unwrap().clone()
    }
    pub async fn wait(&self, predicate: impl Fn(&[Value]) -> bool) -> Vec<Value> {
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                let changed = self.changed.notified();
                tokio::pin!(changed);
                changed.as_mut().enable();
                let records = self.records();
                if predicate(&records) {
                    return records;
                }
                changed.await;
            }
        })
        .await
        .expect("OTLP record deadline")
    }
}

fn remember(entries: &Mutex<Vec<Value>>, value: &Value) {
    let mut entries = entries.lock().unwrap();
    if !entries.contains(value) {
        assert!(entries.len() < 32, "bounded synthetic OTLP metadata");
        entries.push(value.clone());
    }
}

impl Drop for Collector {
    fn drop(&mut self) {
        self.server.abort();
    }
}
pub fn attribute<'a>(record: &'a Value, name: &str) -> &'a Value {
    &record["attributes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|field| field["key"] == name)
        .expect("OTLP attribute")["value"]
}
