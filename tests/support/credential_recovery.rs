//! Synthetic managed pool and a single pre-publication failure during tool feedback.
use crate::filesystem;
use axum::{body::Body, http::HeaderMap, response::Response};
use morphiecore::{
    credential::{CredentialManager, CredentialPool, CredentialRef, Secret},
    gateway::Credentials,
    provider::CredentialBindingId,
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
                // Immediate expiry permits a real foreground recovery without sleeps.
                .header("retry-after", if cooling { "60" } else { "0" })
                .body(Body::empty())
                .unwrap()
        })
    }

    pub fn assert_complete(&self, turns: usize, native_session: bool) {
        assert!(!self.cooling.load(Ordering::SeqCst));
        let attempts = self.attempts.lock().unwrap();
        let mut expected = vec![(1, true), (2, true), (2, false), (1, true)];
        expected.extend((3..=turns).map(|turn| (turn, false)));
        assert_eq!(
            attempts
                .iter()
                .map(|(turn, key, _)| (*turn, *key))
                .collect::<Vec<_>>(),
            expected,
            "migration must stick after the primary honestly recovers in another conversation"
        );
        if native_session {
            let original = attempts[0].2.as_ref().expect("native session carrier");
            assert!(!original.is_empty());
            for (index, (_, _, session)) in attempts.iter().enumerate() {
                if index == 3 {
                    assert_ne!(
                        session.as_ref(),
                        Some(original),
                        "new conversation reused identity"
                    );
                } else {
                    assert_eq!(
                        session.as_ref(),
                        Some(original),
                        "tool round changed identity"
                    );
                }
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
        for request in 0..2 {
            let response = client
                .post(url)
                .bearer_auth(key)
                .header("x-session-affinity", "synthetic-cooling-session")
                .json(input)
                .send()
                .await
                .unwrap();
            assert_eq!(
                response.status(),
                if request == 0 { attempted_status } else { 503 }
            );
            if request == 1 {
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
                [(1, true), (1, false)],
                "cooling must not dispatch another upstream request"
            );
        }
        unavailable.unwrap()
    }
}

/// Use the application credential manager, never a private user store.
pub fn pool(domain: &str, binding: CredentialBindingId) -> (Credentials, tempfile::TempDir) {
    let directory = filesystem::private_directory();
    let manager = CredentialManager::new(directory.path(), vec![]).unwrap();
    for (alias, secret) in [("primary", PRIMARY), ("secondary", SECONDARY)] {
        manager
            .add_api_key(domain, alias, Secret::new(secret.into()).unwrap())
            .unwrap();
    }
    let pool = CredentialPool {
        members: ["primary", "secondary"]
            .into_iter()
            .map(|alias| CredentialRef::ApiKey {
                alias: alias.into(),
            })
            .collect(),
        fallback: true,
        max_attempts: 2,
    };
    let mut credentials = Credentials::new();
    credentials
        .insert_pool(binding, manager.bind_pool(domain, &pool).unwrap())
        .unwrap();
    (credentials, directory)
}
