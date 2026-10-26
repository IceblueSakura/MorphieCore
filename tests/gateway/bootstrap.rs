//! Binary CLI identity and isolated file/bootstrap process behavior.
#[path = "../support/child_line.rs"]
mod child_line;
#[path = "../support/child_process.rs"]
mod child_process;
use super::{Guard, support};
use crate::filesystem;
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
                &json!({"auth_file":"auth.json","bind":"127.0.0.1:0","proxy":"unsupported://invalid"}),
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
    let binding = morphiecore::topology::catalog::API_KEY_BINDINGS
        .iter()
        .find(|b| b.model == "deepseek-flash")
        .unwrap()
        .endpoint_id(morphiecore::topology::GenerationProtocol::OpenAiResponses);
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
#[cfg(unix)]
#[tokio::test]
async fn cli_diagnostics_rotate_recover_and_refuse_unowned_files_before_listening() {
    const SECOND_KEY: &str = "synthetic-second-agent-client-key-0001";
    use morphiecore::credential::{CredentialManager, CredentialPool, CredentialRef, Secret};
    use std::{
        io::Write,
        os::unix::fs::OpenOptionsExt,
        process::Stdio,
        sync::atomic::{AtomicUsize, Ordering},
    };
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = calls.clone();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let _upstream = Guard(tokio::spawn(async move {
        axum::serve(listener, Router::new().route("/v1/chat/completions", axum::routing::post(move || {
            observed.fetch_add(1, Ordering::SeqCst);
            async { axum::Json(json!({"id":"synthetic","object":"chat.completion","created":1,
                "model":"qwen3.8-flash","choices":[{"index":0,"message":{"role":"assistant","content":"done"},
                    "finish_reason":"stop"}],"usage":{"prompt_tokens":1,"completion_tokens":1,"total_tokens":2}})) }
        }))).await.unwrap();
    }));
    let directory = filesystem::private_directory();
    let root = directory.path();
    let write = |name: &str, bytes: &[u8]| {
        std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(root.join(name))
            .unwrap()
            .write_all(bytes)
            .unwrap();
    };
    let manager = CredentialManager::new(root.join("credentials"), vec![]).unwrap();
    manager
        .add_api_key(
            "aliyun-dashscope-cn",
            "one",
            Secret::new("synthetic-rotation-upstream".into()).unwrap(),
        )
        .unwrap();
    manager
        .set_pool(
            "aliyun-dashscope-cn",
            "aliyun-dashscope-cn-api-key",
            0,
            CredentialPool {
                members: vec![CredentialRef::ApiKey {
                    alias: "one".into(),
                }],
                fallback: false,
                max_attempts: 1,
            },
        )
        .unwrap();
    write(
        "auth.json",
        &serde_json::to_vec(&json!({"version":1,"agents":[
            {"agent_id":"fixture-agent","key":support::CLIENT_KEY},
            {"agent_id":"second-agent","name":"Private display label","key":SECOND_KEY,
                "desc":"Private maintenance note"}
        ]}))
        .unwrap(),
    );
    let mut config = json!({"auth_file":"auth.json","bind":"127.0.0.1:0",
        "models_file":"models.json","diagnostics":"diagnostics.jsonl","exchange_timeout_ms":5000});
    write("gateway.json", &serde_json::to_vec(&config).unwrap());
    write(
        "models.json",
        &serde_json::to_vec(&json!({"models":{"qwen3.8-flash":{"max_attempts":1,
        "candidates":[{"binding":"dashscope-flash-chat","base_url":format!("{origin}/v1"),
            "attempt_timeout_ms":3000}]}}}))
        .unwrap(),
    );
    let header = b"{\"kind\":\"morphiecore_diagnostics\",\"version\":1,\"event\":\"file\"}\n";
    let start =
        b"{\"kind\":\"morphiecore_diagnostics\",\"version\":1,\"event\":\"file\",\"padding\":\"";
    let mut seed = header.to_vec();
    seed.extend(start);
    seed.extend(vec![b'x'; (1 << 20) - header.len() - start.len() - 3]);
    seed.extend(b"\"}\n");
    assert_eq!(seed.len(), 1 << 20);
    write("diagnostics.jsonl", &seed);
    let command = || {
        let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_morphiecore"));
        child
            .arg("--config")
            .arg(root.join("gateway.json"))
            .arg("--credentials-dir")
            .arg(root.join("credentials"))
            .env_clear()
            .env("HOME", root)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        child
    };
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(3))
        .build()
        .unwrap();
    let path = root.join("diagnostics.jsonl");
    for run in 0..2 {
        if run == 1 {
            std::fs::OpenOptions::new()
                .append(true)
                .open(&path)
                .unwrap()
                .write_all(b"{\"event\":")
                .unwrap();
        }
        let mut child = command().spawn().unwrap();
        let mut output = tokio::io::BufReader::new(child.stdout.take().unwrap());
        let ready = match child_line::read_line(&mut output, Duration::from_secs(3), 127).await {
            Ok(line) => line,
            Err(error) => {
                use tokio::io::AsyncReadExt;
                let mut message = Vec::new();
                tokio::time::timeout(
                    Duration::from_secs(3),
                    child
                        .stderr
                        .take()
                        .unwrap()
                        .take(4096)
                        .read_to_end(&mut message),
                )
                .await
                .unwrap()
                .unwrap();
                panic!(
                    "synthetic CLI {error}: {}",
                    String::from_utf8_lossy(&message)
                );
            }
        };
        let base = ready
            .trim()
            .strip_prefix("MorphieCore listening on http://127.0.0.1:")
            .unwrap();
        assert!(!base.is_empty() && base.bytes().all(|b| b.is_ascii_digit()));
        let response = client
            .post(format!("http://127.0.0.1:{base}/v1/chat/completions"))
            .bearer_auth(if run == 0 {
                support::CLIENT_KEY
            } else {
                SECOND_KEY
            })
            .header("x-morphiecore-agent-id", "forged-agent")
            .header("x-private-business", "synthetic-arbitrary-header-marker")
            .json(&json!({"model":"qwen3.8-flash","max_completion_tokens":32,
                "reasoning_effort":"none",
                "messages":[{"role":"user","content":"synthetic-private-prompt-marker"}]}))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        assert_eq!(
            response.json::<Value>().await.unwrap()["choices"][0]["message"]["content"],
            "done"
        );
        assert_eq!(
            unsafe { libc::kill(child.id().unwrap() as libc::pid_t, libc::SIGINT) },
            0
        );
        assert!(
            tokio::time::timeout(Duration::from_secs(5), child.wait())
                .await
                .unwrap()
                .unwrap()
                .success()
        );
        for file in [&path, &root.join("diagnostics.jsonl.1")] {
            let data = std::fs::read(file).unwrap();
            assert!(data.len() <= 1 << 20);
            for line in data
                .split(|byte| *byte == b'\n')
                .filter(|line| !line.is_empty())
            {
                serde_json::from_slice::<Value>(line).unwrap();
            }
            let data = String::from_utf8(data).unwrap();
            for marker in [
                support::CLIENT_KEY,
                SECOND_KEY,
                "Private display label",
                "Private maintenance note",
                "forged-agent",
                "synthetic-rotation-upstream",
                "synthetic-private-prompt-marker",
                "synthetic-arbitrary-header-marker",
            ] {
                assert!(!data.contains(marker));
            }
        }
    }
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    let records: Vec<Value> = std::fs::read_to_string(&path)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let requests: Vec<_> = records.iter().filter(|r| r["event"] == "request").collect();
    assert_eq!(requests.len(), 2);
    assert!(
        requests
            .iter()
            .all(|r| r["outcome"] == "complete" && r["candidates"].as_array().unwrap().len() == 1)
    );
    assert_eq!(requests[0]["agent_id"], "fixture-agent");
    assert_eq!(requests[1]["agent_id"], "second-agent");
    assert_ne!(
        requests[0]["request_id"]
            .as_str()
            .unwrap()
            .split(':')
            .next(),
        requests[1]["request_id"]
            .as_str()
            .unwrap()
            .split(':')
            .next()
    );
    let health = records
        .iter()
        .find(|r| r["event"] == "logging_status")
        .unwrap();
    assert_eq!(health["recovered_tail_bytes"], 9);
    for counter in ["dropped", "failed", "flush_incomplete"] {
        assert_eq!(health[counter], 0);
    }
    assert_eq!(health["writer_failed"], false);
    assert_eq!(
        std::fs::read(root.join("diagnostics.jsonl.1")).unwrap(),
        seed
    );
    for capture in [false, true] {
        if capture {
            write("capture.jsonl", b"synthetic-existing-capture");
            config["capture"] =
                json!({"path":root.join("capture.jsonl"),"client_request":{"headers":true}});
            config["diagnostics"] = json!("diagnostics.jsonl");
        } else {
            write("foreign.log", b"synthetic-unrelated-file");
            config["diagnostics"] = json!("foreign.log");
        }
        write("gateway.json", &serde_json::to_vec(&config).unwrap());
        let output = child_process::run(&mut command(), b"", Duration::from_secs(5), 16 << 10)
            .await
            .unwrap();
        assert!(!output.status.success() && output.stdout.is_empty());
        let (name, sentinel) = if capture {
            ("capture.jsonl", b"synthetic-existing-capture".as_slice())
        } else {
            ("foreign.log", b"synthetic-unrelated-file".as_slice())
        };
        assert_eq!(std::fs::read(root.join(name)).unwrap(), sentinel);
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }
}

#[tokio::test]
async fn capture_retention_saturation_keeps_http_healthy_and_auth_redacted() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = calls.clone();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let _upstream = Guard(tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new().fallback(move || {
                observed.fetch_add(1, Ordering::SeqCst);
                async { StatusCode::BAD_GATEWAY }
            }),
        )
        .await
        .unwrap();
    }));
    let directory = filesystem::private_directory();
    let path = directory.path().join("capture.jsonl");
    let gate = support::gateway(&origin, Default::default())
        .with_capture(
            serde_json::from_value(json!({"path":path,"client_request":{"headers":true},
            "max_unit_bytes":1024,"max_exchange_bytes":1024,"max_total_bytes":1024}))
            .unwrap(),
        )
        .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let (shutdown, stopped) = tokio::sync::oneshot::channel();
    let server = gate.clone().serve(listener, async {
        let _ = stopped.await;
    });
    let mut serving = Guard(tokio::spawn(async move { server.await.unwrap() }));
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(3))
        .build()
        .unwrap();
    for _ in 0..8 {
        let response = client
            .get(format!("{origin}/v1/models"))
            .bearer_auth(support::CLIENT_KEY)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        response.bytes().await.unwrap();
        gate.flush_diagnostics().await;
    }
    let data = std::fs::read_to_string(&path).unwrap();
    assert!(
        data.len() <= 1024 && data.contains("[REDACTED]") && !data.contains(support::CLIENT_KEY)
    );
    let records: Vec<Value> = data
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert!(!records.is_empty() && records.len() < 8);
    assert!(
        records
            .iter()
            .all(|record| record["boundary"] == "client_request")
    );
    let status = gate.capture_status().unwrap();
    assert_eq!(status.queue_dropped, 0);
    assert!(!status.write_failed);
    assert_eq!(status.retention_dropped, 8 - records.len() as u64);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    shutdown.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(3), &mut serving.0)
        .await
        .unwrap()
        .unwrap();
}
