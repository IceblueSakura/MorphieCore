//! Bounded owned child I/O. Failures kill and reap before returning; cancellation kills on drop.
use std::{
    process::{Output, Stdio},
    time::Duration,
};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWriteExt},
    process::Command,
};

async fn capture(reader: impl AsyncRead + Unpin, limit: usize) -> Result<Vec<u8>, &'static str> {
    let mut bytes = Vec::new();
    reader
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .await
        .map_err(|_| "child read failed")?;
    if bytes.len() > limit {
        return Err("child output limit");
    }
    Ok(bytes)
}

pub async fn run(
    command: &mut Command,
    input: &[u8],
    deadline: Duration,
    limit: usize,
) -> Result<Output, &'static str> {
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|_| "child spawn failed")?;
    let mut stdin = child.stdin.take().ok_or("missing child stdin")?;
    let stdout = child.stdout.take().ok_or("missing child stdout")?;
    let stderr = child.stderr.take().ok_or("missing child stderr")?;
    let result = tokio::time::timeout(deadline, async {
        let (_, stdout, stderr, status) = tokio::try_join!(
            async {
                stdin
                    .write_all(input)
                    .await
                    .map_err(|_| "child write failed")?;
                drop(stdin);
                Ok(())
            },
            capture(stdout, limit),
            capture(stderr, limit),
            async { child.wait().await.map_err(|_| "child wait failed") },
        )?;
        Ok(Output {
            status,
            stdout,
            stderr,
        })
    })
    .await
    .unwrap_or(Err("child timeout"));
    if result.is_err() {
        let _ = child.kill().await;
        let _ = child.wait().await;
    }
    result
}
