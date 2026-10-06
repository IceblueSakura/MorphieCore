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

#[tokio::test]
async fn readiness_limits_preserve_next_line_and_reject_incomplete_or_unbounded_output() {
    let deadline = Duration::from_secs(1);
    let mut input = b"ok\nnext\n".as_slice();
    assert_eq!(read_line(&mut input, deadline, 3).await.unwrap(), "ok\n");
    assert_eq!(read_line(&mut input, deadline, 5).await.unwrap(), "next\n");
    for (bytes, limit, expected) in [
        (b"long\n".as_slice(), 4, "child output limit"),
        (b"unbounded".as_slice(), 3, "child output limit"),
        (b"".as_slice(), 3, "child incomplete line"),
        (b"ok".as_slice(), 3, "child incomplete line"),
        (b"\xff\n".as_slice(), 3, "child invalid text"),
    ] {
        let mut input = bytes;
        assert_eq!(
            read_line(&mut input, deadline, limit).await.unwrap_err(),
            expected
        );
    }
    let (_writer, reader) = tokio::io::duplex(8);
    let mut input = tokio::io::BufReader::new(reader);
    assert_eq!(
        read_line(&mut input, Duration::from_millis(20), 8)
            .await
            .unwrap_err(),
        "child timeout"
    );
}
