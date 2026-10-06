//! Synthetic owned files, authority wire oracles and manager fixtures shared by credential tests.
use super::*;
use axum::{
    Router,
    http::StatusCode,
    routing::{get, post},
};
use serde_json::Value;
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};
use tokio::sync::Notify;

pub(super) fn offline(path: &std::path::Path) -> Result<CredentialManager, CredentialError> {
    CredentialManager::new(path, builtin_drivers(None)?)
}
impl CredentialManager {
    pub(super) fn synthetic(root: &std::path::Path, origin: &str) -> Result<Self, CredentialError> {
        Self::new(
            root,
            vec![
                Arc::new(grok::GrokAuthority::synthetic(origin)?),
                Arc::new(siwc::SiwcAuthority::synthetic(origin)?),
            ],
        )
    }
    pub(super) async fn login_grok(
        &self,
        alias: &str,
        client: &str,
        notify: impl FnOnce(&DevicePrompt) + Send,
    ) -> Result<AccountStatus, CredentialError> {
        let notify = Mutex::new(Some(notify));
        self.login(
            "grok",
            alias,
            LoginOptions {
                client_id: Some(client.into()),
                ..LoginOptions::default()
            },
            |prompt| {
                let LoginPrompt::Device(prompt) = prompt else {
                    panic!("wrong method");
                };
                notify.lock().unwrap().take().unwrap()(prompt);
            },
        )
        .await
    }
    pub(super) async fn login_grok_browser(
        &self,
        alias: &str,
        client: &str,
        port: u16,
        notify: impl FnOnce(&BrowserPrompt) + Send,
    ) -> Result<AccountStatus, CredentialError> {
        let notify = Mutex::new(Some(notify));
        self.login(
            "grok",
            alias,
            LoginOptions {
                client_id: Some(client.into()),
                method: LoginMethod::Browser,
                callback_port: Some(port),
                consent: false,
            },
            |prompt| {
                let LoginPrompt::Browser(prompt) = prompt else {
                    panic!("wrong method");
                };
                notify.lock().unwrap().take().unwrap()(prompt);
            },
        )
        .await
    }
}

pub(crate) use crate::test_files::{Directory, private_directory};

pub(super) fn read_account(root: &std::path::Path, profile: &str, alias: &str) -> Value {
    serde_json::from_slice::<Value>(&std::fs::read(root.join(format!("{profile}.json"))).unwrap())
        .unwrap()["oauth"][alias]
        .take()
}

pub(super) struct Step {
    pub path: &'static str,
    pub expected: Vec<(&'static str, &'static str)>,
    pub status: StatusCode,
    pub body: Value,
    pub gate: Option<Arc<Notify>>,
}
pub(super) fn step(
    path: &'static str,
    expected: Vec<(&'static str, &'static str)>,
    body: Value,
) -> Step {
    Step {
        path,
        expected,
        status: StatusCode::OK,
        body,
        gate: None,
    }
}
pub(super) struct FormCall {
    pub path: String,
    pub fields: std::collections::BTreeMap<String, String>,
}
pub(super) struct Authority {
    pub origin: String,
    pub steps: Arc<Mutex<VecDeque<Step>>>,
    pub entered: Arc<Notify>,
    pub arrivals: Arc<Mutex<Vec<tokio::time::Instant>>>,
    pub forms: Arc<Mutex<Vec<FormCall>>>,
    task: tokio::task::JoinHandle<()>,
}
fn assert_grok_headers(headers: &axum::http::HeaderMap, stage: Option<&str>) {
    let agent = headers["user-agent"].to_str().unwrap();
    assert!(agent.starts_with("grok-shell/1.0.46 (") && agent.ends_with(')'));
    assert!(
        agent.contains("; ") && !agent.contains("MorphieCore") && !agent.contains("morphiecore")
    );
    assert!(!headers.contains_key("originator"));
    match stage {
        Some("device" | "urn:ietf:params:oauth:grant-type:device_code") => {
            assert_eq!(headers["x-grok-client-version"], "1.0.46");
            assert_eq!(headers["x-grok-client-surface"], "headless");
        }
        Some("authorization_code") => {
            assert_eq!(headers["x-grok-client-version"], "1.0.46");
            assert!(!headers.contains_key("x-grok-client-surface"));
        }
        Some("refresh_token" | "plain") => {
            assert!(!headers.contains_key("x-grok-client-version"));
            assert!(!headers.contains_key("x-grok-client-surface"));
        }
        _ => {}
    }
}
impl Authority {
    pub async fn new(steps: Vec<Step>) -> Self {
        let steps = Arc::new(Mutex::new(VecDeque::from(steps)));
        let entered = Arc::new(Notify::new());
        let arrivals = Arc::new(Mutex::new(Vec::new()));
        let forms = Arc::new(Mutex::new(Vec::new()));
        let handler = {
            let steps = steps.clone();
            let entered = entered.clone();
            let arrivals = arrivals.clone();
            let forms = forms.clone();
            move |request: axum::extract::Request| {
                let steps = steps.clone();
                let entered = entered.clone();
                let arrivals = arrivals.clone();
                let forms = forms.clone();
                async move {
                    arrivals.lock().unwrap().push(tokio::time::Instant::now());
                    let path = request.uri().path().to_owned();
                    let headers = request.headers().clone();
                    let body = axum::body::to_bytes(request.into_body(), 65536)
                        .await
                        .unwrap();
                    let next = steps
                        .lock()
                        .unwrap()
                        .pop_front()
                        .expect("unexpected authority request");
                    assert_eq!(path, next.path);
                    if path.starts_with("/oauth2/") {
                        assert_grok_headers(&headers, None);
                    }
                    if path == "/oauth2/userinfo" {
                        assert_grok_headers(&headers, Some("plain"));
                        assert_eq!(headers["authorization"], next.expected[0].1);
                    } else if path == "/.well-known/jwks.json" {
                        assert!(!headers.contains_key("authorization"));
                        assert!(!headers.contains_key("originator"));
                        // The shared JWKS path serves independent RSA (SIWC) and EC (Grok) fixtures.
                        if next.body["keys"][0]["kty"] == "RSA"
                            || (next.status != StatusCode::OK
                                && headers
                                    .get("user-agent")
                                    .is_some_and(|v| v == siwc::USER_AGENT))
                        {
                            assert_eq!(headers["user-agent"], siwc::USER_AGENT);
                        } else {
                            assert_grok_headers(&headers, Some("plain"));
                        }
                        assert!(body.is_empty());
                    } else if path == "/.well-known/openid-configuration" {
                        assert!(body.is_empty());
                        assert!(!headers.contains_key("authorization"));
                        assert_eq!(headers["user-agent"], siwc::USER_AGENT);
                    } else {
                        assert_eq!(headers["content-type"], "application/x-www-form-urlencoded");
                        assert!(!headers.contains_key("originator"));
                        if path.starts_with("/api/accounts/oauth/") {
                            assert_eq!(headers["user-agent"], siwc::USER_AGENT);
                            assert!(!headers.contains_key("chatgpt-account-id"));
                        }
                        let actual: std::collections::BTreeMap<_, _> =
                            url::form_urlencoded::parse(&body).into_owned().collect();
                        if path.starts_with("/oauth2/") {
                            let stage = if path == "/oauth2/device/code" {
                                Some("device")
                            } else {
                                actual
                                    .get("grant_type")
                                    .map(String::as_str)
                                    .or(Some("plain"))
                            };
                            assert_grok_headers(&headers, stage);
                        }
                        forms.lock().unwrap().push(FormCall {
                            path: path.clone(),
                            fields: actual.clone(),
                        });
                        for (key, expected) in next.expected {
                            assert_eq!(actual.get(key).map(String::as_str), Some(expected));
                        }
                    }
                    entered.notify_one();
                    if let Some(gate) = next.gate {
                        gate.notified().await;
                    }
                    (next.status, axum::Json(next.body))
                }
            }
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let app = Router::new()
            .route("/oauth2/device/code", post(handler.clone()))
            .route("/oauth2/token", post(handler.clone()))
            .route("/oauth2/userinfo", get(handler.clone()))
            .route("/oauth2/revoke", post(handler.clone()))
            .route("/.well-known/openid-configuration", get(handler.clone()))
            .route("/api/accounts/oauth/token", post(handler.clone()))
            .route("/api/accounts/oauth/revoke", post(handler.clone()))
            .route("/.well-known/jwks.json", get(handler));
        let task = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        Self {
            origin,
            steps,
            entered,
            arrivals,
            forms,
            task,
        }
    }
    pub fn done(&self) {
        assert!(self.steps.lock().unwrap().is_empty());
    }
    pub fn pool(&self, dir: &Directory) -> CredentialManager {
        CredentialManager::synthetic(&dir.path, &self.origin).unwrap()
    }
}
impl Drop for Authority {
    fn drop(&mut self) {
        self.task.abort();
    }
}
