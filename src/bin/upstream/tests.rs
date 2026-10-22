use super::*;
use morphiecore::credential::{
    AuthDriver, Credential, DriverFuture, Grant, LoginObserver, LoginOptions, VerifiedIdentity,
};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
#[path = "../../../tests/support/filesystem.rs"]
mod filesystem;

fn command(args: &[&str]) -> Command {
    Options::try_parse_from(std::iter::once("upstream").chain(args.iter().copied()))
        .unwrap()
        .command
}
#[test]
fn shared_metadata_selects_targets_without_cli_endpoint_overrides() {
    let plan = command(&["models", "deepseek"]).plan().unwrap();
    assert_eq!(plan.provider.origin.as_str(), "https://api.deepseek.com");
    assert_eq!(plan.path.as_str(), "/models");
    assert!(plan.body.is_none());
    let plan = command(&[
        "probe",
        "deepseek",
        "--protocol",
        "openai-chat",
        "--model",
        "not-in-catalog",
    ])
    .plan()
    .unwrap();
    assert_eq!(plan.path.as_str(), "/chat/completions");
    assert_eq!(
        plan.body.unwrap(),
        json!({"model":"not-in-catalog","messages":[{"role":"user","content":"Reply with the single word OK."}],"stream":false})
    );
    let plan = command(&[
        "probe",
        "openai",
        "--protocol",
        "openai-responses",
        "--model",
        "new-model",
    ])
    .plan()
    .unwrap();
    assert_eq!(plan.oauth, Some("openai"));
    assert_eq!(
        plan.body.unwrap(),
        json!({"model":"new-model","input":[{"role":"user","content":"Reply with the single word OK."}],"stream":true,"store":false})
    );
    for args in [
        vec!["models", "unknown"],
        vec!["models", "aliyun-tokenplan-cn"],
        vec![
            "probe",
            "google",
            "--protocol",
            "openai-responses",
            "--model",
            "x",
        ],
        vec!["models", "openai", "--api-key", "one"],
        vec!["models", "deepseek", "--account", "one"],
    ] {
        assert!(command(&args).plan().is_err());
    }
    for args in [
        vec!["models", "deepseek", "--protocol", "openai-chat"],
        vec!["models", "deepseek", "--origin", "https://other.invalid"],
        vec!["models", "deepseek", "--path", "/other"],
        vec!["models", "deepseek", "--auth", "bearer"],
        vec!["models", "deepseek", "--api-key", "one", "--account", "two"],
    ] {
        assert!(Options::try_parse_from(std::iter::once("upstream").chain(args)).is_err());
    }
}
#[test]
fn native_probe_paths_and_headers_follow_the_shared_entries() {
    for (provider, protocol, path, auth, body) in [
        (
            "opencode-go",
            "anthropic",
            "/zen/go/v1/messages",
            AuthScheme::ApiKeyHeader("x-api-key"),
            json!({"model":"new","messages":[{"role":"user","content":"Reply with the single word OK."}],"max_tokens":1024,"stream":false}),
        ),
        (
            "google",
            "google",
            "/v1beta/models/new:generateContent",
            AuthScheme::ApiKeyHeader("x-goog-api-key"),
            json!({"contents":[{"role":"user","parts":[{"text":"Reply with the single word OK."}]}]}),
        ),
    ] {
        let plan = command(&["probe", provider, "--protocol", protocol, "--model", "new"])
            .plan()
            .unwrap();
        assert_eq!(plan.path.as_str(), path);
        assert_eq!(plan.auth, auth);
        assert_eq!(plan.body, Some(body));
    }
    for model in ["../other", "x?key=y", "x/other", "models/"] {
        assert!(
            command(&["probe", "google", "--protocol", "google", "--model", model])
                .plan()
                .is_err()
        );
    }
}
#[tokio::test]
async fn default_pool_uses_only_its_first_member_and_explicit_alias_bypasses_it() {
    let directory = filesystem::private_directory();
    let manager = CredentialManager::new(directory.path(), vec![]).unwrap();
    let first = manager
        .add_api_key(
            "deepseek",
            "one",
            Secret::new("synthetic-first".into()).unwrap(),
        )
        .unwrap();
    manager
        .add_api_key(
            "deepseek",
            "two",
            Secret::new("synthetic-second".into()).unwrap(),
        )
        .unwrap();
    let selected = command(&["models", "deepseek"]);
    let plan = selected.plan().unwrap();
    assert!(matches!(
        selected_credential(&manager, selected.target(), &plan),
        Err(Error::Pool)
    ));
    manager
        .set_pool(
            "deepseek",
            "deepseek-api-key",
            0,
            morphiecore::credential::CredentialPool {
                members: vec![
                    CredentialRef::ApiKey {
                        alias: "one".into(),
                    },
                    CredentialRef::ApiKey {
                        alias: "two".into(),
                    },
                ],
                fallback: true,
                max_attempts: 2,
            },
        )
        .unwrap();
    assert_eq!(
        credential(&manager, selected.target(), &plan)
            .await
            .unwrap()
            .0
            .expose(),
        "synthetic-first"
    );
    manager
        .set_api_key_enabled("deepseek", "one", first.revision, false)
        .unwrap();
    assert!(
        credential(&manager, selected.target(), &plan)
            .await
            .is_err()
    );
    let explicit = command(&["models", "deepseek", "--api-key", "two"]);
    assert_eq!(
        credential(&manager, explicit.target(), &explicit.plan().unwrap())
            .await
            .unwrap()
            .0
            .expose(),
        "synthetic-second"
    );
}
#[test]
fn model_json_preserves_metadata_without_interpreting_pagination() {
    let value = wire::decode(br#"{"data":[{"id":"new","metadata":{"context":123}}],"has_more":true,"last_id":"new","nextPageToken":"opaque"}"#).unwrap();
    assert_eq!(
        value,
        json!({"data":[{"id":"new","metadata":{"context":123}}],"has_more":true,"last_id":"new","nextPageToken":"opaque"})
    );
    assert!(wire::decode(br#"{"data":[],"data":[]}"#).is_err());
    assert!(contains_secret(
        &json!({"metadata":{"key":"synthetic-secret"}}),
        "synthetic-secret"
    ));
    assert!(!contains_secret(&value, "synthetic-secret"));
}
fn response() -> Value {
    json!({"status":"completed","output":[{"type":"message","role":"assistant","status":"completed","content":[{"type":"output_text","text":"OK"}]}]})
}
#[test]
fn probe_success_requires_text_and_an_honest_terminal() {
    for (protocol, source) in [
        (
            Protocol::OpenaiChat,
            json!({"choices":[{"message":{"role":"assistant","content":"OK"},"finish_reason":"stop"}]}),
        ),
        (Protocol::OpenaiResponses, response()),
        (
            Protocol::Anthropic,
            json!({"type":"message","role":"assistant","content":[{"type":"text","text":"OK"}],"stop_reason":"end_turn"}),
        ),
        (
            Protocol::Google,
            json!({"candidates":[{"content":{"parts":[{"text":"private thought","thought":true},{"text":"OK"}]},"finishReason":"STOP"}]}),
        ),
    ] {
        assert_eq!(
            wire::probe(&serde_json::to_vec(&source).unwrap(), protocol).unwrap(),
            json!({"completed":true,"text_bytes":2})
        );
    }
    for source in [
        json!({"status":"incomplete","output":[]}),
        json!({"status":"completed","output":[]}),
        json!({"status":"failed","output":[]}),
    ] {
        assert!(
            wire::probe(
                &serde_json::to_vec(&source).unwrap(),
                Protocol::OpenaiResponses
            )
            .is_err()
        );
    }
    assert!(wire::probe(br#"{"choices":[{"message":{"role":"assistant","content":"partial"},"finish_reason":"length"}]}"#,Protocol::OpenaiChat).is_err());
    let terminal = format!(
        "event: response.completed\ndata: {}\n\n",
        json!({"type":"response.completed","response":response()})
    );
    assert!(wire::responses_stream(terminal.as_bytes()).is_ok());
    assert!(wire::responses_stream(format!("{terminal}data: [DONE]\n\n").as_bytes()).is_ok());
    for stream in [
        terminal.trim_end().to_owned(),
        format!("{terminal}{terminal}"),
        format!("{terminal}data: {{\"type\":\"error\"}}\n\n"),
        "data: [DONE]\n\n".into(),
        "data: {\"type\":\"response.failed\"}\n\n".into(),
        "event: wrong\ndata: {\"type\":\"response.completed\"}\n\n".into(),
    ] {
        assert!(wire::responses_stream(stream.as_bytes()).is_err());
    }
}

struct Driver {
    refreshes: Arc<AtomicUsize>,
    expired: bool,
    fail: bool,
}
fn grant(access: &str, expiry: Option<u64>) -> Grant {
    Grant {
        identity: VerifiedIdentity {
            subject: "synthetic-person".into(),
            scope: None,
        },
        credential: Credential {
            access: Secret::new(access.into()).unwrap(),
            refresh: Some(Secret::new("synthetic-refresh".into()).unwrap()),
            id_token: None,
            expires_at: expiry,
            scopes: None,
        },
    }
}
impl AuthDriver for Driver {
    fn profile(&self) -> &'static str {
        "grok"
    }
    fn login_client(&self, _: &LoginOptions) -> Result<String, CredentialError> {
        Ok("synthetic-client".into())
    }
    fn validate(
        &self,
        _: &str,
        _: Option<&VerifiedIdentity>,
        _: Option<&Credential>,
    ) -> Result<(), CredentialError> {
        Ok(())
    }
    fn login<'a>(
        &'a self,
        _: &'a str,
        _: &'a LoginOptions,
        _: LoginObserver<'a>,
    ) -> DriverFuture<'a, Grant> {
        Box::pin(async move { Ok(grant("synthetic-old-access", self.expired.then_some(1))) })
    }
    fn can_refresh(&self) -> bool {
        true
    }
    fn refresh<'a>(
        &'a self,
        _: &'a str,
        _: &'a VerifiedIdentity,
        _: &'a Credential,
    ) -> DriverFuture<'a, Grant> {
        Box::pin(async move {
            self.refreshes.fetch_add(1, Ordering::SeqCst);
            if self.fail {
                Err(CredentialError::InvalidGrant)
            } else {
                Ok(grant("synthetic-new-access", None))
            }
        })
    }
}
#[tokio::test]
async fn expired_oauth_refreshes_once_through_manager_and_persists_without_rebinding_identity() {
    for (expired, fail) in [(true, false), (false, false), (true, true)] {
        let directory = filesystem::private_directory();
        let refreshes = Arc::new(AtomicUsize::new(0));
        let manager = CredentialManager::new(
            directory.path(),
            vec![Arc::new(Driver {
                refreshes: refreshes.clone(),
                expired,
                fail,
            })],
        )
        .unwrap();
        manager
            .login("grok", "one", LoginOptions::default(), |_| {})
            .await
            .unwrap();
        let pool = morphiecore::credential::CredentialPool {
            members: vec![CredentialRef::OAuth {
                profile: "grok".into(),
                alias: "one".into(),
            }],
            fallback: false,
            max_attempts: 1,
        };
        manager
            .set_pool("grok", "grok-oauth", 0, pool.clone())
            .unwrap();
        manager
            .set_pool("deepseek", "deepseek-api-key", 0, pool)
            .unwrap();
        let wrong = command(&["models", "deepseek"]);
        assert!(matches!(
            credential(&manager, wrong.target(), &wrong.plan().unwrap()).await,
            Err(Error::CredentialMismatch)
        ));
        assert_eq!(refreshes.load(Ordering::SeqCst), 0);
        let command = command(&[
            "probe",
            "grok",
            "--protocol",
            "openai-responses",
            "--model",
            "new",
        ]);
        let plan = command.plan().unwrap();
        let result = credential(&manager, command.target(), &plan).await;
        assert_eq!(refreshes.load(Ordering::SeqCst), usize::from(expired));
        if fail {
            assert!(matches!(
                result,
                Err(Error::Credential(CredentialError::InvalidGrant))
            ));
            assert!(
                manager
                    .bind_access("grok", "one")
                    .unwrap()
                    .borrow()
                    .is_err()
            );
        } else {
            let (secret, refreshed) = result.unwrap();
            assert_eq!(refreshed, expired);
            assert_eq!(
                secret.expose(),
                if expired {
                    "synthetic-new-access"
                } else {
                    "synthetic-old-access"
                }
            );
            let (_, again) = credential(&manager, command.target(), &plan).await.unwrap();
            assert!(!again);
            assert_eq!(refreshes.load(Ordering::SeqCst), usize::from(expired));
        }
    }
}
