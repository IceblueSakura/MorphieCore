//! Bounded readiness reads for an already-owned synthetic child.
use std::time::Duration;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncReadExt};

pub async fn read_line(
    reader: impl AsyncBufRead + Unpin,
    deadline: Duration,
    limit: usize,
) -> Result<String, &'static str> {
    let mut bytes = Vec::new();
    let mut limited = reader.take((limit as u64).saturating_add(1));
    tokio::time::timeout(deadline, limited.read_until(b'\n', &mut bytes))
        .await
        .map_err(|_| "child timeout")?
        .map_err(|_| "child read failed")?;
    if bytes.len() > limit {
        return Err("child output limit");
    }
    if bytes.last() != Some(&b'\n') {
        return Err("child incomplete line");
    }
    String::from_utf8(bytes).map_err(|_| "child invalid text")
}
