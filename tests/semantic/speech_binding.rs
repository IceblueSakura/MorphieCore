use super::speech_support::*;
use morphiecore::{
    adapter::speech::Request,
    execution::speech::prepare,
    lowering::speech::check_request,
    provider::{EndpointPath, SecretMaterial},
    semantic::{
        task::speech_synthesis::{SpeechRequest, Speed},
        value::AudioEncoding,
    },
    topology::{self, ModelId},
};
use serde_json::json;
const ORIGIN: &str = "https://synthetic.invalid";

#[test]
fn speech_target_admission_checks_final_controls_without_erasing_them() {
    let (_, route) = binding(ORIGIN);
    let mut caps = route.endpoint.capabilities.clone();
    let mut request = Request::new(
        "public-speech",
        SpeechRequest::new("hello", "alloy").unwrap(),
    );
    assert!(check_request(&request.task, &caps).is_ok());
    caps.instructions = false;
    request.task.instructions = Some(String::new());
    assert!(check_request(&request.task, &caps).is_err());
    request.task.instructions = None;
    caps.speed = false;
    request.task.speed = Some(Speed::new("1".parse().unwrap()).unwrap());
    assert!(check_request(&request.task, &caps).is_err());
    request.task.speed = None;
    request.task.format = Some(AudioEncoding::Flac);
    assert!(check_request(&request.task, &caps).is_err());
    request.task.format = None;
    request.task.set_voice("other").unwrap();
    assert!(check_request(&request.task, &caps).is_err());
    request.task.set_voice("alloy").unwrap();
    caps.formats = vec![AudioEncoding::Wav];
    assert!(
        check_request(&request.task, &caps).is_err(),
        "absent format means mp3, not a target-dependent default"
    );
    request.task.format = Some(AudioEncoding::Wav);
    assert!(check_request(&request.task, &caps).is_ok());

    let original = request.clone();
    let prepared = prepare(
        &route,
        &provider(ORIGIN),
        &SecretMaterial::new("synthetic-key").unwrap(),
        &request,
    )
    .unwrap();
    assert_eq!(prepared.path, "/audio/speech");
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&prepared.body).unwrap(),
        json!({"model":"private-speech","input":"hello","voice":"alloy","response_format":"wav"})
    );
    assert!(
        prepared
            .safe_headers
            .contains(&("accept".into(), "application/octet-stream".into()))
    );
    assert_eq!(original, request);
}

#[test]
fn speech_routes_require_trusted_operation_bounded_execution_and_unique_labels() {
    // Both compilation orders must protect the other task's identities.
    let (image_op, image_route) = super::image_support::binding(ORIGIN);
    let (speech_op, speech_route) = binding(ORIGIN);
    for case in 0..4 {
        let mut image = image_route.clone();
        match case {
            0 => image.model = speech_route.model.clone(),
            1 => image.canonical_model = speech_route.canonical_model.clone(),
            2 => image.id = speech_route.id.clone(),
            _ => image.endpoint.id = speech_route.endpoint.id.clone(),
        }
        image.endpoint.canonical_model = image.canonical_model.clone();
        let mut image_provider = provider(ORIGIN);
        image_provider.id = image_op.provider.clone();
        let combined = || {
            topology::compile(
                vec![provider(ORIGIN), image_provider.clone()],
                vec![],
                vec![],
                vec![],
                vec![],
            )
            .unwrap()
        };
        assert!(
            combined()
                .with_images(vec![image_op.clone()], vec![image.clone()])
                .unwrap()
                .with_speech(vec![speech_op.clone()], vec![speech_route.clone()])
                .is_err()
        );
        assert!(
            combined()
                .with_speech(vec![speech_op.clone()], vec![speech_route.clone()])
                .unwrap()
                .with_images(vec![image_op.clone()], vec![image])
                .is_err()
        );
    }
    let build =
        || topology::compile(vec![provider(ORIGIN)], vec![], vec![], vec![], vec![]).unwrap();
    let (operation, route) = binding(ORIGIN);
    assert!(build().with_speech(vec![], vec![route.clone()]).is_err());
    let mut mismatched = route.clone();
    mismatched.endpoint.profile = morphiecore::adapter::speech::Profile::OpenRouterMp3;
    assert!(
        build()
            .with_speech(vec![operation.clone()], vec![mismatched])
            .is_err()
    );
    for case in 0..9 {
        let mut changed = route.clone();
        match case {
            0 => changed.endpoint.target.path = EndpointPath::new("/other").unwrap(),
            1 => changed.endpoint.canonical_model = ModelId::new("other").unwrap(),
            2 => changed.endpoint.execution.streaming = true,
            3 => changed.endpoint.execution.retry_before_commit = true,
            4 => changed.endpoint.execution.response_body_limit = 0,
            5 => changed.endpoint.upstream_model = String::new(),
            6 => changed.endpoint.capabilities.voices.clear(),
            7 => changed.endpoint.capabilities.formats.clear(),
            _ => changed.endpoint.capabilities.voices.push("alloy".into()),
        }
        assert!(
            build()
                .with_speech(vec![operation.clone()], vec![changed])
                .is_err(),
            "{case}"
        );
    }
    assert!(
        build()
            .with_speech(vec![operation], vec![route.clone(), route])
            .is_err()
    );
    let compiled = topology(ORIGIN);
    assert!(compiled.model("public-speech").is_none());
    assert!(compiled.speech_route("public-speech").is_some());
}

#[test]
fn speech_activation_requires_metadata_single_source_credentials_and_explicit_entry() {
    use morphiecore::{
        gateway::{Credentials, Gateway, Limits, SpeechEntry},
        provider::CredentialBindingId,
    };
    use std::sync::Arc;
    let credentials = || {
        let mut c = Credentials::new();
        c.insert(
            CredentialBindingId::new("speech-key").unwrap(),
            Arc::new(SecretMaterial::new("synthetic-key").unwrap()),
        );
        c
    };
    let key = || SecretMaterial::new("synthetic-client-key-long-enough").unwrap();
    let entry = || SpeechEntry {
        model: "public-speech".into(),
    };
    assert!(
        Gateway::new(
            topology(ORIGIN),
            vec![],
            credentials(),
            key(),
            Limits::default(),
            None
        )
        .is_err()
    );
    assert!(
        Gateway::new_with_media(
            topology(ORIGIN),
            vec![],
            vec![],
            vec![entry()],
            vec![],
            Credentials::new(),
            key(),
            Limits::default(),
            None
        )
        .is_err()
    );
    assert!(
        Gateway::new_with_media(
            topology(ORIGIN),
            vec![],
            vec![],
            vec![entry(), entry()],
            vec![],
            credentials(),
            key(),
            Limits::default(),
            None
        )
        .is_err()
    );
    assert!(
        Gateway::new_with_media(
            topology(ORIGIN),
            vec![],
            vec![],
            vec![entry()],
            vec![],
            credentials(),
            key(),
            Limits::default(),
            None
        )
        .is_ok()
    );
    for speech_bytes in [0, morphiecore::semantic::value::MAX_AUDIO_BYTES + 1] {
        assert!(
            Gateway::new_with_media(
                topology(ORIGIN),
                vec![],
                vec![],
                vec![entry()],
                vec![],
                credentials(),
                key(),
                Limits {
                    speech_bytes,
                    ..Limits::default()
                },
                None
            )
            .is_err()
        );
    }
    let (operation, route) = binding(ORIGIN);
    let no_metadata = topology::compile(vec![provider(ORIGIN)], vec![], vec![], vec![], vec![])
        .unwrap()
        .with_speech(vec![operation], vec![route])
        .unwrap();
    assert!(
        Gateway::new_with_media(
            no_metadata,
            vec![],
            vec![],
            vec![entry()],
            vec![],
            credentials(),
            key(),
            Limits::default(),
            None
        )
        .is_err()
    );
}
