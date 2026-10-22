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
fn target_selection_is_explicit_and_oauth_cannot_be_redirected() {
    let chat = command(&[
        "probe",
        "--provider",
        "deepseek",
        "--api-key",
        "one",
        "--protocol",
        "openai-chat",
        "--model",
        "not-in-catalog",
    ]);
    let plan = chat.plan().unwrap();
    assert_eq!(plan.origin.as_str(), "https://api.deepseek.com");
    assert_eq!(plan.path, "/chat/completions");
    assert_eq!(
        plan.body.unwrap(),
        json!({"model":"not-in-catalog","messages":[{"role":"user","content":"Reply with the single word OK."}],"max_tokens":128,"stream":false})
    );
    let oauth = command(&[
        "probe",
        "--provider",
        "openai",
        "--account",
        "one",
        "--protocol",
        "openai-responses",
        "--model",
        "new-model",
    ]);
    let plan = oauth.plan().unwrap();
    assert_eq!(plan.oauth, Some("openai"));
    assert_eq!(
        plan.body.unwrap(),
        json!({"model":"new-model","input":[{"role":"user","content":"Reply with the single word OK."}],"stream":true,"store":false})
    );
    for suffix in [
        vec![
            "--origin",
            "https://other.invalid",
            "--path",
            "/v1/responses",
        ],
        vec!["--path", "/other"],
        vec!["--auth", "bearer"],
    ] {
        let mut args = vec![
            "probe",
            "--provider",
            "openai",
            "--account",
            "one",
            "--protocol",
            "openai-responses",
            "--model",
            "x",
        ];
        args.extend(suffix);
        assert!(matches!(command(&args).plan(), Err(Error::OAuthTarget)));
    }
    for provider in ["unknown", "google"] {
        assert!(
            command(&[
                "probe",
                "--provider",
                provider,
                "--account",
                "one",
                "--protocol",
                "openai-responses",
                "--model",
                "x"
            ])
            .plan()
            .is_err()
        );
    }
    assert!(
        command(&[
            "models",
            "--provider",
            "aliyun-tokenplan-cn",
            "--api-key",
            "one",
            "--protocol",
            "openai-chat"
        ])
        .plan()
        .is_err()
    );
    for origin in [
        "https://user:key@host.invalid",
        "http://host.invalid",
        "https://host.invalid?token=x",
    ] {
        assert!(
            command(&[
                "models",
                "--provider",
                "custom",
                "--api-key",
                "one",
                "--protocol",
                "openai-chat",
                "--origin",
                origin,
                "--path",
                "/models"
            ])
            .plan()
            .is_err()
        );
    }
}
#[test]
fn native_requests_paths_headers_and_cursors_are_not_inferred_from_model_names() {
    for (protocol, path, auth, body) in [
        (
            "anthropic",
            "/v1/messages",
            AuthScheme::ApiKeyHeader("x-api-key"),
            json!({"model":"new","messages":[{"role":"user","content":"Reply with the single word OK."}],"max_tokens":64,"stream":false}),
        ),
        (
            "google",
            "/v1beta/models/new:generateContent",
            AuthScheme::ApiKeyHeader("x-goog-api-key"),
            json!({"contents":[{"role":"user","parts":[{"text":"Reply with the single word OK."}]}],"generationConfig":{"maxOutputTokens":64}}),
        ),
    ] {
        let base = if protocol == "google" {
            "/v1beta/models"
        } else {
            "/v1/messages"
        };
        let request = command(&[
            "probe",
            "--provider",
            "custom",
            "--api-key",
            "one",
            "--protocol",
            protocol,
            "--origin",
            "https://example.invalid",
            "--path",
            base,
            "--model",
            "new",
            "--max-output-tokens",
            "64",
        ]);
        let plan = request.plan().unwrap();
        assert_eq!(plan.path, path);
        assert_eq!(plan.auth, auth);
        assert_eq!(plan.body, Some(body));
    }
    let plan = command(&[
        "models",
        "--provider",
        "google",
        "--api-key",
        "one",
        "--protocol",
        "google",
        "--cursor",
        "a&key=other",
    ])
    .plan()
    .unwrap();
    assert_eq!(plan.path, "/v1beta/models?pageToken=a%26key%3Dother");
    assert!(plan.body.is_none());
    for model in ["../other", "x?key=y", "x/other", "models/"] {
        assert!(
            command(&[
                "probe",
                "--provider",
                "google",
                "--api-key",
                "one",
                "--protocol",
                "google",
                "--model",
                model
            ])
            .plan()
            .is_err()
        );
    }
    let go = command(&[
        "probe",
        "--provider",
        "opencode-go",
        "--api-key",
        "one",
        "--protocol",
        "anthropic",
        "--model",
        "new",
    ])
    .plan()
    .unwrap();
    assert_eq!(go.auth, AuthScheme::ApiKeyHeader("x-api-key"));
    assert_eq!(go.path, "/zen/go/v1/messages");
    for args in [
        vec![
            "probe",
            "--provider",
            "custom",
            "--api-key",
            "secret",
            "--account",
            "one",
        ],
        vec!["models", "--provider", "custom", "--protocol", "google"],
        vec!["models", "--provider", "custom", "--key", "never-print"],
    ] {
        assert!(Options::try_parse_from(std::iter::once("upstream").chain(args)).is_err());
    }
}
#[test]
fn model_pages_preserve_ids_and_report_incomplete_lists_without_following_urls() {
    for (protocol, siwc, source, expected) in [
        (
            Protocol::OpenaiChat,
            false,
            json!({"data":[{"id":"new-model","unknown":1}]}),
            json!({"ids":["new-model"],"count":1,"complete":true,"next_cursor":null}),
        ),
        (
            Protocol::OpenaiResponses,
            true,
            json!({"models":[{"slug":"gpt-new","visibility":"list"}]}),
            json!({"ids":["gpt-new"],"count":1,"complete":true,"next_cursor":null}),
        ),
        (
            Protocol::Google,
            false,
            json!({"models":[{"name":"models/new"}],"nextPageToken":"opaque"}),
            json!({"ids":["models/new"],"count":1,"complete":false,"next_cursor":"opaque"}),
        ),
        (
            Protocol::Anthropic,
            false,
            json!({"data":[{"id":"new"}],"has_more":true,"last_id":"new"}),
            json!({"ids":["new"],"count":1,"complete":false,"next_cursor":"new"}),
        ),
    ] {
        assert_eq!(
            wire::models(&serde_json::to_vec(&source).unwrap(), protocol, siwc, None).unwrap(),
            expected
        );
    }
    for source in [
        json!({"data":[{"id":"same"},{"id":"same"}]}),
        json!({"data":[{"id":1}]}),
        json!({"data":[],"error":{"message":"private"}}),
        json!({"data":[],"has_more":true}),
    ] {
        assert!(
            wire::models(
                &serde_json::to_vec(&source).unwrap(),
                Protocol::OpenaiChat,
                false,
                None
            )
            .is_err()
        );
    }
    assert!(
        wire::models(
            br#"{"data":[],"data":[]}"#,
            Protocol::OpenaiChat,
            false,
            None
        )
        .is_err()
    );
    assert!(
        wire::models(
            br#"{"nextPageToken":"same"}"#,
            Protocol::Google,
            false,
            Some("same")
        )
        .is_err()
    );
    assert!(
        wire::models(
            br#"{"data":[],"has_more":"false"}"#,
            Protocol::Anthropic,
            false,
            None
        )
        .is_err()
    );
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
        let command = command(&[
            "probe",
            "--provider",
            "grok",
            "--account",
            "one",
            "--protocol",
            "openai-responses",
            "--model",
            "new",
        ]);
        let result = credential(&manager, command.target(), Some("grok")).await;
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
            let (_, again) = credential(&manager, command.target(), Some("grok"))
                .await
                .unwrap();
            assert!(!again);
            assert_eq!(refreshes.load(Ordering::SeqCst), usize::from(expired));
        }
    }
}
