//! Pinned public inputs. Framing adaptations never rewrite payload fields.
use morphiecore::{
    adapter::UpstreamAdapter,
    execution::{Attempt, AttemptError},
    protocol::stream::SseLimits,
};
use serde_json::Value;
use std::path::Path;

pub fn asset(name: &str) -> Vec<u8> {
    std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("testdata/upstream")
            .join(name),
    )
    .unwrap_or_else(|error| panic!("upstream asset {name}: {error}"))
}

pub fn messages_with_delimiter(name: &str) -> Vec<u8> {
    let mut bytes = asset(name);
    // These SDK fixtures end at EOF without an SSE dispatch delimiter.
    // Retain their raw bytes separately and add only the transport delimiter.
    bytes.extend_from_slice(b"\n\n");
    bytes
}

pub fn jsonl_sse(name: &str, event_key: Option<&str>) -> Vec<u8> {
    let input = String::from_utf8(asset(name)).unwrap();
    let mut output = String::new();
    for line in input.lines() {
        let value: Value = serde_json::from_str(line).expect("nonempty JSON payload line");
        if let Some(key) = event_key {
            let event = value[key].as_str().expect("event discriminator");
            assert!(!event.contains(['\r', '\n']));
            output.push_str(&format!("event: {event}\n"));
        }
        output.push_str(&format!("data: {line}\n\n"));
    }
    output.into_bytes()
}

pub fn decode(
    adapter: UpstreamAdapter,
    bytes: &[u8],
    chunk: usize,
) -> Result<Attempt, AttemptError> {
    let mut attempt = Attempt::new(adapter, 2 << 20, SseLimits::default());
    attempt.begin(200, "text/event-stream")?;
    for part in bytes.chunks(chunk) {
        let mut rest = part;
        while !rest.is_empty() {
            let (used, _) = attempt.push(rest)?;
            assert!(used > 0 && used <= rest.len());
            rest = &rest[used..];
        }
    }
    // Payload terminals cannot publish a successful result before transport EOF.
    assert!(attempt.response().is_err());
    attempt.finish()?;
    Ok(attempt)
}
