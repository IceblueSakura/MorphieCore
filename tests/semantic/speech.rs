use morphiecore::{
    adapter::speech::Request,
    protocol::openai::speech,
    semantic::{
        task::speech_synthesis::SpeechRequest,
        value::{AudioEncoding, MAX_AUDIO_BYTES},
    },
};
use serde_json::json;

#[test]
fn speech_presence_unknown_fields_and_unsupported_branches_fail_closed() {
    let base = json!({"model":"public","input":"hello","voice":"alloy"});
    for (key, value) in [
        ("model", json!(null)),
        ("input", json!("")),
        ("voice", json!({"id":"voice_fixture"})),
        ("instructions", json!(null)),
        ("speed", json!(null)),
        ("response_format", json!(null)),
        ("stream_format", json!(null)),
        ("stream_format", json!("sse")),
        ("stream", json!(false)),
        ("provider", json!(null)),
        ("response_format", json!("pcm16")),
        ("speed", json!("1")),
        ("speed", json!(4.1)),
    ] {
        let mut wire = base.clone();
        wire[key] = value;
        assert!(
            speech::decode_request(wire.to_string().as_bytes()).is_err(),
            "{key}"
        );
    }
    for key in ["model", "input", "voice"] {
        let mut wire = base.clone();
        wire.as_object_mut().unwrap().remove(key);
        assert!(speech::decode_request(wire.to_string().as_bytes()).is_err());
    }
    assert!(
        speech::decode_request(br#"{"model":"a","input":"x","voice":"a","voice":"b"}"#).is_err()
    );
    assert!(SpeechRequest::new("你".repeat(4096), "alloy").is_ok());
    assert!(SpeechRequest::new("你".repeat(4097), "alloy").is_err());
    let mut task = SpeechRequest::new("old", "alloy").unwrap();
    assert!(task.set_input("").is_err());
    assert_eq!(task.input(), "old");
    task.instructions = Some("x".repeat(4097));
    assert!(task.validate().is_err());
}

#[test]
fn binary_reports_are_independent_of_requested_format_and_have_no_chat_metadata() {
    let task = SpeechRequest::new("hello", "alloy").unwrap();
    let result = speech::decode_response(
        bytes::Bytes::from_static(b"synthetic audio"),
        "application/octet-stream",
        &task,
    )
    .unwrap();
    assert_eq!(result.data().as_ref(), b"synthetic audio");
    assert_eq!(result.encoding(), None);
    assert_eq!(
        speech::response_content_type(&result),
        "application/octet-stream"
    );
    let mp3 = speech::decode_response(
        bytes::Bytes::from_static(b"synthetic audio"),
        "audio/mpeg",
        &task,
    )
    .unwrap();
    assert_eq!(mp3.encoding(), Some(AudioEncoding::Mp3));
    assert_eq!(speech::response_content_type(&mp3), "audio/mpeg");
    for mime in [
        "audio/wav",
        "application/json",
        "text/event-stream",
        "audio/mpeg; charset=utf-8",
        "audio/mpeg, audio/mpeg",
    ] {
        assert!(speech::decode_response(bytes::Bytes::from_static(b"error"), mime, &task).is_err());
    }
    assert!(speech::decode_response(bytes::Bytes::new(), "audio/mpeg", &task).is_err());
    assert!(
        speech::decode_response(vec![0; MAX_AUDIO_BYTES + 1].into(), "audio/mpeg", &task).is_err()
    );
    assert!(!format!("{result:?}").contains("synthetic audio"));
    let mut task = task;
    for (label, encoding, mime) in [
        ("mp3", AudioEncoding::Mp3, "audio/mpeg"),
        ("opus", AudioEncoding::Opus, "audio/opus"),
        ("aac", AudioEncoding::Aac, "audio/aac"),
        ("flac", AudioEncoding::Flac, "audio/flac"),
        ("wav", AudioEncoding::Wav, "audio/wav"),
        ("pcm", AudioEncoding::PcmS16Le24KhzMono, "audio/pcm"),
    ] {
        task.format = Some(encoding);
        let request = Request::new("fixture", task.clone());
        assert_eq!(
            speech::encode_request(&request, "upstream").unwrap()["response_format"],
            label
        );
        assert_eq!(
            speech::decode_response(bytes::Bytes::from_static(b"\0\0"), mime, &task)
                .unwrap()
                .encoding(),
            Some(encoding)
        );
    }
    task.format = Some(AudioEncoding::Wav);
    let alias =
        speech::decode_response(bytes::Bytes::from_static(b"\0\0"), "audio/x-wav", &task).unwrap();
    assert_eq!(alias.encoding(), Some(AudioEncoding::Wav));
    assert_eq!(speech::response_content_type(&alias), "audio/wav");
    assert_eq!(alias.data().as_ref(), b"\0\0");
}
