use morphiecore::{
    adapter::transcription::Request,
    protocol::{aliyun_asr, openai::transcription},
    semantic::task::speech_recognition::{AudioInput, InputFormat, RecognitionRequest},
};
use serde_json::json;

fn response() -> serde_json::Value {
    json!({"request_id":"synthetic-asr","output":{
        "text":"Hi.","sentence":{"sentence_id":1,"sentence_end":true,
            "begin_time":0,"end_time":750,"channel_id":0,"text":"Hi.",
            "words":[{"text":"Hi","punctuation":".","begin_time":0,"end_time":750,"fixed":true}]}},
        "usage":{"duration":1}})
}
#[test]
fn native_asr_nested_reports_require_closed_and_identical_values() {
    let mut wire = response();
    let mut inner = wire["output"].clone();
    inner["request_id"] = wire["request_id"].clone();
    wire["output"]["output"] = inner;
    wire["output"]["request_id"] = wire["request_id"].clone();
    wire["output"]["usage"] = wire["usage"].clone();
    let result = aliyun_asr::decode_response(&serde_json::to_vec(&wire).unwrap()).unwrap();
    assert_eq!(result.text(), "Hi.");
    assert_eq!(result.report().processed_seconds, Some(1));
    for (path, value) in [
        ("/output/output/text", json!("different")),
        ("/output/output/request_id", json!("different")),
        ("/output/request_id", json!("different")),
        ("/output/usage/duration", json!(2)),
        ("/output/output/sentence/end_time", json!(1)),
        ("/output/output/sentence/words/0/fixed", json!(false)),
    ] {
        let mut bad = wire.clone();
        *bad.pointer_mut(path).unwrap() = value;
        assert!(aliyun_asr::decode_response(&serde_json::to_vec(&bad).unwrap()).is_err());
    }
    for path in ["/output", "/output/output"] {
        let mut bad = wire.clone();
        bad.pointer_mut(path).unwrap()["unknown"] = json!(1);
        assert!(aliyun_asr::decode_response(&serde_json::to_vec(&bad).unwrap()).is_err());
    }
    for key in ["text", "sentence", "request_id"] {
        let mut bad = wire.clone();
        bad["output"]["output"].as_object_mut().unwrap().remove(key);
        assert!(aliyun_asr::decode_response(&serde_json::to_vec(&bad).unwrap()).is_err());
    }
}
#[test]
fn native_asr_usage_seconds_do_not_claim_an_exact_timing_ceiling() {
    let mut value = response();
    value["output"]["sentence"]["end_time"] = json!(1750);
    let result = aliyun_asr::decode_response(&serde_json::to_vec(&value).unwrap()).unwrap();
    assert_eq!(result.report().processed_seconds, Some(1));
    assert_eq!(result.report().last_sentence.as_ref().unwrap().end_ms, 1750);
    value["output"]["sentence"]["end_time"] = json!(300_001);
    assert!(aliyun_asr::decode_response(&serde_json::to_vec(&value).unwrap()).is_err());
    value["output"]["sentence"]["end_time"] = json!(1750);
    value["usage"]["duration"] = json!(301);
    assert!(aliyun_asr::decode_response(&serde_json::to_vec(&value).unwrap()).is_err());
}
#[test]
fn tokenplan_asr_has_independent_file_semantics_and_exact_native_input() {
    let mut task = RecognitionRequest::new(
        AudioInput::new(bytes::Bytes::from_static(b"abc"), InputFormat::Wav).unwrap(),
    );
    task.language = Some("zh".into());
    let request = Request {
        model: "public-asr".into(),
        task,
    };
    assert_eq!(
        aliyun_asr::encode_request(&request, "upstream-asr").unwrap(),
        json!({"model":"upstream-asr","input":{"messages":[{"role":"user","content":[
            {"type":"input_audio","input_audio":{"data":"data:audio/wav;base64,YWJj"}}
        ]}]},"parameters":{"format":"wav","language_hints":["zh"]}})
    );
    assert!(AudioInput::new(bytes::Bytes::new(), InputFormat::Mp3).is_err());
    assert!(AudioInput::new(bytes::Bytes::from(vec![0; (1 << 20) + 1]), InputFormat::Mp3).is_err());
    let mut edited = request;
    for language in ["not-a-language", "xx"] {
        edited.task.language = Some(language.into());
        assert!(aliyun_asr::encode_request(&edited, "upstream-asr").is_err());
    }
}
#[test]
fn tokenplan_asr_preserves_last_sentence_reports_and_explicit_json_projection() {
    let mut result =
        aliyun_asr::decode_response(&serde_json::to_vec(&response()).unwrap()).unwrap();
    assert_eq!(result.text(), "Hi.");
    assert_eq!(result.report().processed_seconds, Some(1));
    assert_eq!(
        result.report().last_sentence.as_ref().unwrap().words[0].end_ms,
        750
    );
    let projected = transcription::project_result(&result).unwrap();
    assert_eq!(
        projected.body,
        json!({"text":"Hi.","usage":{"type":"duration","seconds":1}})
    );
    assert!(projected.omitted_alignment);
    result.set_text("Edited").unwrap();
    assert!(result.report().last_sentence.is_none());
    assert!(result.report().processed_seconds.is_none());
    assert_eq!(
        transcription::project_result(&result).unwrap().body,
        json!({"text":"Edited"})
    );
}
#[test]
fn tokenplan_asr_rejects_partial_conflicting_and_non_native_reports() {
    for (pointer, value) in [
        ("/output/sentence/sentence_end", json!(false)),
        ("/output/sentence/words/0/fixed", json!(false)),
        ("/output/sentence/words/0/end_time", json!(-1)),
        ("/output/sentence/words/0/text", json!("different")),
        ("/usage/duration", json!(-1)),
    ] {
        let mut v = response();
        *v.pointer_mut(pointer).unwrap() = value;
        assert!(aliyun_asr::decode_response(&serde_json::to_vec(&v).unwrap()).is_err());
    }
    assert!(aliyun_asr::decode_response(br#"{"output":{"text":"x","text":"y"}}"#).is_err());
    let mut v = response();
    v["output"]["text"] = json!("Earlier sentence. Hi.");
    let result = aliyun_asr::decode_response(&serde_json::to_vec(&v).unwrap()).unwrap();
    assert_eq!(result.text(), "Earlier sentence. Hi.");
    // The last sentence never becomes a fabricated complete timing inventory.
    assert_eq!(result.report().last_sentence.as_ref().unwrap().text, "Hi.");
}
fn upload(fields: &[(&str, &str)], file_header: &str) -> bytes::Bytes {
    let mut body = Vec::new();
    for (name, value) in fields {
        body.extend_from_slice(
            format!(
                "--fixture\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n"
            )
            .as_bytes(),
        );
    }
    body.extend_from_slice(format!("--fixture\r\nContent-Disposition: form-data; name=\"file\"; filename=\"clip.wav\"\r\n{file_header}\r\nabc\r\n--fixture--\r\n").as_bytes());
    body.into()
}
#[tokio::test]
async fn standard_transcription_multipart_is_bounded_and_rejects_ambiguous_controls() {
    let media = "multipart/form-data; boundary=fixture";
    let request = transcription::decode_request(
        media,
        upload(
            &[
                ("model", "public-asr"),
                ("language", "zh"),
                ("response_format", "json"),
                ("stream", "false"),
            ],
            "Content-Type: audio/wav\r\n",
        ),
    )
    .await
    .unwrap();
    assert_eq!(request.model, "public-asr");
    assert_eq!(request.task.audio.data().as_ref(), b"abc");
    assert_eq!(request.task.audio.format(), InputFormat::Wav);
    for fields in [
        vec![("model", "one"), ("model", "two")],
        vec![("model", "one"), ("prompt", "")],
        vec![("model", "one"), ("stream", "true")],
        vec![("model", "one"), ("response_format", "verbose_json")],
        vec![("model", "one"), ("timestamp_granularities[]", "word")],
        vec![("model", "one"), ("temperature", "0")],
    ] {
        assert!(
            transcription::decode_request(media, upload(&fields, "Content-Type: audio/wav\r\n"))
                .await
                .is_err()
        );
    }
    for header in [
        "Content-Type: audio/mpeg\r\n",
        "Content-Type: audio/wav\r\nContent-Transfer-Encoding: base64\r\n",
    ] {
        assert!(
            transcription::decode_request(media, upload(&[("model", "one")], header))
                .await
                .is_err()
        );
    }
    let mut trailing = upload(&[("model", "one")], "Content-Type: audio/wav\r\n").to_vec();
    trailing.extend_from_slice(b"trailing garbage");
    assert!(
        transcription::decode_request(media, trailing.into())
            .await
            .is_err()
    );
    assert!(
        transcription::decode_request(media, bytes::Bytes::from(vec![0; 2 << 20]))
            .await
            .is_err()
    );
}
