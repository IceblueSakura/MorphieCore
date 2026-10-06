//! API-key CLI input and on-disk shape are checked independently of its encoder.
use crate::test_files::Directory;
use serde_json::Value;
use std::{process::Stdio, time::Duration};
use tokio::{process::Command, time::timeout};

async fn command(dir: &Directory, args: &[&str], input: Option<&[u8]>) -> std::process::Output {
    use std::os::unix::fs::PermissionsExt;
    let input_dir = tempfile::Builder::new()
        .permissions(std::fs::Permissions::from_mode(0o700))
        .tempdir()
        .unwrap();
    let input_file = input_dir.path().join("input.key");
    if let Some(bytes) = input {
        use std::io::Write;
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        options.open(&input_file).unwrap().write_all(bytes).unwrap();
    }
    let args: Vec<_> = args
        .iter()
        .map(|v| {
            if *v == "INPUT_FILE" {
                input_file.to_str().unwrap()
            } else {
                v
            }
        })
        .collect();
    let child = Command::new(env!("CARGO_BIN_EXE_morphiecore-auth"))
        .args(args)
        .arg("--store")
        .arg(&dir.path)
        .env_clear()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    timeout(Duration::from_secs(10), child.wait_with_output())
        .await
        .unwrap()
        .unwrap()
}
#[tokio::test]
async fn api_key_cli_uses_explicit_file_and_never_reports_material() {
    let dir = Directory::new();
    let args = [
        "api-key",
        "add",
        "--domain",
        "synthetic",
        "--alias",
        "one",
        "--secret-file",
        "INPUT_FILE",
    ];
    let added = command(&dir, &args, Some(b"synthetic-private-value\n")).await;
    assert!(
        added.status.success(),
        "{}",
        String::from_utf8_lossy(&added.stderr)
    );
    let status: Value = serde_json::from_slice(&added.stdout).unwrap();
    assert_eq!(status["revision"], 1);
    assert_eq!(status["state"], "enabled");
    let record: Value =
        serde_json::from_slice::<Value>(&std::fs::read(dir.path.join("synthetic.json")).unwrap())
            .unwrap()["api_keys"]["one"]
            .take();
    assert_eq!(record["kind"], "api_key");
    assert_eq!(record["secret"], "synthetic-private-value");
    assert!(record.get("client_id").is_none());
    let listed = command(&dir, &["list"], None).await;
    assert!(listed.status.success());
    let inventory: Value = serde_json::from_slice(&listed.stdout).unwrap();
    assert_eq!(inventory[0]["kind"], "api_key");
    assert_eq!(inventory[0]["domain"], "synthetic");
    for output in [&added, &listed] {
        for bytes in [&output.stdout, &output.stderr] {
            let text = String::from_utf8_lossy(bytes);
            assert!(!text.contains("private-value"));
            assert!(!text.contains(record["record_id"].as_str().unwrap()));
            assert!(!text.contains(record["epoch"].as_str().unwrap()));
        }
    }
    let stale = command(
        &dir,
        &[
            "api-key",
            "remove",
            "--domain",
            "synthetic",
            "--alias",
            "one",
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
            "api-key",
            "remove",
            "--domain",
            "synthetic",
            "--alias",
            "one",
            "--revision",
            "1",
        ],
        None,
    )
    .await;
    assert!(removed.status.success());
    let record: Value =
        serde_json::from_slice::<Value>(&std::fs::read(dir.path.join("synthetic.json")).unwrap())
            .unwrap()["api_keys"]["one"]
            .take();
    assert!(record["secret"].is_null());
}
#[tokio::test]
async fn api_key_cli_rejects_argv_secrets_and_invalid_input_without_creating_store() {
    for args in [
        vec![
            "api-key",
            "add",
            "--domain",
            "synthetic",
            "--alias",
            "one",
            "--key",
            "never-print-this",
        ],
        vec!["api-key", "add", "--domain", "synthetic", "--alias", "one"],
        vec![
            "api-key",
            "add",
            "--domain",
            "../escape",
            "--alias",
            "one",
            "--secret-fd",
            "0",
        ],
        vec!["api-key", "list", "--secret-fd", "0"],
    ] {
        let dir = Directory::new();
        let result = command(&dir, &args, None).await;
        assert!(!result.status.success());
        assert!(!String::from_utf8_lossy(&result.stderr).contains("never-print-this"));
        assert!(!dir.path.exists());
    }
    for input in [
        b"".as_slice(),
        b"synthetic\r\nInjected: header",
        b"synthetic space",
    ] {
        let dir = Directory::new();
        let result = command(
            &dir,
            &[
                "api-key",
                "add",
                "--domain",
                "synthetic",
                "--alias",
                "one",
                "--secret-file",
                "INPUT_FILE",
            ],
            Some(input),
        )
        .await;
        assert!(!result.status.success());
        assert!(!dir.path.exists());
    }
}
