use morphiecore::{
    adapter::speech::Request,
    protocol::{aliyun_speech, openai::speech},
};
use serde_json::json;

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
