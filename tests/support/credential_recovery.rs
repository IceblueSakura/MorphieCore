//! Synthetic named accounts and pre-publication failures during tool feedback.
use crate::filesystem;
use axum::{body::Body, http::HeaderMap, response::Response};
use morphiecore::{
    account::{Accounts, BoundAccounts},
    credential::{CredentialManager, Secret},
};
use std::sync::{
    Mutex,
    atomic::{AtomicBool, Ordering},
};

pub const PRIMARY: &str = "synthetic-recovery-primary";
pub const SECONDARY: &str = "synthetic-recovery-secondary";

#[derive(Default)]
pub struct Recovery {
    attempts: Mutex<Vec<(usize, bool, Option<String>)>>,
    cooling: AtomicBool,
}

impl Recovery {
    /// Record attempts only after the protocol-specific independent wire oracle.
    pub fn observe(&self, turn: usize, secret: &str, headers: &HeaderMap) -> Option<Response> {
        let primary = match secret {
            PRIMARY => true,
            SECONDARY => false,
            _ => panic!("unexpected synthetic credential"),
        };
        for name in [
            "x-session-affinity",
            "x-session-id",
            "x-morphiecore-conversation-id",
        ] {
            assert!(!headers.contains_key(name), "ingress identity leaked");
        }
        let session = headers
            .get("x-opencode-session")
            .map(|v| v.to_str().unwrap().to_owned());
        let mut attempts = self.attempts.lock().unwrap();
        let cooling = self.cooling.load(Ordering::SeqCst);
        let fail = cooling || (primary && turn == 2 && attempts.len() == 1);
        attempts.push((turn, primary, session));
        assert!(attempts.len() <= 7, "unexpected retry or duplicate request");
        fail.then(|| {
            Response::builder()
                .status(503)
                // This hint exceeds the short recovery budget; the client must retry.
                .header("retry-after", "60")
                .body(Body::empty())
                .unwrap()
        })
    }

    pub fn assert_complete(&self, turns: usize, native_session: bool) {
        assert!(!self.cooling.load(Ordering::SeqCst));
        let attempts = self.attempts.lock().unwrap();
        let mut expected = vec![(1, true), (2, true), (2, false)];
        expected.extend((3..=turns).map(|turn| (turn, false)));
        assert_eq!(
            attempts
                .iter()
                .map(|(turn, key, _)| (*turn, *key))
                .collect::<Vec<_>>(),
            expected,
            "a new HTTP request moves accounts; later turns retain that target"
        );
        if native_session {
            let original = attempts[0].2.as_ref().expect("native session carrier");
            assert!(!original.is_empty());
            for (_, _, session) in attempts.iter() {
                assert_eq!(
                    session.as_ref(),
                    Some(original),
                    "tool round changed identity"
                );
            }
        }
    }

    #[allow(dead_code)] // HTTP availability belongs to the gateway target, not SDK history.
    pub async fn assert_all_cooling(
        &self,
        url: &str,
        key: &str,
        input: &serde_json::Value,
        attempted_status: u16,
    ) -> serde_json::Value {
        assert!(self.attempts.lock().unwrap().is_empty());
        self.cooling.store(true, Ordering::SeqCst);
        let client = reqwest::Client::builder()
            .no_proxy()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .unwrap();
        let started = std::time::Instant::now();
        let mut unavailable = None;
        for request in 0..3 {
            let mut outgoing = client
                .post(url)
                .bearer_auth(key)
                .header("x-session-affinity", "synthetic-cooling-session");
            if url.ends_with("/v1/messages") {
                outgoing = outgoing.header("anthropic-version", "2026-10-21");
            }
            let response = outgoing.json(input).send().await.unwrap();
            assert_eq!(
                response.status(),
                if request < 2 { attempted_status } else { 503 }
            );
            if request == 2 {
                let delay = response.headers()["retry-after"]
                    .to_str()
                    .unwrap()
                    .parse::<u64>()
                    .unwrap();
                assert!(delay <= 60);
                assert!(
                    started.elapsed() + std::time::Duration::from_secs(delay)
                        >= std::time::Duration::from_secs(60),
                    "Retry-After shortened the reported upstream cooldown"
                );
                let body: serde_json::Value = response.json().await.unwrap();
                let text = body.to_string();
                for private in [PRIMARY, SECONDARY, "127.0.0.1", "synthetic-cooling-session"] {
                    assert!(!text.contains(private), "private routing detail in error");
                }
                unavailable = Some(body);
            } else {
                // Drain the attempted failure before checking the locally unavailable exchange.
                response.bytes().await.unwrap();
            }
            assert_eq!(
                self.attempts
                    .lock()
                    .unwrap()
                    .iter()
                    .map(|(turn, key, _)| (*turn, *key))
                    .collect::<Vec<_>>(),
                [(1, true), (1, false)][..(request + 1).min(2)],
                "each request dispatches one account; all-cooling dispatches none"
            );
        }
        unavailable.unwrap()
    }
}

/// Use the application credential manager, never a private user store.
pub fn accounts(domain: &str, base_url: &str) -> (BoundAccounts, tempfile::TempDir) {
    use std::io::Write;
    let directory = filesystem::private_directory();
    let manager = CredentialManager::new(directory.path().join("credentials"), vec![]).unwrap();
    for (alias, secret) in [("primary", PRIMARY), ("secondary", SECONDARY)] {
        manager
            .add_api_key(domain, alias, Secret::new(secret.into()).unwrap())
            .unwrap();
    }
    let path = directory.path().join("accounts.json");
    let value = serde_json::json!({"version":1,"providers":{domain:{"accounts":[
        {"name":"primary","enabled":true,"credential":{"alias":"primary"},"base_url":base_url},
        {"name":"secondary","enabled":true,"credential":{"alias":"secondary"},"base_url":base_url}
    ]}}});
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options
        .open(&path)
        .unwrap()
        .write_all(&serde_json::to_vec(&value).unwrap())
        .unwrap();
    let bound = Accounts::load(&path).unwrap().bind(&manager).unwrap();
    (bound, directory)
}
