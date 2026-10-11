use morphiecore::{adapter::speech::Profile, protocol::openai::speech};
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
