//! Explicit installed-client gates: CLI loader, synthetic authorities and isolated homes.
#![cfg(unix)]
#[path = "support/child_process.rs"]
mod child_process;
#[path = "support/filesystem.rs"]
mod filesystem;
use morphiecore::credential::{CredentialManager, Secret};
use std::{path::PathBuf, time::Duration};

async fn installed_case(script: &str, case: &str, variable: &str, arguments: &[&str]) {
    // Explicit unwrapped binaries only: PATH entries may source host credentials.
    let binary =
        PathBuf::from(std::env::var_os(variable).expect("set the unwrapped client binary"));
    assert!(binary.is_absolute() && binary.is_file());
    let directory = filesystem::private_directory();
    let root = directory.path();
    let native = case.contains("messages");
    let domain = if script == "google_owner_replay" {
        "google"
    } else if script == "protocol_defaults" && case.ends_with("-signed") {
        "opencode-go"
    } else if script == "protocol_defaults" && case.contains("chat") {
        // pi's builtin strict read tool routes through the beta endpoint binding.
        if case.starts_with("pi-chat") {
            "deepseek-beta"
        } else {
            "deepseek"
        }
    } else if case.contains("responses") {
        "openai"
    } else if native {
        "opencode-go"
    } else {
        "aliyun-tokenplan-cn"
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
    if script == "lifecycle"
        && let Some(nginx) = std::env::var_os("MORPHIECORE_NGINX_BIN")
    {
        let nginx = PathBuf::from(nginx);
        assert!(nginx.is_absolute() && nginx.is_file());
        command.arg(nginx);
    }
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
    // The Python driver records the current host binary's actual version.
    assert!(report["version"].as_str().is_some_and(|v| !v.is_empty()));
    eprintln!("{report}");
}

#[tokio::test]
#[ignore = "requires current pi and explicit replay extension source; native Google signatures and session resume"]
async fn pi_responses_google_owner_replay() {
    let source = std::env::var("MORPHIECORE_PI_REPLAY_SOURCE")
        .expect("set the workstation replay.ts source path");
    assert!(std::path::Path::new(&source).is_absolute());
    installed_case(
        "google_owner_replay",
        "pi-responses-google-owner-replay",
        "MORPHIECORE_PI_BIN",
        &[&source],
    )
    .await;
}

#[tokio::test]
#[ignore = "requires explicit unwrapped installed pi; synthetic Chat recovery, no Provider"]
async fn pi_chat_credential_recovery() {
    installed_case("recovery", "pi-chat", "MORPHIECORE_PI_BIN", &[]).await;
}

#[tokio::test]
#[ignore = "requires current pi; Readable Chat signed and opaque thinking through native Messages"]
async fn pi_chat_signed_reasoning_tool_history() {
    installed_case(
        "protocol_defaults",
        "pi-chat-signed",
        "MORPHIECORE_PI_BIN",
        &[],
    )
    .await;
}

#[tokio::test]
#[ignore = "requires current OpenCode; Readable Chat signed and opaque thinking through native Messages"]
async fn opencode_chat_signed_reasoning_tool_history() {
    installed_case(
        "protocol_defaults",
        "opencode-chat-signed",
        "MORPHIECORE_OPENCODE_BIN",
        &[],
    )
    .await;
}

#[tokio::test]
#[ignore = "requires explicit unwrapped installed pi; synthetic native Messages recovery"]
async fn pi_messages_credential_recovery() {
    installed_case("recovery", "pi-messages", "MORPHIECORE_PI_BIN", &[]).await;
}

#[tokio::test]
#[ignore = "requires current installed pi; default native Messages tool/history through gateway"]
async fn pi_messages_default_tool_history() {
    installed_case(
        "protocol_defaults",
        "pi-messages-default",
        "MORPHIECORE_PI_BIN",
        &[],
    )
    .await;
}

#[tokio::test]
#[ignore = "requires current installed OpenCode; default native Messages tool/history through gateway"]
async fn opencode_messages_default_tool_history() {
    installed_case(
        "protocol_defaults",
        "opencode-messages-default",
        "MORPHIECORE_OPENCODE_BIN",
        &[],
    )
    .await;
}

#[tokio::test]
#[ignore = "requires current installed OpenCode; default native Responses tool/history through gateway"]
async fn opencode_responses_default_tool_history() {
    installed_case(
        "protocol_defaults",
        "opencode-responses-default",
        "MORPHIECORE_OPENCODE_BIN",
        &[],
    )
    .await;
}

#[tokio::test]
#[ignore = "requires current installed OpenCode; native Chat defaults and tool/history"]
async fn opencode_chat_default_tool_history() {
    installed_case(
        "protocol_defaults",
        "opencode-chat-default",
        "MORPHIECORE_OPENCODE_BIN",
        &[],
    )
    .await;
}

#[tokio::test]
#[ignore = "requires current installed pi; builtin Chat strict tools dispatch through the beta binding"]
async fn pi_chat_default_strict_guarantee() {
    installed_case(
        "protocol_defaults",
        "pi-chat-default",
        "MORPHIECORE_PI_BIN",
        &[],
    )
    .await;
}

#[tokio::test]
#[ignore = "requires current installed pi; shipped gateway profile and strict tool/history loop"]
async fn pi_chat_gateway_profile() {
    installed_case(
        "protocol_defaults",
        "pi-chat-gateway-profile",
        "MORPHIECORE_PI_BIN",
        &[],
    )
    .await;
}

#[tokio::test]
#[ignore = "requires current installed pi; builtin Responses inline tools and direct-call history"]
async fn pi_responses_default_tool_history() {
    installed_case(
        "protocol_defaults",
        "pi-responses-default",
        "MORPHIECORE_PI_BIN",
        &[],
    )
    .await;
}

#[tokio::test]
#[ignore = "requires explicit unwrapped installed OpenCode; synthetic Chat recovery"]
async fn opencode_chat_credential_recovery() {
    installed_case("recovery", "opencode-chat", "MORPHIECORE_OPENCODE_BIN", &[]).await;
}

#[tokio::test]
#[ignore = "requires current installed pi; long hint and next-request account selection"]
async fn pi_chat_long_hint_and_next_request_failover() {
    installed_case(
        "recovery_boundary",
        "pi-chat",
        "MORPHIECORE_PI_BIN",
        &["next-request"],
    )
    .await;
}

#[tokio::test]
#[ignore = "requires current installed pi; two same-target failures and no third send"]
async fn pi_chat_two_send_recovery_limit() {
    installed_case(
        "recovery_boundary",
        "pi-chat",
        "MORPHIECORE_PI_BIN",
        &["retry-limit"],
    )
    .await;
}

#[tokio::test]
#[ignore = "requires current installed pi; cancel after retry hint without a second send"]
async fn pi_chat_cancel_recovery_wait() {
    installed_case(
        "recovery_boundary",
        "pi-chat",
        "MORPHIECORE_PI_BIN",
        &["cancel-wait"],
    )
    .await;
}

#[tokio::test]
#[ignore = "requires explicit unwrapped installed pi; ordinary Chat summary and rebuilt tools"]
async fn pi_chat_summary_and_rebuilt_tool_history() {
    installed_case("summary", "pi-chat", "MORPHIECORE_PI_BIN", &["ordinary"]).await;
}

#[tokio::test]
#[ignore = "requires explicit unwrapped installed pi; ordinary Messages summary and rebuilt tools"]
async fn pi_messages_summary_and_rebuilt_tool_history() {
    installed_case(
        "summary",
        "pi-messages",
        "MORPHIECORE_PI_BIN",
        &["ordinary"],
    )
    .await;
}

#[tokio::test]
#[ignore = "requires explicit unwrapped installed pi; split Messages summaries and rebuilt tools"]
async fn pi_messages_split_summary_and_rebuilt_tool_history() {
    installed_case("summary", "pi-messages", "MORPHIECORE_PI_BIN", &["split"]).await;
}

#[tokio::test]
#[ignore = "requires explicit unwrapped installed pi; one bounded Chat summary retry"]
async fn pi_chat_summary_retry_and_rebuilt_tool_history() {
    installed_case("summary", "pi-chat", "MORPHIECORE_PI_BIN", &["retry"]).await;
}

#[tokio::test]
#[ignore = "requires explicit unwrapped installed pi; synthetic strict-limit errors only"]
async fn pi_messages_strict_fallback_and_gateway_error() {
    installed_case("strict_tools", "pi-messages", "MORPHIECORE_PI_BIN", &[]).await;
}

#[tokio::test]
#[ignore = "requires explicit unwrapped installed OpenCode; ordinary summary retry/nudge and tools"]
async fn opencode_chat_summary_retry_nudge_and_rebuilt_tool_history() {
    installed_case(
        "opencode_summary",
        "opencode-chat",
        "MORPHIECORE_OPENCODE_BIN",
        &[],
    )
    .await;
}

#[tokio::test]
#[ignore = "requires installed pi; held Chat content, cancellation and single-permit recovery"]
async fn pi_chat_incremental_lifecycle() {
    installed_case("lifecycle", "pi-chat", "MORPHIECORE_PI_BIN", &[]).await;
}

#[tokio::test]
#[ignore = "requires installed pi; held Messages content, cancellation and single-permit recovery"]
async fn pi_messages_incremental_lifecycle() {
    installed_case("lifecycle", "pi-messages", "MORPHIECORE_PI_BIN", &[]).await;
}

#[tokio::test]
#[ignore = "requires installed pi; incomplete Responses write, cancellation and new-write feedback"]
async fn pi_responses_tool_argument_cancellation() {
    installed_case(
        "lifecycle",
        "pi-responses-tool-cancel",
        "MORPHIECORE_PI_BIN",
        &[],
    )
    .await;
}

#[tokio::test]
#[ignore = "requires installed OpenCode; core delta, interrupt and single-permit recovery"]
async fn opencode_chat_incremental_lifecycle() {
    installed_case(
        "lifecycle",
        "opencode-chat",
        "MORPHIECORE_OPENCODE_BIN",
        &[],
    )
    .await;
}
