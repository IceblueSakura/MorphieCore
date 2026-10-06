//! Independent native wire oracles for the existing Router/SDK loopbacks.
use axum::{body::Body, http::HeaderMap, response::Response};
use serde_json::{Value, json};
pub const WAV:&[u8]=b"RIFF\x28\0\0\0WAVEfmt \x10\0\0\0\x01\0\x01\0\xc0\x5d\0\0\x80\xbb\0\0\x02\0\x10\0data\x04\0\0\0\0\0\x01\0";
pub fn speech(headers: &HeaderMap, request: &Value, fail: bool) -> Response {
    assert_eq!(
        headers["authorization"],
        "Bearer synthetic-tokenplan-credential-0001"
    );
    assert_eq!(headers["x-dashscope-sse"], "enable");
    assert_eq!(headers["accept"], "text/event-stream");
    assert!(!headers.contains_key("x-never-forward"));
    assert_eq!(
        *request,
        json!({"model":"qwen-audio-3.0-tts-plus",
        "input":{"text":"synthetic plan speech","voice":"longanlingxin","format":"mp3"}})
    );
    let mut wire = Vec::new();
    for (kind, data) in [
        ("sentence-begin", ""),
        ("sentence-synthesis", "bmF0aXZlLWF1ZGlv"),
        ("sentence-end", ""),
    ] {
        let value = json!({"request_id":"synthetic-native","output":{"finish_reason":"null","type":kind,
            "original_text":"synthetic plan speech","sentence":{"index":0,"words":[]},"audio":{"data":data}},
            "usage":{"characters":21}});
        wire.extend_from_slice(format!("data: {value}\n\n").as_bytes());
    }
    let final_value = json!({"request_id":"synthetic-native","output":{"finish_reason":if fail {"error"}else{"stop"},
        "audio":{"data":"","id":"synthetic-audio","url":"https://must-not-fetch.invalid/audio.mp3","expires_at":100}},
        "usage":{"characters":21}});
    wire.extend_from_slice(format!("data: {final_value}\n\n").as_bytes());
    Response::builder()
        .header("content-type", "text/event-stream")
        .body(Body::from_stream(futures_util::stream::iter(
            wire.chunks(7)
                .map(|c| Ok::<_, std::io::Error>(bytes::Bytes::copy_from_slice(c)))
                .collect::<Vec<_>>(),
        )))
        .unwrap()
}
pub fn transcription(headers: &HeaderMap, request: &Value, fail: bool) -> Response {
    assert_eq!(
        headers["authorization"],
        "Bearer synthetic-tokenplan-credential-0001"
    );
    assert_eq!(headers["x-dashscope-sse"], "disable");
    assert_eq!(headers["accept"], "application/json");
    assert!(!headers.contains_key("x-never-forward"));
    assert_eq!(
        *request,
        json!({"model":"qwen-audio-3.0-asr-flash","input":{"messages":[{"role":"user","content":[
        {"type":"input_audio","input_audio":{"data":"data:audio/wav;base64,UklGRigAAABXQVZFZm10IBAAAAABAAEAwF0AAIC7AAACABAAZGF0YQQAAAAAAAEA"}}
    ]}]},"parameters":{"format":"wav","language_hints":["en"]}})
    );
    let value = json!({"request_id":"synthetic-asr","output":{"text":"Hi.","sentence":{
        "sentence_id":1,"sentence_end":!fail,"channel_id":0,"text":"Hi.","begin_time":0,"end_time":1,
        "words":[{"text":"Hi","punctuation":".","begin_time":0,"end_time":1,"fixed":true}]}},
        "usage":{"duration":1}});
    Response::builder()
        .header("content-type", "application/json")
        .body(Body::from(value.to_string()))
        .unwrap()
}
