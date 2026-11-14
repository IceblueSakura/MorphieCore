//! Named-account API-key CLI input, redaction and storage are independently checked.
use crate::test_files::Directory;
use serde_json::{Value, json};
use std::{io::Write, os::unix::fs::OpenOptionsExt, time::Duration};
use tokio::process::Command;

fn private_write(path: &std::path::Path, bytes: &[u8]) {
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .unwrap()
        .write_all(bytes)
        .unwrap();
}
async fn command(dir: &Directory, args: &[&str], input: Option<&[u8]>) -> std::process::Output {
    let files = crate::test_files::private_directory();
    let input_file = files.path().join("input.key");
    if let Some(input) = input {
        private_write(&input_file, input);
    }
    let accounts = files.path().join("accounts.json");
    private_write(
        &accounts,
        &serde_json::to_vec(&json!({"version":1,"providers":{"openai":{"accounts":[
            {"name":"primary","enabled":true,"credential":{"alias":"one"}},
            {"name":"backup","enabled":true,"credential":{"alias":"two"}}
        ]}}}))
        .unwrap(),
    );
    let args: Vec<_> = args
        .iter()
        .map(|value| {
            if *value == "INPUT_FILE" {
                input_file.to_str().unwrap()
            } else {
                value
            }
        })
        .collect();
    let mut command = Command::new(env!("CARGO_BIN_EXE_morphiecore-auth"));
    command
        .args(args)
        .arg("--store")
        .arg(&dir.path)
        .arg("--accounts")
        .arg(accounts)
        .env_clear();
    crate::child_process::run(&mut command, b"", Duration::from_secs(10), 64 << 10)
        .await
        .expect("bounded key CLI operation")
}
#[tokio::test]
async fn api_key_cli_uses_configured_account_names_and_never_reports_material() {
    let dir = Directory::new();
    let added = command(
        &dir,
        &[
            "key",
            "add",
            "--provider",
            "openai",
            "--account",
            "primary",
            "--secret-file",
            "INPUT_FILE",
            "--json",
        ],
        Some(b"synthetic-private-value\n"),
    )
    .await;
    assert!(
        added.status.success(),
        "{}",
        String::from_utf8_lossy(&added.stderr)
    );
    let status: Value = serde_json::from_slice(&added.stdout).unwrap();
    assert_eq!(status["revision"], 1);
    assert_eq!(status["state"], "enabled");
    assert_eq!(status["account"], "primary");
    let doc: Value =
        serde_json::from_slice(&std::fs::read(dir.path.join("openai.json")).unwrap()).unwrap();
    assert_eq!(doc["version"], 1);
    let record = &doc["api_keys"]["one"];
    assert_eq!(record["kind"], "api_key");
    assert_eq!(record["secret"], "synthetic-private-value");
    assert!(record.get("client_id").is_none());
    let listed = command(&dir, &["status", "--provider", "openai", "--json"], None).await;
    assert!(listed.status.success());
    let inventory: Value = serde_json::from_slice(&listed.stdout).unwrap();
    assert_eq!(inventory["accounts"][0]["revision"], 1);
    assert_eq!(inventory["accounts"][1]["state"], "unprovisioned");
    let human = command(&dir, &["status", "--provider", "openai"], None).await;
    assert!(human.status.success());
    let text = String::from_utf8_lossy(&human.stdout);
    assert!(text.contains("openai/primary:") && text.contains("revision=1"));
    assert!(text.contains("openai/backup:") && text.contains("unprovisioned"));
    for output in [&added, &listed, &human] {
        for bytes in [&output.stdout, &output.stderr] {
            let text = String::from_utf8_lossy(bytes);
            for private in [
                "private-value",
                record["record_id"].as_str().unwrap(),
                record["epoch"].as_str().unwrap(),
                "\"alias\"",
            ] {
                assert!(!text.contains(private));
            }
        }
    }
    let stale = command(
        &dir,
        &[
            "key",
            "remove",
            "--provider",
            "openai",
            "--account",
            "primary",
            "--revision",
            "0",
        ],
        None,
    )
    .await;
    assert!(!stale.status.success());
    let removed = command(
        &dir,
        &[
            "key",
            "remove",
            "--provider",
            "openai",
            "--account",
            "primary",
            "--revision",
            "1",
        ],
        None,
    )
    .await;
    assert!(removed.status.success());
    let record: Value =
        serde_json::from_slice::<Value>(&std::fs::read(dir.path.join("openai.json")).unwrap())
            .unwrap()["api_keys"]["one"]
            .take();
    assert!(record["secret"].is_null());
}
#[tokio::test]
async fn api_key_cli_rejects_argv_secrets_and_invalid_input_before_creating_store() {
    for args in [
        vec![
            "key",
            "add",
            "--provider",
            "openai",
            "--account",
            "primary",
            "--key",
            "never-print-this",
        ],
        vec!["key", "add", "--provider", "openai", "--account", "primary"],
    ] {
        let dir = Directory::new();
        let output = command(&dir, &args, None).await;
        assert!(!output.status.success());
        assert!(!String::from_utf8_lossy(&output.stderr).contains("never-print-this"));
        assert!(!dir.path.exists());
    }
    for input in [
        b"".as_slice(),
        b"synthetic\r\nInjected: header",
        b"synthetic space",
    ] {
        let dir = Directory::new();
        let output = command(
            &dir,
            &[
                "key",
                "add",
                "--provider",
                "openai",
                "--account",
                "primary",
                "--secret-file",
                "INPUT_FILE",
            ],
            Some(input),
        )
        .await;
        assert!(!output.status.success());
        assert!(!dir.path.exists());
    }
    let dir = Directory::new();
    let output = command(
        &dir,
        &[
            "key",
            "replace",
            "--provider",
            "openai",
            "--account",
            "primary",
            "--revision",
            "1",
            "--secret-file",
            "INPUT_FILE",
        ],
        Some(b""),
    )
    .await;
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(!dir.path.exists());
}
#[tokio::test]
async fn unknown_provider_and_credential_alias_do_not_bypass_account_configuration() {
    for (provider, account) in [("../escape", "primary"), ("openai", "one")] {
        let dir = Directory::new();
        let output = command(
            &dir,
            &[
                "key",
                "add",
                "--provider",
                provider,
                "--account",
                account,
                "--secret-file",
                "INPUT_FILE",
            ],
            Some(b"synthetic-private-value"),
        )
        .await;
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        assert!(!dir.path.exists());
        assert!(!dir.path.parent().unwrap().join("escape.json").exists());
    }
}
