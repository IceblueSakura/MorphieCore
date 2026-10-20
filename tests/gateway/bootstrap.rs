//! Binary CLI identity and isolated file/bootstrap process behavior.
#[path = "../support/child_line.rs"]
mod child_line;
#[path = "../support/child_process.rs"]
mod child_process;
use super::{Guard, support};
use axum::{Router, http::StatusCode};
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};
use tokio::net::TcpListener;

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
    use morphiecore::credential::{CredentialPool, CredentialRef, Secret};
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
    manager
        .set_pool(
            "deepseek",
            "deepseek-api-key",
            0,
            morphiecore::credential::CredentialPool {
                members: vec![morphiecore::credential::CredentialRef::ApiKey {
                    alias: "one".into(),
                }],
                fallback: false,
                max_attempts: 1,
            },
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
    manager
        .set_pool(
            "openrouter",
            "openrouter-api-key",
            0,
            CredentialPool {
                members: vec![CredentialRef::ApiKey {
                    alias: "speech".into(),
                }],
                fallback: false,
                max_attempts: 1,
            },
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
    options
        .open(&path)
        .unwrap()
        .write_all(
            &serde_json::to_vec(
                &json!({"client_key":support::CLIENT_KEY,"bind":"127.0.0.1:0","proxy":"unsupported://invalid"}),
            )
            .unwrap(),
        )
        .unwrap();
    let binding = morphiecore::topology::catalog::API_KEY_BINDINGS
        .iter()
        .find(|b| b.model == "deepseek-flash")
        .unwrap()
        .endpoint_id(morphiecore::topology::ProtocolProfile::OpenAiResponses);
    options
        .open(root.parent().unwrap().join("models.json"))
        .unwrap()
        .write_all(
            &serde_json::to_vec(&json!({"models":{
                "deepseek-flash":{"candidates":[{"binding":binding.as_str()}]},
                "qwen-audio-3.0-tts-flash":{}
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
    // No admitted request is issued: the synthetic key must never reach a Provider.
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    process.kill().await.unwrap();
    process.wait().await.unwrap();
}
