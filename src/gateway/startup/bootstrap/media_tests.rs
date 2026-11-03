//! Standalone tasks select one configured account without activating task recovery.
use super::{
    test_support::{Fixture, selected_models, service},
    *,
};
use serde_json::json;

#[tokio::test]
async fn configured_task_sends_once_to_its_named_account_despite_generation_recovery() {
    use axum::{Json, Router, http::HeaderMap, routing::post};
    use std::sync::{Arc, Mutex};
    use tower::ServiceExt;
    let calls = Arc::new(Mutex::new(Vec::new()));
    let observed = calls.clone();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let root = format!("http://{}/custom/v1", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new().route(
                "/custom/v1/embeddings",
                post(
                    move |headers: HeaderMap, Json(body): Json<serde_json::Value>| {
                        let calls = observed.clone();
                        async move {
                            calls.lock().unwrap().push((
                                headers["authorization"].to_str().unwrap().to_owned(),
                                body,
                            ));
                            axum::http::StatusCode::SERVICE_UNAVAILABLE
                        }
                    },
                ),
            ),
        )
        .await
        .unwrap();
    });
    let boot = Fixture::with_accounts(
        service(),
        json!({"models":{"text-embedding-3-small":{"account":"backup"}}}),
        &[("openrouter", "primary", "synthetic-primary"), ("openrouter", "backup", "synthetic-backup")],
        json!({"version":1,"providers":{"openrouter":{
            "recovery":{"recovery_timeout_ms":5000},
            "accounts":(["primary","backup"].map(|name| json!({
                "name":name,"enabled":true,"credential":{"kind":"api_key","alias":name},"base_url":root
            })))
        }}}),
    ).unwrap();
    let response = boot
        .gateway
        .router()
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/v1/embeddings")
                .header(
                    "authorization",
                    "Bearer synthetic-gateway-client-token-0001",
                )
                .header("content-type", "application/json")
                .body(axum::body::Body::from(
                    r#"{"model":"text-embedding-3-small","input":"synthetic"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(response.status().is_server_error());
    axum::body::to_bytes(response.into_body(), 65536)
        .await
        .unwrap();
    {
        let calls = calls.lock().unwrap();
        assert_eq!(
            calls.len(),
            1,
            "standalone tasks do not inherit Generation recovery"
        );
        assert_eq!(calls[0].0, "Bearer synthetic-backup");
        assert_eq!(calls[0].1["model"], "openai/text-embedding-3-small");
    }
    server.abort();
    let _ = server.await;
}

#[test]
fn embeddings_select_one_named_account_and_publish_only_the_selected_task() {
    for binding in catalog::EMBEDDING_BINDINGS {
        let provider = (binding.provider)();
        let boot = Fixture::new(
            service(),
            json!({"models":{binding.model:{"account":"primary"}}}),
            &[
                (provider.id.as_str(), "primary", "synthetic-embedding-key"),
                (provider.id.as_str(), "standby", "synthetic-other-key"),
            ],
        )
        .unwrap();
        assert!(boot.gateway.state.entries.is_empty());
        assert_eq!(boot.gateway.state.embeddings.len(), 1);
        let selected = &boot.gateway.state.embeddings[binding.model];
        assert_eq!(
            selected.secret.resolve().unwrap().0.expose(),
            "synthetic-embedding-key"
        );
        assert!(boot.gateway.state.models.models.contains_key(binding.model));
        let snapshot: serde_json::Value =
            serde_json::from_slice(&boot.gateway.state.capabilities.0).unwrap();
        assert_eq!(snapshot["data"].as_array().unwrap().len(), 1);
        assert_eq!(snapshot["data"][0]["model"], binding.model);
        assert_eq!(snapshot["data"][0]["protocol"], "embeddings");
        assert_eq!(
            snapshot["data"][0]["embedding"]["default_dimensions"],
            binding.default_dimensions
        );
        assert_eq!(
            snapshot["data"][0]["embedding"]["output_encodings"],
            json!(["float", "base64"])
        );
        assert!(!snapshot.to_string().contains(provider.origin.as_str()));
        assert!(!snapshot.to_string().contains("synthetic-embedding-key"));
        if let Some(text) = catalog::API_KEY_BINDINGS.iter().find(|b| {
            (b.provider)().id == provider.id
                && !b
                    .public_model()
                    .contract
                    .native(crate::semantic::task::generation::Feature::AudioInput)
        }) {
            let generation = Fixture::new(
                service(),
                selected_models(&[text.model], &[provider.id.as_str()]),
                &[(provider.id.as_str(), "primary", "synthetic-embedding-key")],
            )
            .unwrap();
            assert!(generation.gateway.state.embeddings.is_empty());
            assert!(!generation.gateway.state.entries.is_empty());
        }
        for account in [None, Some("missing")] {
            let config = account.map_or_else(|| json!({}), |name| json!({"account":name}));
            assert!(
                Fixture::new(
                    service(),
                    json!({"models":{binding.model:config}}),
                    &[(provider.id.as_str(), "primary", "synthetic-embedding-key")]
                )
                .is_err()
            );
        }
    }
}

#[test]
fn task_accounts_share_target_composition_but_not_automatic_account_selection() {
    let model = "text-embedding-3-small";
    let keys = [
        ("openrouter", "primary", "synthetic-first"),
        ("openrouter", "backup", "synthetic-second"),
    ];
    let declarations = json!({"version":1,"providers":{"openrouter":{"accounts":[
        {"name":"primary","enabled":true,"credential":{"kind":"api_key","alias":"primary"}},
        {"name":"backup","enabled":true,"credential":{"kind":"api_key","alias":"backup"},"base_url":"https://relay.invalid/custom/v1"}
    ]}}});
    let boot = Fixture::with_accounts(
        service(),
        json!({"models":{model:{"account":"backup"}}}),
        &keys,
        declarations.clone(),
    )
    .unwrap();
    let selected = &boot.gateway.state.embeddings[model];
    assert_eq!(selected.provider.origin.as_str(), "https://relay.invalid");
    assert_eq!(
        selected.route.endpoint.target.path.as_str(),
        "/custom/v1/embeddings"
    );
    assert_eq!(
        selected.secret.resolve().unwrap().0.expose(),
        "synthetic-second"
    );
    let mut disabled = declarations;
    disabled["providers"]["openrouter"]["accounts"][1]["enabled"] = json!(false);
    assert!(matches!(
        Fixture::with_accounts(
            service(),
            json!({"models":{model:{"account":"backup"}}}),
            &keys,
            disabled
        ),
        Err(StartupError::Credentials)
    ));
}

#[test]
fn tokenplan_audio_stays_explicit_and_each_task_uses_its_fixed_account() {
    let keys = [
        ("aliyun-tokenplan-cn", "primary", "synthetic-first"),
        ("aliyun-tokenplan-cn", "backup", "synthetic-second"),
    ];
    let boot = Fixture::new(
        service(),
        json!({"models":{
            "qwen-audio-3.0-tts-plus":{"account":"primary"},
            "qwen-audio-3.0-asr-flash":{"binding":"tokenplan-qwen-asr","account":"backup"}
        }}),
        &keys,
    )
    .unwrap();
    assert!(boot.gateway.state.entries.is_empty());
    assert_eq!(boot.gateway.state.speech.len(), 1);
    assert_eq!(boot.gateway.state.transcriptions.len(), 1);
    assert_eq!(
        boot.gateway.state.speech["qwen-audio-3.0-tts-plus"]
            .secret
            .resolve()
            .unwrap()
            .0
            .expose(),
        "synthetic-first"
    );
    assert_eq!(
        boot.gateway.state.transcriptions["qwen-audio-3.0-asr-flash"]
            .secret
            .resolve()
            .unwrap()
            .0
            .expose(),
        "synthetic-second"
    );
    let text = catalog::API_KEY_BINDINGS
        .iter()
        .find(|b| (b.provider)().id.as_str() == "aliyun-tokenplan-cn")
        .unwrap();
    let generation = Fixture::new(
        service(),
        selected_models(&[text.model], &["aliyun-tokenplan-cn"]),
        &keys,
    )
    .unwrap();
    assert!(
        generation.gateway.state.speech.is_empty()
            && generation.gateway.state.transcriptions.is_empty()
    );
    for invalid in [
        json!({"binding":"dashscope-qwen-asr","account":"primary"}),
        json!({"account":"primary"}),
    ] {
        assert!(
            Fixture::new(
                service(),
                json!({"models":{"qwen-audio-3.0-asr-flash":invalid}}),
                &keys
            )
            .is_err()
        );
    }
}

#[test]
fn openrouter_speech_and_images_do_not_activate_from_generation_accounts() {
    let keys = [
        ("openrouter", "primary", "synthetic-first"),
        ("openrouter", "backup", "synthetic-second"),
    ];
    let boot=Fixture::new(service(),json!({"models":{
        "qwen-audio-3.0-tts-flash":{"account":"primary"},"gpt-image-2.5-flare":{"account":"backup"}
    }}),&keys).unwrap();
    assert!(boot.gateway.state.entries.is_empty());
    assert_eq!(boot.gateway.state.speech.len(), 1);
    assert_eq!(boot.gateway.state.images.len(), 1);
    assert_eq!(
        boot.gateway.state.images["gpt-image-2.5-flare"]
            .secret
            .resolve()
            .unwrap()
            .0
            .expose(),
        "synthetic-second"
    );
    let view: serde_json::Value = serde_json::from_slice(&boot.gateway.state.models.list).unwrap();
    assert!(view["data"].as_array().unwrap().contains(&json!({
        "id":"qwen-audio-3.0-tts-flash","object":"model","created":1784592000,"owned_by":"Alibaba"
    })));
    let generation = Fixture::new(
        service(),
        selected_models(&["gpt-6-luna"], &["openrouter"]),
        &keys,
    )
    .unwrap();
    assert!(
        generation.gateway.state.images.is_empty() && generation.gateway.state.speech.is_empty()
    );
    assert!(!generation.gateway.state.entries.is_empty());
}
