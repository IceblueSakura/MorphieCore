//! Terminal oracle for downstream SSE bodies, independent of the encoders.

/// Whether a published stream ended with its single protocol error frame
/// (Chat/Responses `error`, Messages `event: error`) and no success terminal.
pub fn error_terminated(wire: &[u8]) -> bool {
    let wire = std::str::from_utf8(wire).expect("UTF-8 SSE");
    let errors = wire
        .split("\n\n")
        .filter(|frame| {
            frame.lines().any(|line| line == "event: error")
                || frame
                    .lines()
                    .filter_map(|line| line.strip_prefix("data: "))
                    .filter_map(|data| serde_json::from_str::<serde_json::Value>(data).ok())
                    .any(|value| {
                        value["type"] == "error"
                            || value.get("error").is_some_and(serde_json::Value::is_object)
                    })
        })
        .count();
    assert!(errors <= 1, "duplicate error frames: {wire}");
    if errors == 1 {
        for success in ["[DONE]", "response.completed", "message_stop"] {
            assert!(
                !wire.contains(success),
                "error and success terminals: {wire}"
            );
        }
    }
    errors == 1
}
