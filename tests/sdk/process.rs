//! Synthetic child failures exercise the shared SDK/native-probe process boundary.
use std::{
    io::{Read, Write},
    time::Duration,
};
use tokio::process::Command;

#[test]
fn fixture() {
    match std::env::var("MORPHIECORE_SYNTHETIC_CHILD").as_deref() {
        Ok("wait") => loop {
            std::thread::park();
        },
        Ok("flood") => {
            let _ = std::io::stdout().write_all(&[b'x'; 65536]);
        }
        Ok("fail") => std::process::exit(7),
        Ok("echo") => {
            let mut input = Vec::new();
            std::io::stdin().read_to_end(&mut input).unwrap();
            std::io::stdout().write_all(&input).unwrap();
            std::io::stderr().write_all(b"synthetic-stderr").unwrap();
        }
        _ => {}
    }
}
fn command(mode: &str) -> Command {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", "process_tests::fixture", "--nocapture"])
        .env_clear()
        .env("MORPHIECORE_SYNTHETIC_CHILD", mode);
    command
}
#[tokio::test]
async fn child_limits_timeout_exit_and_concurrent_capture_are_distinct() {
    for (mode, expected) in [("wait", "child timeout"), ("flood", "child output limit")] {
        let result =
            crate::child_process::run(&mut command(mode), b"", Duration::from_millis(500), 512)
                .await;
        assert_eq!(result.unwrap_err(), expected);
    }
    let output = crate::child_process::run(
        &mut command("echo"),
        b"synthetic-input",
        Duration::from_secs(3),
        1024,
    )
    .await
    .unwrap();
    assert!(output.status.success());
    assert!(
        String::from_utf8(output.stdout)
            .unwrap()
            .contains("synthetic-input")
    );
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("synthetic-stderr")
    );
    let output = crate::child_process::run(&mut command("fail"), b"", Duration::from_secs(3), 1024)
        .await
        .unwrap();
    assert_eq!(output.status.code(), Some(7));
}
