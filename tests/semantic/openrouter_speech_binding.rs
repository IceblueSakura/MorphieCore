use morphiecore::{
    adapter::speech::{Profile, Request},
    execution::speech::prepare,
    provider::SecretMaterial,
    semantic::{task::speech_synthesis::SpeechRequest, value::AudioEncoding},
    topology::catalog,
};
use serde_json::json;

#[test]
fn openrouter_speech_binding_is_task_specific_and_uses_developer_publication() {
    let binding = &catalog::SPEECH_BINDINGS[0];
    let topology = catalog::default_topology().unwrap();
    let route = topology.speech_route("qwen-audio-3.0-tts-flash").unwrap();
    assert_eq!(binding.model, route.model.as_str());
    assert_eq!(
        route.endpoint.upstream_model,
        "qwen/qwen-audio-3.0-tts-flash"
    );
    assert_eq!(
        route.endpoint.target.origin.as_str(),
        "https://openrouter.ai"
    );
    assert_eq!(route.endpoint.target.path.as_str(), "/api/v1/audio/speech");
    assert_eq!(route.endpoint.profile, Profile::OpenRouterMp3);
    assert_eq!(route.endpoint.capabilities.formats, [AudioEncoding::Mp3]);
    assert_eq!(
        route.endpoint.capabilities.voices,
        ["loongjohn", "longanhuan_v3.6"]
    );
    assert!(!route.endpoint.capabilities.instructions);
    assert!(!route.endpoint.capabilities.speed);
    assert!(!route.endpoint.execution.streaming);
    assert!(!route.endpoint.execution.retry_before_commit);
    assert!(topology.model(binding.model).is_none());
    assert!(topology.image_route(binding.model).is_none());
    let metadata = topology.model_metadata(&route.canonical_model).unwrap();
    assert_eq!(metadata.developer(), "Alibaba");
    assert_eq!(metadata.released_at(), 1784592000);
    let request = Request::new(
        binding.model,
        SpeechRequest::new("hello", "loongjohn").unwrap(),
    );
    let prepared = prepare(
        route,
        &binding.provider(),
        &SecretMaterial::new("synthetic-router-key").unwrap(),
        &request,
    )
    .unwrap();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&prepared.body).unwrap(),
        json!({"model":"qwen/qwen-audio-3.0-tts-flash","input":"hello","voice":"loongjohn","response_format":"mp3"})
    );
}
