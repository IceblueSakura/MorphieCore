use morphiecore::{
    adapter::speech::Request,
    protocol::{aliyun_speech, openai::speech},
    semantic::value::AudioEncoding,
};
use serde_json::{Value, json};

fn frame(kind: &str, data: &str) -> Value {
    json!({
        "request_id":"synthetic-request",
        "output":{"finish_reason":"null","type":kind,
            "sentence":{"index":0,"words":[]},"original_text":"Hi.",
            "audio":{"data":data,"id":"synthetic-audio","expires_at":10}},
        "usage":{"characters":3}
    })
}
fn terminal() -> Value {
    json!({"request_id":"synthetic-request",
        "output":{"finish_reason":"stop",
            "audio":{"data":"","id":"synthetic-audio","expires_at":10,
                "url":"https://audio.invalid/result.mp3"}},
        "usage":{"characters":3}})
}
fn consume(
    frames: &[Value],
) -> Result<
    morphiecore::semantic::task::speech_synthesis::SpeechResult,
    morphiecore::protocol::CodecError,
> {
    let mut decoder = aliyun_speech::Decoder::new(64, 8192, 16);
    for value in frames {
        decoder.push(value)?;
    }
    decoder.finish()
}

#[test]
fn tokenplan_speech_request_is_native_not_chat_and_never_drops_controls() {
    let request = speech::decode_request(
        br#"{"model":"synthetic","input":"Hi.","voice":"longanlingxin","stream_format":"audio"}"#,
    )
    .unwrap();
    assert_eq!(
        aliyun_speech::encode_request(&request, "upstream").unwrap(),
        json!({"model":"upstream","input":{"text":"Hi.","voice":"longanlingxin","format":"mp3"}})
    );
    for control in [
        json!({"instructions":""}),
        json!({"speed":1}),
        json!({"response_format":"wav"}),
    ] {
        let mut v = json!({"model":"synthetic","input":"Hi.","voice":"longanlingxin"});
        v.as_object_mut()
            .unwrap()
            .extend(control.as_object().unwrap().clone());
        let r = speech::decode_request(&serde_json::to_vec(&v).unwrap()).unwrap();
        assert!(aliyun_speech::encode_request(&r, "upstream").is_err());
    }
    let mut changed: Request = request;
    changed.task.set_input("Edited").unwrap();
    assert_eq!(
        aliyun_speech::encode_request(&changed, "upstream").unwrap()["input"]["text"],
        "Edited"
    );
}

#[test]
fn tokenplan_speech_keeps_report_but_binary_projection_is_explicit_and_unreported_format_stays_unknown()
 {
    let result = consume(&[
        frame("sentence-begin", ""),
        frame("sentence-synthesis", "YWJj"),
        frame("sentence-end", ""),
        terminal(),
    ])
    .unwrap();
    assert_eq!(result.audio().data().as_ref(), b"abc");
    assert_eq!(result.audio().encoding(), None);
    assert_eq!(result.report().billed_characters, Some(3));
    assert_eq!(result.report().sentences[0].text.as_deref(), Some("Hi."));
    assert_eq!(
        result.report().reference.url.as_deref(),
        Some("https://audio.invalid/result.mp3")
    );
    let projected = speech::project_result(&result).unwrap();
    assert_eq!(projected.audio.data().as_ref(), b"abc");
    assert!(
        projected.omitted.reference && projected.omitted.accounting && projected.omitted.alignment
    );
    assert_eq!(
        speech::response_content_type(&projected.audio),
        "application/octet-stream"
    );
    // Absence is not a license to infer an MP3 report from the requested format.
    assert_ne!(projected.audio.encoding(), Some(AudioEncoding::Mp3));
    let mut edited = result;
    edited.replace_audio(
        morphiecore::semantic::value::AudioArtifact::new(
            bytes::Bytes::from_static(b"edited"),
            None,
        )
        .unwrap(),
    );
    assert_eq!(
        speech::project_result(&edited).unwrap().omitted,
        morphiecore::lowering::speech::BinaryOmissions::default()
    );
}

#[test]
fn tokenplan_speech_requires_real_ordered_completion_and_bounded_data() {
    let begin = frame("sentence-begin", "");
    let data = frame("sentence-synthesis", "YWJj");
    let end = frame("sentence-end", "");
    for frames in [
        vec![terminal()],
        vec![begin.clone(), data.clone(), terminal()],
        vec![begin.clone(), data.clone(), end.clone()],
        vec![begin.clone(), end.clone(), terminal()],
        vec![
            begin.clone(),
            data.clone(),
            end.clone(),
            terminal(),
            terminal(),
        ],
        vec![
            begin.clone(),
            frame("sentence-synthesis", "!"),
            end.clone(),
            terminal(),
        ],
    ] {
        assert!(consume(&frames).is_err());
    }
    let mut small = aliyun_speech::Decoder::new(2, 8192, 16);
    small.push(&begin).unwrap();
    assert!(small.push(&data).is_err());
    assert!(small.push(&end).is_err());
    assert!(small.finish().is_err());
    let mut regress = terminal();
    regress["usage"]["characters"] = json!(2);
    assert!(consume(&[begin, data, end, regress]).is_err());
}
#[test]
fn tokenplan_speech_binding_prepares_only_native_sse_on_its_own_auth_domain() {
    use morphiecore::{execution::speech::prepare, provider::SecretMaterial, topology::catalog};
    let topology = catalog::default_topology().unwrap();
    let route = topology
        .speech_route("qwen-audio-3.0-tts-plus")
        .expect("explicit binding");
    let provider = topology.provider("aliyun-tokenplan-cn").unwrap();
    let request = speech::decode_request(
        br#"{"model":"qwen-audio-3.0-tts-plus","input":"Hi.","voice":"longanlingxin"}"#,
    )
    .unwrap();
    let secret = SecretMaterial::new("synthetic-only").unwrap();
    let wire = prepare(route, provider, &secret, &request).unwrap();
    assert_eq!(
        wire.origin,
        "https://token-plan.cn-beijing.maas.aliyuncs.com"
    );
    assert_eq!(wire.path, "/api/v1/services/audio/tts/SpeechSynthesizer");
    assert!(
        wire.safe_headers
            .contains(&("x-dashscope-sse".into(), "enable".into()))
    );
    assert!(
        wire.safe_headers
            .contains(&("accept".into(), "text/event-stream".into()))
    );
    assert_eq!(
        route.endpoint.credential.as_str(),
        "aliyun-tokenplan-cn-api-key"
    );
    assert!(route.endpoint.execution.streaming);
    assert!(!route.endpoint.execution.retry_before_commit);
    assert!(topology.model("qwen-audio-3.0-tts-plus").is_none());
    let mut changed = route.clone();
    changed.endpoint.execution.streaming = false;
    assert!(prepare(&changed, provider, &secret, &request).is_err());
    let mut foreign_voice = request;
    foreign_voice.task.set_voice("longanhuan_v3.6").unwrap();
    assert!(prepare(route, provider, &secret, &foreign_voice).is_err());
}
#[test]
fn speech_report_keeps_usage_finality_without_inventing_a_terminal_count() {
    let mut stop = terminal();
    stop.as_object_mut().unwrap().remove("usage");
    let result = consume(&[
        frame("sentence-begin", ""),
        frame("sentence-synthesis", "YWJj"),
        frame("sentence-end", ""),
        stop,
    ])
    .unwrap();
    assert_eq!(result.report().billed_characters, Some(3));
    assert!(!result.report().billed_characters_final);
    let result = consume(&[
        frame("sentence-begin", ""),
        frame("sentence-synthesis", "YWJj"),
        frame("sentence-end", ""),
        terminal(),
    ])
    .unwrap();
    assert!(result.report().billed_characters_final);
}
