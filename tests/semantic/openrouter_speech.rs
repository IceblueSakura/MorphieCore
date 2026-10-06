use morphiecore::{
    adapter::speech::{Profile, Request},
    protocol::openai::speech,
    semantic::{
        task::speech_synthesis::{SpeechRequest, Speed},
        value::AudioEncoding,
    },
};
use serde_json::json;

#[test]
fn openrouter_speech_pins_mp3_default_and_preserves_final_typed_values() {
    for explicit in [false, true] {
        let raw = if explicit {
            br#"{"model":"public","input":"original","voice":"fixture","response_format":"mp3","stream_format":"audio"}"#.as_slice()
        } else {
            br#"{"model":"public","input":"original","voice":"fixture"}"#.as_slice()
        };
        let mut request = speech::decode_request(raw).unwrap();
        request.task.set_input("edited").unwrap();
        request.task.set_voice("second").unwrap();
        let original = request.clone();
        assert_eq!(
            Profile::OpenRouterMp3
                .encode_request(&request, "vendor/model")
                .unwrap(),
            json!({"model":"vendor/model","input":"edited","voice":"second","response_format":"mp3"})
        );
        assert_eq!(request, original);
        let standard = Profile::Standard
            .encode_request(&request, "standard-model")
            .unwrap();
        assert_eq!(standard.get("response_format").is_some(), explicit);
        assert_eq!(standard.get("stream_format").is_some(), explicit);
    }
}

#[test]
fn openrouter_speech_never_drops_controls_or_guesses_a_pcm_layout() {
    let mut request = Request::new("fixture", SpeechRequest::new("text", "fixture").unwrap());
    request.task.instructions = Some(String::new());
    assert!(
        Profile::OpenRouterMp3
            .encode_request(&request, "upstream")
            .is_err()
    );
    request.task.instructions = None;
    request.task.speed = Some(Speed::new("1".parse().unwrap()).unwrap());
    assert!(
        Profile::OpenRouterMp3
            .encode_request(&request, "upstream")
            .is_err()
    );
    request.task.speed = None;
    for format in [
        AudioEncoding::Wav,
        AudioEncoding::Flac,
        AudioEncoding::Aac,
        AudioEncoding::Opus,
        AudioEncoding::PcmS16Le24KhzMono,
    ] {
        request.task.format = Some(format);
        assert!(
            Profile::OpenRouterMp3
                .encode_request(&request, "upstream")
                .is_err()
        );
    }
    request.task.format = None;
    assert_eq!(
        Profile::OpenRouterMp3
            .reported_encoding("audio/mpeg", &request.task)
            .unwrap(),
        Some(AudioEncoding::Mp3)
    );
    assert_eq!(
        Profile::OpenRouterMp3
            .reported_encoding("application/octet-stream", &request.task)
            .unwrap(),
        None
    );
    assert!(
        Profile::OpenRouterMp3
            .reported_encoding("audio/pcm", &request.task)
            .is_err()
    );
    for (field, value) in [
        ("provider", json!({"options":{}})),
        ("input_references", json!([])),
        ("stream_format", json!("sse")),
    ] {
        let mut wire = json!({"model":"fixture","input":"text","voice":"fixture"});
        wire[field] = value;
        assert!(speech::decode_request(wire.to_string().as_bytes()).is_err());
    }
}
