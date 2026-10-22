//! Explicit installed-client gates: CLI loader, synthetic authorities and isolated homes.
#![cfg(unix)]
#[path = "support/child_process.rs"]
mod child_process;
#[path = "support/filesystem.rs"]
mod filesystem;
use morphiecore::credential::{CredentialManager, CredentialPool, CredentialRef, Secret};
use std::{path::PathBuf, time::Duration};

async fn installed_case(
    script: &str,
    case: &str,
    variable: &str,
    arguments: &[&str],
) -> serde_json::Value {
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
        .arg(format!("tests/clients/{script}.py"))
        .arg(case)
        .arg(root)
        .arg(env!("CARGO_BIN_EXE_morphiecore"))
        .arg(binary)
        .args(arguments)
        .current_dir(env!("CARGO_MANIFEST_DIR"));
    let output = child_process::run(&mut command, b"", Duration::from_secs(120), 16 << 10)
        .await
        .expect("bounded installed-client process");
    assert!(
        output.status.success(),
        "{case} {script} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["case"], case);
    // The Python driver validates the supported major before launching the case.
    assert!(report["version"].as_str().is_some_and(|v| !v.is_empty()));
    eprintln!("{report}");
    report
}

async fn recovery_case(case: &str, variable: &str) {
    let report = installed_case("recovery", case, variable, &[]).await;
    assert_eq!(report["client_requests"], 4);
    assert_eq!(report["upstream_attempts"], 5);
    assert_eq!(report["extra_client_requests"], 0);
    assert_eq!(report["sticky_after_recovery"], true);
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

async fn pi_summary_case(case: &str, scenario: &str) {
    let report = installed_case("summary", case, "MORPHIECORE_PI_BIN", &[scenario]).await;
    let auxiliary = scenario != "ordinary";
    assert_eq!(report["client_requests"], if auxiliary { 7 } else { 6 });
    assert_eq!(report["upstream_attempts"], if auxiliary { 7 } else { 6 });
    assert_eq!(
        report["ordinary_summaries"],
        if scenario == "split" { 2 } else { 1 }
    );
    assert_eq!(report["summary_retries"], u8::from(scenario == "retry"));
    assert_eq!(report["rebuilt_tool_history"], true);
}

#[tokio::test]
#[ignore = "requires explicit unwrapped installed pi; ordinary Chat summary and rebuilt tools"]
async fn pi_chat_summary_and_rebuilt_tool_history() {
    pi_summary_case("pi-chat", "ordinary").await;
}

#[tokio::test]
#[ignore = "requires explicit unwrapped installed pi; ordinary Messages summary and rebuilt tools"]
async fn pi_messages_summary_and_rebuilt_tool_history() {
    pi_summary_case("pi-messages", "ordinary").await;
}

#[tokio::test]
#[ignore = "requires explicit unwrapped installed pi; split Messages summaries and rebuilt tools"]
async fn pi_messages_split_summary_and_rebuilt_tool_history() {
    pi_summary_case("pi-messages", "split").await;
}

#[tokio::test]
#[ignore = "requires explicit unwrapped installed pi; one bounded Chat summary retry"]
async fn pi_chat_summary_retry_and_rebuilt_tool_history() {
    pi_summary_case("pi-chat", "retry").await;
}

#[tokio::test]
#[ignore = "requires explicit unwrapped installed pi; synthetic strict-limit errors only"]
async fn pi_messages_strict_fallback_and_gateway_error() {
    let report = installed_case("strict_tools", "pi-messages", "MORPHIECORE_PI_BIN", &[]).await;
    assert_eq!(report["direct_client_requests"], 2);
    assert_eq!(report["direct_strict_fallbacks"], 1);
    assert_eq!(report["client_requests"], 1);
    assert_eq!(report["upstream_attempts"], 1);
    assert_eq!(report["gateway_strict_fallbacks"], 0);
    assert_eq!(report["honest_error"], true);
}

#[tokio::test]
#[ignore = "requires explicit unwrapped installed OpenCode; ordinary summary retry/nudge and tools"]
async fn opencode_chat_summary_retry_nudge_and_rebuilt_tool_history() {
    let report = installed_case(
        "opencode_summary",
        "opencode-chat",
        "MORPHIECORE_OPENCODE_BIN",
        &[],
    )
    .await;
    assert_eq!(report["client_requests"], 7);
    assert_eq!(report["upstream_attempts"], 7);
    assert_eq!(report["summary_retries"], 1);
    assert_eq!(report["summary_nudges"], 1);
    assert_eq!(report["rebuilt_tool_history"], true);
}
