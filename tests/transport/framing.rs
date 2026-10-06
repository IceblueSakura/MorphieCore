//! Verifies incremental SSE decoder fragmentation, UTF-8, CRLF, multiline data, and terminal boundaries.

use morphiecore::transport::sse::{SseDecodeError, SseDecoder};

#[test]
fn decoder_handles_fragmented_utf8_crlf_and_multiline_data() {
    let payload = "event: response.output_text.delta\r\nid: evt-1\r\ndata: {\"delta\":\"A😊\"}\r\ndata: second\r\n\r\n";
    // Exhaust all two-cut partitions, including cuts inside UTF-8 and CRLF.
    // Expected wire facts are independent of the unfragmented decoder.
    let bytes = payload.as_bytes();
    for first in 0..=bytes.len() {
        for second in first..=bytes.len() {
            let mut decoder = SseDecoder::new(1024);
            let mut events = Vec::new();
            for chunk in [&bytes[..first], &bytes[first..second], &bytes[second..]] {
                events.extend(decoder.push(chunk).unwrap());
            }
            events.extend(decoder.finish_strict().unwrap());
            assert_eq!(events.len(), 1, "cuts {first}, {second}");
            assert_eq!(events[0].event(), Some("response.output_text.delta"));
            assert_eq!(events[0].id(), Some("evt-1"));
            assert_eq!(events[0].data(), "{\"delta\":\"A😊\"}\nsecond");
        }
    }
}

#[test]
fn event_size_limit_is_applied_per_event_not_per_chunk() {
    let mut decoder = SseDecoder::new(12);

    let events = decoder.push(b"data: one\n\ndata: two\n\n").unwrap();

    assert_eq!(events.len(), 2);
    assert_eq!(events[0].data(), "one");
    assert_eq!(events[1].data(), "two");
}

#[test]
fn finish_preserves_a_complete_event_without_a_trailing_blank_line() {
    let mut decoder = SseDecoder::new(64);

    assert!(decoder.push(b"data: final").unwrap().is_empty());
    let events = decoder.finish().unwrap();

    assert_eq!(events.len(), 1);
    assert_eq!(events[0].data(), "final");
}

#[test]
fn decoder_parses_retry_and_ignores_invalid_retry_and_nul_ids() {
    let mut decoder = SseDecoder::new(256);

    // Combine valid and invalid control fields in one complete event.
    let events = decoder
        .push(b"id: stable\nretry: invalid\nretry: 1500\nid: rejected\0id\ndata: ready\n\n")
        .unwrap();

    // Preserve the last valid values while ignoring malformed replacements.
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].id(), Some("stable"));
    assert_eq!(events[0].retry_ms(), Some(1500));
    assert_eq!(events[0].data(), "ready");
}

#[test]
fn decoder_rejects_invalid_utf8_in_complete_and_terminal_lines() {
    // Reject invalid UTF-8 as soon as a newline completes the field.
    let mut complete = SseDecoder::new(64);
    assert_eq!(
        complete.push(b"data: \xff\n").unwrap_err(),
        SseDecodeError::InvalidUtf8
    );

    // Retain an incomplete field until EOF and reject it at finalization.
    let mut terminal = SseDecoder::new(64);
    assert!(terminal.push(b"data: \xff").unwrap().is_empty());
    assert_eq!(terminal.finish().unwrap_err(), SseDecodeError::InvalidUtf8);
}

#[test]
fn decoder_rejects_complete_events_and_incomplete_lines_over_the_limit() {
    // Enforce the limit while consuming a complete line.
    let mut complete = SseDecoder::new(9);
    assert_eq!(
        complete.push(b"data: one\n").unwrap_err(),
        SseDecodeError::EventTooLarge
    );

    // Enforce the same limit before an attacker supplies any newline boundary.
    let mut incomplete = SseDecoder::new(4);
    assert_eq!(
        incomplete.push(b"data:").unwrap_err(),
        SseDecodeError::EventTooLarge
    );
}

#[test]
fn decoder_applies_wpt_inspired_case_sensitive_field_and_data_joining_rules() {
    // Informed by web-platform-tests EventSource field parsing at
    // 269bca0dd35c303639f3c9cf1d8bcb3d911bdb60 (BSD-3-Clause). The fixture
    // is reduced to LF/CRLF here; bare CR is checked independently below.
    let payload = concat!(
        "Data: ignored\r\n",
        "datax: ignored\n",
        ": keepalive\n",
        "data:  leading-space\r\n",
        "data:\n",
        "data:tail\r\n",
        " data: ignored\n",
        "\n",
    );
    let mut decoder = SseDecoder::new(512);

    let events = decoder.push(payload.as_bytes()).unwrap();

    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event(), None);
    assert_eq!(events[0].data(), " leading-space\n\ntail");
}

#[test]
fn decoder_handles_bom_bare_cr_and_exact_crlf_event_budget() {
    let mut decoder = SseDecoder::new(31);
    let events = decoder.push("\u{feff}data: 🧪\r\r".as_bytes()).unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].data(), "🧪");
    decoder.finish_strict().unwrap();

    let frame = b"data: x\r\n\r\n";
    assert_eq!(frame.len(), 11);
    let mut exact = SseDecoder::new(frame.len());
    assert_eq!(exact.push(frame).unwrap()[0].data(), "x");
    exact.finish_strict().unwrap();
    let mut under = SseDecoder::new(frame.len() - 1);
    assert_eq!(
        under.push(frame).unwrap_err(),
        SseDecodeError::EventTooLarge
    );
    assert_eq!(
        under.push(b"data: y\n\n").unwrap_err(),
        SseDecodeError::Closed
    );
}

#[test]
fn strict_eof_requires_a_blank_line_even_after_a_completed_data_line() {
    let mut decoder = SseDecoder::new(64);
    decoder.push(b"data: last\n").unwrap();
    assert_eq!(
        decoder.finish_strict().unwrap_err(),
        SseDecodeError::UnexpectedEof
    );
    assert_eq!(decoder.finish().unwrap_err(), SseDecodeError::Closed);
}
