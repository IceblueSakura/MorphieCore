//! Explicit installed-client gates: CLI loader, synthetic authorities and isolated homes.
#![cfg(unix)]
#[path = "support/child_process.rs"]
mod child_process;
#[path = "support/filesystem.rs"]
mod filesystem;
use morphiecore::credential::{CredentialManager, CredentialPool, CredentialRef, Secret};
use std::{path::PathBuf, time::Duration};

async fn recovery_case(case: &str, variable: &str) {
    // Explicit unwrapped binaries only: PATH entries may source host credentials.
    let binary =
        PathBuf::from(std::env::var_os(variable).expect("set the unwrapped client binary"));
    assert!(binary.is_absolute() && binary.is_file());
    let directory = filesystem::private_directory();
    let root = directory.path();
    let native = case == "pi-messages";
    let (domain, binding) = if native {
        ("opencode-go", "opencode-go-api-key")
    } else {
        ("aliyun-dashscope-cn", "aliyun-dashscope-cn-api-key")
    };
    let manager = CredentialManager::new(root.join("credentials"), vec![]).unwrap();
    for alias in ["primary", "secondary"] {
        manager
            .add_api_key(
                domain,
                alias,
                Secret::new(format!("synthetic-client-{alias}")).unwrap(),
            )
            .unwrap();
    }
    manager
        .set_pool(
            domain,
            binding,
            0,
            CredentialPool {
                members: ["primary", "secondary"]
                    .into_iter()
                    .map(|alias| CredentialRef::ApiKey {
                        alias: alias.into(),
                    })
                    .collect(),
                fallback: true,
                max_attempts: 2,
            },
        )
        .unwrap();
    let mut command = tokio::process::Command::new(
        std::env::var_os("MORPHIECORE_CLIENT_PYTHON").unwrap_or_else(|| "python3".into()),
    );
    command
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .env("HOME", root)
        .args(["tests/clients/recovery.py", case])
        .arg(root)
        .arg(env!("CARGO_BIN_EXE_morphiecore"))
        .arg(binary)
        .current_dir(env!("CARGO_MANIFEST_DIR"));
    let output = child_process::run(&mut command, b"", Duration::from_secs(120), 16 << 10)
        .await
        .expect("bounded installed-client recovery process");
    assert!(
        output.status.success(),
        "{case} recovery failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["case"], case);
    assert_eq!(
        report["version"],
        if case == "opencode-chat" {
            "opencode v2.0.6+e5ecb57"
        } else {
            "1.3.0"
        }
    );
    assert_eq!(report["client_requests"], 4);
    assert_eq!(report["upstream_attempts"], 5);
    assert_eq!(report["extra_client_requests"], 0);
    assert_eq!(report["sticky_after_recovery"], true);
    eprintln!("{report}");
}

#[tokio::test]
#[ignore = "requires explicit unwrapped installed pi; synthetic Chat recovery, no Provider"]
async fn pi_chat_credential_recovery() {
    recovery_case("pi-chat", "MORPHIECORE_PI_BIN").await;
}

#[tokio::test]
#[ignore = "requires explicit unwrapped installed pi; synthetic native Messages recovery"]
async fn pi_messages_credential_recovery() {
    recovery_case("pi-messages", "MORPHIECORE_PI_BIN").await;
}

#[tokio::test]
#[ignore = "requires explicit unwrapped installed OpenCode; synthetic Chat recovery"]
async fn opencode_chat_credential_recovery() {
    recovery_case("opencode-chat", "MORPHIECORE_OPENCODE_BIN").await;
}
