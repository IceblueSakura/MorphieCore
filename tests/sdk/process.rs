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
        #[cfg(unix)]
        Ok("descendant") => {
            let mut socket = std::net::TcpStream::connect(
                std::env::var("MORPHIECORE_SYNTHETIC_SOCKET").unwrap(),
            )
            .unwrap();
            socket.write_all(b"ready").unwrap();
            loop {
                std::thread::park();
            }
        }
        #[cfg(unix)]
        Ok("tree") => {
            let mut child = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "process_tests::fixture", "--nocapture"])
                .env("MORPHIECORE_SYNTHETIC_CHILD", "descendant")
                .spawn()
                .unwrap();
            child.wait().unwrap();
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

#[cfg(unix)]
#[tokio::test]
async fn child_group_timeout_and_cancellation_release_descendant_resources() {
    use tokio::io::AsyncReadExt;
    for cancel in [false, true] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap().to_string();
        let task = tokio::spawn(async move {
            let mut command = command("tree");
            command.env("MORPHIECORE_SYNTHETIC_SOCKET", address);
            crate::child_process::run(
                &mut command,
                b"",
                Duration::from_secs(if cancel { 30 } else { 2 }),
                4096,
            )
            .await
        });
        let (mut socket, _) = tokio::time::timeout(Duration::from_secs(3), listener.accept())
            .await
            .unwrap()
            .unwrap();
        let mut ready = [0; 5];
        tokio::time::timeout(Duration::from_secs(3), socket.read_exact(&mut ready))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(&ready, b"ready");
        if cancel {
            task.abort();
            assert!(task.await.unwrap_err().is_cancelled());
        } else {
            assert_eq!(task.await.unwrap().unwrap_err(), "child timeout");
        }
        let mut byte = [0];
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(3), socket.read(&mut byte))
                .await
                .unwrap()
                .unwrap(),
            0
        );
    }
}
