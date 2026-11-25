//! One selected Google Interactions candidate; independent native tool/error/final-history oracle.
mod interactions;
use crate::morphiecore::{
    gateway::{Entry, Gateway, Limits, TaskEntries},
    protocol::{ClientProtocol, openai::GenerationApi},
    provider::{SecretMaterial, TrustedOrigin},
    topology::{self, catalog::google_bindings},
};
use axum::{Router, routing::post};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};
use tokio::net::TcpListener;

pub const KEY: &str = "synthetic-google-loop-client-000001";
pub const ERROR: &str = "missing file";
pub const RESULT: &str = "cedar-71";
pub struct Fixture {
    pub base: String,
    pub seen: Arc<Mutex<Vec<Value>>>,
    pub gateway: Gateway,
    tasks: Vec<tokio::task::JoinHandle<()>>,
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
    profile: GenerationApi,
    messages: bool,
    strict: bool,
    seen: Arc<Mutex<Vec<Value>>>,
}
pub async fn start(profile: GenerationApi, strict: bool) -> Fixture {
    start_with(profile, false, strict).await
}
pub async fn start_messages() -> Fixture {
    start_with(GenerationApi::Chat, true, false).await
}
async fn start_with(profile: GenerationApi, messages: bool, strict: bool) -> Fixture {
    let binding = &google_bindings()[0];
    let seen = Arc::new(Mutex::new(Vec::new()));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin =
        TrustedOrigin::parse(&format!("http://{}", listener.local_addr().unwrap())).unwrap();
    let mut endpoint = binding.endpoint();
    endpoint.target.origin = origin.clone();
    let mut provider = binding.provider();
    provider.origin = origin;
    let route = binding.route();
    let app = Router::new()
        .route(endpoint.target.path.as_str(), post(interactions::answer))
        .with_state(Upstream {
            profile,
            messages,
            strict,
            seen: seen.clone(),
        });
    let upstream = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let topology = topology::compile(
        vec![provider],
        vec![endpoint.clone()],
        vec![route],
        vec![binding.public_model()],
        vec![binding.canonical_model()],
    )
    .unwrap()
    .with_model_metadata([(endpoint.canonical_model.clone(), binding.metadata())])
    .unwrap();
    let gateway = Gateway::new(
        topology,
        TaskEntries {
            generation: vec![Entry {
                model: binding.model.into(),
                protocol: if messages {
                    ClientProtocol::Messages
                } else {
                    profile.into()
                },
                endpoint: endpoint.id,
            }],
            ..Default::default()
        },
        BTreeMap::from([(
            endpoint.credential,
            Arc::new(SecretMaterial::new("synthetic-google-loop-upstream").unwrap()),
        )]),
        std::collections::BTreeMap::from([(
            "fixture-agent".into(),
            SecretMaterial::new(KEY).unwrap(),
        )]),
        Limits::default(),
        None,
    )
    .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}/v1", listener.local_addr().unwrap());
    let owner = gateway.clone();
    let server = tokio::spawn(async move {
        owner.serve(listener, std::future::pending()).await.unwrap();
    });
    Fixture {
        base,
        seen,
        gateway,
        tasks: vec![server, upstream],
    }
}
