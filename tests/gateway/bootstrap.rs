//! Binary CLI identity and isolated file/bootstrap process behavior.
use super::{Guard, support};
use crate::filesystem;
use axum::{Router, http::StatusCode};
use morphiecore_test_support::{child_line, child_process};
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};
use tokio::net::TcpListener;

#[tokio::test]
async fn catalog_inspection_needs_neither_home_nor_credentials() {
    let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_morphiecore"));
    command.env_clear().args(["--catalog", "--json"]);
    let output = child_process::run(&mut command, b"", Duration::from_secs(5), 256 << 10)
        .await
        .unwrap();
    assert!(output.status.success());
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    let model = value["models"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["model"] == "claude-haiku-5-5")
        .unwrap();
    assert!(
        model["candidates"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["binding"] == "opencode-go-haiku-messages")
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("https://"));
}

#[cfg(unix)]
#[tokio::test]
async fn fresh_account_setup_live_runner_and_diagnostics_share_the_production_loader() {
    let collector = crate::telemetry::Collector::start().await;
    use axum::{Json, http::HeaderMap, routing::post};
    use std::{
        io::Write,
        os::unix::fs::{DirBuilderExt, OpenOptionsExt},
        sync::Mutex,
    };
    let calls = Arc::new(Mutex::new(Vec::new()));
    let observed = calls.clone();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}/custom/v1", listener.local_addr().unwrap());
    let _upstream = Guard(tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new().route(
                "/custom/v1/messages",
                post(move |headers: HeaderMap, Json(body): Json<Value>| {
                    let calls = observed.clone();
                    async move {
                        calls
                            .lock()
                            .unwrap()
                            .push((headers["x-api-key"].to_str().unwrap().to_owned(), body));
                        StatusCode::SERVICE_UNAVAILABLE
                    }
                }),
            ),
        )
        .await
        .unwrap();
    }));
    let directory = filesystem::private_directory();
    let root = directory.path();
    let store = root.join("credentials");
    let accounts = root.join("accounts.json");
    let write = |name: &str, bytes: &[u8]| {
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(root.join(name))
            .unwrap()
            .write_all(bytes)
            .unwrap();
    };
    write(
        "accounts.json",
        &serde_json::to_vec(&json!({"version":1,"providers":{"opencode-go":{
            "recovery":{"recovery_timeout_ms":0},"accounts":(["primary","backup"].map(|name| json!({
                "name":name,"enabled":true,"credential":{"alias":name},"base_url":origin
            })))
        }}}))
        .unwrap(),
    );
    let auth = |args: &[&str]| {
        let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_morphiecore-auth"));
        command
            .env_clear()
            .args([
                "--accounts",
                accounts.to_str().unwrap(),
                "--store",
                store.to_str().unwrap(),
            ])
            .args(args);
        command
    };
    let explained = child_process::run(
        &mut auth(&["explain"]),
        b"",
        Duration::from_secs(5),
        16 << 10,
    )
    .await
    .unwrap();
    assert!(explained.status.success() && !store.exists());
    assert!(String::from_utf8_lossy(&explained.stdout).contains("opencode-go/primary"));
    let absent = child_process::run(
        &mut auth(&["validate", "--json"]),
        b"",
        Duration::from_secs(5),
        16 << 10,
    )
    .await
    .unwrap();
    assert!(!absent.status.success() && !store.exists());
    for name in ["primary", "backup"] {
        let secret = format!("synthetic-{name}");
        let filename = format!("{name}.key");
        write(&filename, secret.as_bytes());
        let added = child_process::run(
            &mut auth(&[
                "key",
                "add",
                "--provider",
                "opencode-go",
                "--account",
                name,
                "--secret-file",
                root.join(&filename).to_str().unwrap(),
            ]),
            b"",
            Duration::from_secs(5),
            16 << 10,
        )
        .await
        .unwrap();
        assert!(added.status.success());
        assert!(!String::from_utf8_lossy(&added.stdout).contains(&secret));
        assert!(!String::from_utf8_lossy(&added.stderr).contains(&secret));
    }
    let valid = child_process::run(
        &mut auth(&["validate", "--json"]),
        b"",
        Duration::from_secs(5),
        16 << 10,
    )
    .await
    .unwrap();
    assert!(valid.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&valid.stdout).unwrap()["accounts_checked"],
        2
    );
    let run = root.join("run");
    std::fs::DirBuilder::new().mode(0o700).create(&run).unwrap();
    let write_run = |name: &str, value: Value| {
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(run.join(name))
            .unwrap()
            .write_all(&serde_json::to_vec(&value).unwrap())
            .unwrap();
    };
    let client_key = "synthetic-bootstrap-client-key-0001";
    write_run(
        "auth.json",
        json!({"version":1,"agents":[{"agent_id":"fixture-agent","key":client_key}]}),
    );
    write_run(
        "models.json",
        json!({"version":1,"models":{"claude-haiku-5-5":{
            "total_timeout_ms":120000,
            "candidates":[{"binding":"opencode-go-haiku-messages","maximum_output_tokens":2048,"default_output_tokens":null}]
        }}}),
    );
    write_run(
        "gateway.json",
        json!({
            "version":1,
            "accounts_file":accounts,
            "credentials_dir":store,
            "auth_file":"auth.json",
            "bind":"127.0.0.1:0",
            "models_file":"models.json",
            "delivery":{"exchange_timeout_ms":120000},
            "diagnostics":{"endpoint":collector.endpoint}
        }),
    );
    let mut process = tokio::process::Command::new(env!("CARGO_BIN_EXE_morphiecore"))
        .args(["--config", run.join("gateway.json").to_str().unwrap()])
        .env_clear()
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let mut output = tokio::io::BufReader::new(process.stdout.take().unwrap());
    let ready = match child_line::read_line(&mut output, Duration::from_secs(10), 128).await {
        Ok(line) => line,
        Err(error) => {
            let _ = process.kill().await;
            let _ = process.wait().await;
            panic!("{error}");
        }
    };
    let gateway_origin = ready
        .trim()
        .strip_prefix("MorphieCore listening on ")
        .expect("bounded readiness line");
    assert!(gateway_origin.starts_with("http://127.0.0.1:"));
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(15))
        .build()
        .unwrap();
    let reply = client
        .post(format!("{gateway_origin}/v1/messages"))
        .bearer_auth(client_key)
        .header("anthropic-version", "2026-10-21")
        .header("x-morphiecore-conversation-id", "messages-input-acceptance")
        .json(&json!({
            "model":"claude-haiku-5-5",
            "messages":[{"role":"user","content":"synthetic"}],
            "max_tokens":2048,
            "stream":false
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(reply.status().as_u16(), 529);
    assert_eq!(
        unsafe { libc::kill(process.id().unwrap() as libc::pid_t, libc::SIGINT) },
        0
    );
    let exited = tokio::time::timeout(Duration::from_secs(5), process.wait_with_output())
        .await
        .unwrap()
        .unwrap();
    assert!(
        exited.status.success(),
        "{}",
        String::from_utf8_lossy(&exited.stderr)
    );
    assert_eq!(calls.lock().unwrap().len(), 1);
    assert_eq!(calls.lock().unwrap()[0].0, "synthetic-primary");
    assert_eq!(calls.lock().unwrap()[0].1["max_tokens"], 2048);
    let exported = collector
        .wait(|logs| logs.iter().any(|log| log["eventName"] == "gateway.failure"))
        .await
        .len();
    let mut validation = tokio::process::Command::new(env!("CARGO_BIN_EXE_morphiecore"));
    validation.env_clear().args([
        "--config",
        run.join("gateway.json").to_str().unwrap(),
        "--validate",
        "--json",
    ]);
    let validated = child_process::run(&mut validation, b"", Duration::from_secs(5), 16 << 10)
        .await
        .unwrap();
    assert!(validated.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&validated.stdout).unwrap(),
        json!({"valid":true})
    );
    assert_eq!(collector.records().len(), exported);
    let mut explanation = tokio::process::Command::new(env!("CARGO_BIN_EXE_morphiecore"));
    explanation.env_clear().args([
        "--config",
        run.join("gateway.json").to_str().unwrap(),
        "--explain",
        "--json",
    ]);
    let explained = child_process::run(&mut explanation, b"", Duration::from_secs(5), 64 << 10)
        .await
        .unwrap();
    assert!(
        explained.status.success(),
        "{}",
        String::from_utf8_lossy(&explained.stderr)
    );
    let effective: Value = serde_json::from_slice(&explained.stdout).unwrap();
    assert_eq!(effective["diagnostics_enabled"], true);
    assert_eq!(effective["diagnostics"]["private_bodies"], false);
    assert!(effective["diagnostics"].get("endpoint").is_none());
    assert_eq!(effective["selections"][0]["account"], "primary");
    assert_eq!(effective["selections"][1]["account"], "backup");
    assert_eq!(
        effective["selections"][0]["binding"],
        "opencode-go-haiku-messages"
    );
    for secret in [
        "synthetic-primary",
        "synthetic-backup",
        origin.as_str(),
        store.to_str().unwrap(),
        run.to_str().unwrap(),
    ] {
        assert!(!String::from_utf8_lossy(&explained.stdout).contains(secret));
    }
    assert_eq!(
        calls.lock().unwrap().len(),
        1,
        "explanation must not send or probe"
    );
    assert_eq!(collector.records().len(), exported);
    let status = child_process::run(
        &mut auth(&["status"]),
        b"",
        Duration::from_secs(5),
        16 << 10,
    )
    .await
    .unwrap();
    assert!(status.status.success());
    let status = String::from_utf8(status.stdout).unwrap();
    assert!(status.contains("opencode-go/primary:") && status.contains("opencode-go/backup:"));
    assert!(!status.contains("synthetic-"));
    let mut diagnostic = tokio::process::Command::new(env!("CARGO_BIN_EXE_morphiecore-upstream"));
    diagnostic.env_clear().args([
        "probe",
        "opencode-go",
        "--account",
        "backup",
        "--accounts",
        accounts.to_str().unwrap(),
        "--store",
        store.to_str().unwrap(),
        "--protocol",
        "anthropic",
        "--model",
        "claude-haiku-5-5",
        "--timeout-seconds",
        "3",
    ]);
    let result = child_process::run(&mut diagnostic, b"", Duration::from_secs(5), 16 << 10)
        .await
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("503"));
    let calls = calls.lock().unwrap();
    assert_eq!(
        calls.len(),
        2,
        "diagnostic failure cannot retry or choose another account"
    );
    assert_eq!(calls[1].0, "synthetic-backup");
    assert_eq!(calls[1].1["max_tokens"], 1024);
}

#[tokio::test]
async fn binaries_report_cli_identity_without_loading_credentials() {
    for (binary, name) in [
        (env!("CARGO_BIN_EXE_morphiecore"), "morphiecore"),
        (env!("CARGO_BIN_EXE_morphiecore-auth"), "morphiecore-auth"),
    ] {
        let mut command = tokio::process::Command::new(binary);
        command.arg("--help").env_clear().kill_on_drop(true);
        let output = child_process::run(&mut command, b"", Duration::from_secs(5), 64 << 10)
            .await
            .expect("bounded CLI help");
        assert!(output.status.success());
        assert!(output.stderr.is_empty());
        let help = String::from_utf8(output.stdout).unwrap();
        assert!(help.contains(&format!("Usage: {name} ")), "{help}");
    }
}

#[tokio::test]
async fn binary_bootstraps_default_home_files_and_ignores_environment_keys() {
    use morphiecore::credential::Secret;
    use std::sync::atomic::{AtomicUsize, Ordering};
    // A rejecting loopback proxy makes even an accidental upstream dispatch offline.
    let trap = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy = format!("http://{}", trap.local_addr().unwrap());
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = calls.clone();
    let _trap = Guard(tokio::spawn(async move {
        axum::serve(
            trap,
            Router::new().fallback(move || {
                let observed = observed.clone();
                async move {
                    observed.fetch_add(1, Ordering::SeqCst);
                    StatusCode::BAD_GATEWAY
                }
            }),
        )
        .await
        .unwrap();
    }));
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join(".local/share/morphiecore/credentials");
    let mut directories = std::fs::DirBuilder::new();
    directories.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        directories.mode(0o700);
    }
    directories.create(root.parent().unwrap()).unwrap();
    let manager = morphiecore::credential::CredentialManager::new(&root, vec![]).unwrap();
    manager
        .add_api_key(
            "deepseek",
            "one",
            morphiecore::credential::Secret::new("synthetic-file-key".into()).unwrap(),
        )
        .unwrap();
    let path = root.parent().unwrap().join("gateway.json");
    manager
        .add_api_key(
            "openrouter",
            "speech",
            Secret::new("synthetic-speech-file-key".into()).unwrap(),
        )
        .unwrap();
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    use std::io::Write;
    options.open(root.parent().unwrap().join("accounts.json")).unwrap().write_all(&serde_json::to_vec(&json!({
        "version":1,"providers":{
            "deepseek":{"accounts":[{"name":"primary","enabled":true,"credential":{"alias":"one"}}]},
            "openrouter":{"accounts":[{"name":"speech","enabled":true,"credential":{"alias":"speech"}}]}
        }
    })).unwrap()).unwrap();
    options
        .open(&path)
        .unwrap()
        .write_all(
            &serde_json::to_vec(
                &json!({"version":1,"auth_file":"auth.json","bind":"127.0.0.1:0","proxy":"unsupported://invalid"}),
            )
            .unwrap(),
        )
        .unwrap();
    options
        .open(root.parent().unwrap().join("auth.json"))
        .unwrap()
        .write_all(
            &serde_json::to_vec(&json!({"version":1,"agents":[
                {"agent_id":"fixture-agent","key":support::CLIENT_KEY}
            ]}))
            .unwrap(),
        )
        .unwrap();
    let binding = morphiecore::topology::catalog::api_key_bindings()
        .iter()
        .find(|b| b.model == "deepseek-v4.1-flash-1017")
        .unwrap()
        .endpoint_id(morphiecore::topology::GenerationProtocol::OpenAiResponses);
    options
        .open(root.parent().unwrap().join("models.json"))
        .unwrap()
        .write_all(
            &serde_json::to_vec(&json!({"version":1,"models":{
                "deepseek-v4.1-flash-1017":{"candidates":[{"binding":binding.as_str()}]},
                "qwen-audio-3.0-tts-flash":{"account":"speech"}
            }}))
            .unwrap(),
        )
        .unwrap();
    let mut process = tokio::process::Command::new(env!("CARGO_BIN_EXE_morphiecore"))
        .args(["--proxy", &proxy])
        .env_clear()
        .env("HTTPS_PROXY", "unsupported://environment")
        .env(
            if cfg!(windows) { "USERPROFILE" } else { "HOME" },
            dir.path(),
        )
        .env(
            "MORPHIECORE_CLIENT_KEY",
            "synthetic-ignored-environment-key-0001",
        )
        .env(
            "MORPHIECORE_DEEPSEEK_API_KEY",
            "synthetic-not-a-provider-key-0001",
        )
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let mut output = tokio::io::BufReader::new(process.stdout.take().unwrap());
    let ready = match child_line::read_line(&mut output, Duration::from_secs(3), 127).await {
        Ok(line) => line,
        Err(error) => {
            let _ = process.kill().await;
            let _ = process.wait().await;
            panic!("{error}");
        }
    };
    let origin = ready
        .trim()
        .strip_prefix("MorphieCore listening on ")
        .expect("bounded readiness line");
    assert!(origin.starts_with("http://127.0.0.1:"));
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(2))
        .build()
        .unwrap();
    assert_eq!(
        client
            .post(format!("{origin}/v1/responses"))
            .json(&json!({"model":"missing","input":"hello"}))
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    assert_eq!(
        client
            .post(format!("{origin}/v1/responses"))
            .bearer_auth(support::CLIENT_KEY)
            .json(&json!({"model":"missing","input":"hello"}))
            .send()
            .await
            .unwrap()
            .status(),
        404
    );
    let models: Value = client
        .get(format!("{origin}/v1/models"))
        .bearer_auth(support::CLIENT_KEY)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(models["data"].as_array().unwrap().len(), 2);
    assert!(models["data"].as_array().unwrap().contains(&json!({
        "id":"qwen-audio-3.0-tts-flash","object":"model","created":1784592000,"owned_by":"Alibaba"
    })));
    let text_models: Value = client
        .get(format!("{origin}/v1/morphiecore/models"))
        .bearer_auth(support::CLIENT_KEY)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(text_models["scope"], "generation_text");
    assert_eq!(text_models["data"].as_array().unwrap().len(), 1);
    assert_eq!(text_models["data"][0]["id"], models["data"][0]["id"]);
    assert_eq!(
        text_models["data"][0]["interfaces"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    // No admitted request is issued: the synthetic key must never reach a Provider.
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    #[cfg(unix)]
    {
        assert_eq!(
            unsafe { libc::kill(process.id().unwrap() as libc::pid_t, libc::SIGTERM) },
            0
        );
        assert!(
            tokio::time::timeout(Duration::from_secs(3), process.wait())
                .await
                .unwrap()
                .unwrap()
                .success()
        );
        assert!(
            client
                .get(format!("{origin}/v1/models"))
                .send()
                .await
                .is_err()
        );
    }
    #[cfg(not(unix))]
    {
        process.kill().await.unwrap();
        process.wait().await.unwrap();
    }
}
async fn export_synthetic_failure(endpoint: String, private_bodies: bool) {
    use axum::{Json, routing::post};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tower::ServiceExt;
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = calls.clone();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let _upstream = Guard(tokio::spawn(async move {
        axum::serve(listener, Router::new().route("/chat/completions", post(move || {
            let calls = observed.clone();
            async move {
                calls.fetch_add(1, Ordering::SeqCst);
                (StatusCode::BAD_REQUEST, Json(json!({"error":{"message":"synthetic business failure","api_key":"upstream-secret"}})))
            }
        }))).await.unwrap();
    }));
    let gate = support::gateway(&origin, Default::default())
        .with_diagnostics(morphiecore::gateway::DiagnosticsConfig {
            level: morphiecore::gateway::LogLevel::Warn,
            endpoint,
            private_bodies,
            body_bytes: 4096,
        })
        .unwrap();
    let request = axum::http::Request::builder()
        .method("POST")
        .uri("/v1/chat/completions?api_key=query-secret")
        .header("authorization", format!("Bearer {}", support::CLIENT_KEY))
        .header("x-private", "private-header")
        .header("x-request-id", "forged-request")
        .header("agent_id", "forged-agent")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(
            json!({"model":"public-model",
            "messages":[{"role":"user","content":"business-prompt"}],"api_key":"client-secret"})
            .to_string(),
        ))
        .unwrap();
    let response = gate.router().oneshot(request).await.unwrap();
    assert_eq!(response.status(), 502);
    axum::body::to_bytes(response.into_body(), 65536)
        .await
        .unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    gate.shutdown_diagnostics().await;
}

#[tokio::test]
#[ignore = "requires an isolated loopback OTLP collector connected to the development query backend"]
async fn emit_failure_for_indexed_query() {
    let endpoint =
        std::env::var("MORPHIECORE_TEST_INDEXED_OTLP").expect("set the test collector endpoint");
    let uri: axum::http::Uri = endpoint.parse().expect("test collector URI");
    assert_eq!(uri.scheme_str(), Some("http"));
    assert_eq!(uri.host(), Some("127.0.0.1"));
    assert!(uri.port().is_some() && uri.path() == "/" && uri.query().is_none());
    export_synthetic_failure(endpoint, false).await;
}

#[tokio::test]
async fn private_failure_context_reaches_otlp_with_trusted_identity_and_redacted_bodies() {
    let collector = crate::telemetry::Collector::start().await;
    export_synthetic_failure(collector.endpoint.clone(), true).await;
    let logs = collector
        .wait(|logs| {
            logs.iter()
                .any(|log| log["eventName"] == "gateway.failure.response")
        })
        .await;
    let failure = logs
        .iter()
        .find(|log| log["eventName"] == "gateway.failure")
        .unwrap();
    assert_eq!(failure["severityText"], "WARN");
    let spans = collector.spans();
    let request_span = spans
        .iter()
        .find(|span| span["name"] == "gateway.request")
        .unwrap();
    let operation_span = spans
        .iter()
        .find(|span| span["name"] == "gateway.operation")
        .unwrap();
    let attempt_span = spans
        .iter()
        .find(|span| span["name"] == "gateway.attempt")
        .unwrap();
    let client_span = spans
        .iter()
        .find(|span| span["name"] == "gateway.upstream.http")
        .unwrap();
    assert_eq!(failure["traceId"], request_span["traceId"]);
    assert_eq!(failure["spanId"], attempt_span["spanId"]);
    assert_eq!(attempt_span["parentSpanId"], operation_span["spanId"]);
    assert_eq!(client_span["parentSpanId"], attempt_span["spanId"]);
    assert_eq!(client_span["traceId"], request_span["traceId"]);
    assert_eq!(client_span["kind"], 3, "OTLP client span");
    assert_eq!(
        crate::telemetry::attribute(client_span, "morphiecore.io.outcome")["stringValue"],
        "eof",
        "failed status does not imply a transport failure"
    );
    assert_eq!(operation_span["parentSpanId"], request_span["spanId"]);
    let followup = logs
        .iter()
        .find(|log| log["eventName"] == "gateway.failure.response")
        .unwrap();
    assert_eq!(followup["spanId"], request_span["spanId"]);
    assert_eq!(failure["traceId"].as_str().unwrap().len(), 32);
    let trace_text = serde_json::to_string(&spans).unwrap();
    assert!(
        !trace_text.contains("business-prompt")
            && !trace_text.contains("synthetic business failure")
    );
    assert_eq!(
        crate::telemetry::attribute(failure, "http.response.status_code")["intValue"],
        "400"
    );
    assert_eq!(
        crate::telemetry::attribute(failure, "agent_id")["stringValue"],
        "fixture-agent"
    );
    let id = crate::telemetry::attribute(failure, "request_id")["stringValue"]
        .as_str()
        .unwrap();
    assert_ne!(id, "forged-request");
    for log in &logs {
        assert_eq!(
            crate::telemetry::attribute(log, "request_id")["stringValue"],
            id
        );
    }
    let text = format!(
        "{} {} {}",
        serde_json::to_string(&logs).unwrap(),
        trace_text,
        serde_json::to_string(&collector.metadata()).unwrap()
    );
    assert!(text.contains("business-prompt") && text.contains("synthetic business failure"));
    for secret in [
        support::CLIENT_KEY,
        "synthetic-upstream-credential-0001",
        "client-secret",
        "upstream-secret",
        "query-secret",
        "private-header",
        "forged-agent",
    ] {
        assert!(!text.contains(secret), "{secret}");
    }
}

#[tokio::test]
async fn failure_export_without_capture_contains_only_closed_metadata() {
    let collector = crate::telemetry::Collector::start().await;
    export_synthetic_failure(collector.endpoint.clone(), false).await;
    assert!(
        collector
            .records()
            .iter()
            .any(|log| log["eventName"] == "gateway.failure")
    );
    assert!(
        collector
            .records()
            .iter()
            .all(|log| log["eventName"] != "gateway.failure.response")
    );
    let metadata = collector.metadata();
    let resource = metadata
        .iter()
        .find(|entry| {
            entry["attributes"]
                .as_array()
                .is_some_and(|fields| fields.iter().any(|field| field["key"] == "service.name"))
        })
        .expect("an exported resource");
    let mut keys: Vec<_> = resource["attributes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|field| field["key"].as_str().unwrap())
        .collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        [
            "morphiecore.build.revision",
            "service.name",
            "service.version"
        ]
    );
    let export =
        serde_json::to_string(&(collector.records(), collector.spans(), metadata)).unwrap();
    for canary in [
        support::CLIENT_KEY,
        "synthetic-upstream-credential-0001",
        "business-prompt",
        "synthetic business failure",
        "client-secret",
        "upstream-secret",
        "query-secret",
        "private-header",
        "forged-agent",
        "/chat/completions",
    ] {
        assert!(!export.contains(canary), "{canary}");
    }
}

#[tokio::test]
async fn unavailable_or_stalled_export_cannot_block_admission_or_a_healthy_generation() {
    use axum::{Json, body::Body, response::IntoResponse, routing::post};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tower::ServiceExt;
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = calls.clone();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let _upstream = Guard(tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new().route(
                "/chat/completions",
                post(move |Json(request): Json<Value>| {
                    let calls = observed.clone();
                    async move {
                        calls.fetch_add(1, Ordering::SeqCst);
                        if request["stream"] == true {
                            let wire = crate::chat_wire::events(2)
                                .iter()
                                .map(|event| format!("data: {event}\n\n"))
                                .collect::<String>()
                                + "data: [DONE]\n\n";
                            ([("content-type", "text/event-stream")], wire).into_response()
                        } else {
                            Json(crate::chat_wire::response(2)).into_response()
                        }
                    }
                }),
            ),
        )
        .await
        .unwrap();
    }));
    for stalled in [false, true] {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let _collector = if stalled {
            Some(Guard(tokio::spawn(async move {
                axum::serve(
                    listener,
                    Router::new().route(
                        "/v1/logs",
                        post(|| async { std::future::pending::<StatusCode>().await }),
                    ),
                )
                .await
                .unwrap();
            })))
        } else {
            drop(listener);
            None
        };
        let gate = support::gateway(&origin, Default::default())
            .with_diagnostics(morphiecore::gateway::DiagnosticsConfig {
                level: morphiecore::gateway::LogLevel::Warn,
                endpoint,
                private_bodies: false,
                body_bytes: 1024,
            })
            .unwrap();
        // Repeated failures and a healthy request must not wait for a stalled exporter.
        for _ in 0..80 {
            let response = tokio::time::timeout(
                Duration::from_secs(5),
                gate.router().oneshot(
                    axum::http::Request::builder()
                        .method("POST")
                        .uri("/v1/chat/completions")
                        .header("authorization", format!("Bearer {}", support::CLIENT_KEY))
                        .header("content-type", "application/json")
                        .body(Body::from("invalid JSON"))
                        .unwrap(),
                ),
            )
            .await
            .unwrap()
            .unwrap();
            assert_eq!(response.status(), 400);
            axum::body::to_bytes(response.into_body(), 4096)
                .await
                .unwrap();
        }
        let response = tokio::time::timeout(
            Duration::from_secs(5),
            gate.router().oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/v1/chat/completions")
                    .header("authorization", format!("Bearer {}", support::CLIENT_KEY))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({"model":"public-model","messages":[{"role":"user","content":"ok"}]})
                            .to_string(),
                    ))
                    .unwrap(),
            ),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(response.status(), 200);
        let body: Value = serde_json::from_slice(
            &axum::body::to_bytes(response.into_body(), 65536)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(body["choices"][0]["message"]["content"], "old 🧪");
        gate.shutdown();
    }
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}
