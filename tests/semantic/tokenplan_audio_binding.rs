use morphiecore::{
    adapter::transcription::Request,
    execution::transcription::prepare,
    provider::{SecretMaterial, TrustedOrigin},
    semantic::task::speech_recognition::{AudioInput, InputFormat, RecognitionRequest},
    topology::catalog,
};
#[test]
fn tokenplan_asr_binding_has_native_operation_and_its_own_task_identity() {
    let topology = catalog::default_topology().unwrap();
    let route = topology
        .transcription_route("qwen-audio-3.0-asr-flash")
        .unwrap();
    assert!(topology.model(route.model.as_str()).is_none());
    assert!(topology.speech_route(route.model.as_str()).is_none());
    let provider = topology.provider("aliyun-tokenplan-cn").unwrap();
    let request = Request {
        model: route.model.as_str().into(),
        task: RecognitionRequest::new(
            AudioInput::new(bytes::Bytes::from_static(b"abc"), InputFormat::Mp3).unwrap(),
        ),
    };
    let secret = SecretMaterial::new("synthetic-only").unwrap();
    let prepared = prepare(route, provider, &secret, &request).unwrap();
    assert_eq!(
        prepared.origin,
        "https://token-plan.cn-beijing.maas.aliyuncs.com"
    );
    assert_eq!(
        prepared.path,
        "/api/v1/services/aigc/multimodal-generation/generation"
    );
    assert!(
        prepared
            .safe_headers
            .contains(&("x-dashscope-sse".into(), "disable".into()))
    );
    assert!(!route.endpoint.execution.streaming && !route.endpoint.execution.retry_before_commit);
    assert_eq!(
        route.endpoint.credential.as_str(),
        "aliyun-tokenplan-cn-api-key"
    );
    let metadata = topology.model_metadata(&route.canonical_model).unwrap();
    assert_eq!(metadata.developer(), "Alibaba");
    assert_eq!(metadata.released_at(), 1785369600);
    let mut bad = route.clone();
    bad.endpoint.target.origin = TrustedOrigin::parse("https://wrong.invalid").unwrap();
    assert!(prepare(&bad, provider, &secret, &request).is_err());
}
