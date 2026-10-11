//! Independent text embedding oracles; no catalog, credentials or live model.
use morphiecore::{
    protocol::openai::embeddings,
    semantic::{task::embedding::*, value::Presence},
};
use serde_json::json;

#[test]
fn text_batches_decode_without_generation_or_private_carriers() {
    let decoded = embeddings::decode_request(
        r#"{"model":"synthetic-vector","input":["first","第二条"],"dimensions":2,"encoding_format":"float","user":"synthetic"}"#.as_bytes(),
    ).unwrap();
    assert_eq!(decoded.task.inputs(), &["first", "第二条"]);
    assert_eq!(decoded.task.dimensions, Presence::Value(2));
    assert_eq!(decoded.identity.user, Presence::Value("synthetic".into()));
    let single =
        embeddings::decode_request(br#"{"model":"synthetic-vector","input":"first"}"#).unwrap();
    assert_eq!(single.task.inputs(), &["first"]);
    assert_eq!(single.task.dimensions, Presence::Absent);
    for extra in [
        json!({"input":""}),
        json!({"input":[]}),
        json!({"input":["x",""]}),
        json!({"input":[1,2]}),
        json!({"input":[["x"]]}),
        json!({"input":["x",1]}),
        json!({"input":null}),
        json!({"dimensions":0}),
        json!({"dimensions":true}),
        json!({"dimensions":null}),
        json!({"dimensions":1.5}),
        json!({"encoding_format":"base64"}),
        json!({"encoding_format":null}),
        json!({"stream":false}),
        json!({"provider":null}),
        json!({"_openbridge":{}}),
        json!({"user":null}),
    ] {
        let mut wire = json!({"model":"synthetic-vector","input":"x"});
        wire.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        assert!(
            embeddings::decode_request(&serde_json::to_vec(&wire).unwrap()).is_err(),
            "{wire}"
        );
    }
    assert!(
        embeddings::decode_request(br#"{"model":"synthetic-vector","input":"a","input":"b"}"#,)
            .is_err()
    );
}

#[test]
fn result_order_indices_precision_and_usage_are_independent_facts() {
    let response = embeddings::decode_response(
        br#"{"object":"list","model":"reported-vector","data":[
        {"object":"embedding","index":1,"embedding":[-0.125,0.12345678901234567890123456789]},
        {"object":"embedding","index":0,"embedding":[0,1e-20]}],
        "usage":{"prompt_tokens":7,"total_tokens":7}}"#,
    )
    .unwrap();
    let mut request = EmbeddingRequest::new(vec!["a".into(), "b".into()]).unwrap();
    request.dimensions = Presence::Value(2);
    response.validate_for(&request).unwrap();
    assert_eq!(response.vectors()[0].index(), 1);
    assert_eq!(
        response.vectors()[0].values()[1].to_string(),
        "0.12345678901234567890123456789"
    );
    assert_eq!(response.model(), "reported-vector");
    let encoded = embeddings::encode_response(&response).unwrap();
    assert_eq!(encoded["data"][0]["index"], 1);
    assert_eq!(
        encoded["usage"],
        json!({"prompt_tokens":7,"total_tokens":7})
    );
    request.dimensions = Presence::Value(3);
    assert!(response.validate_for(&request).is_err());
}

#[test]
fn dashscope_input_only_usage_is_a_named_basis_not_missing_fact_inference() {
    use morphiecore::{adapter::embeddings::Profile, lowering::embeddings::project_response};
    let wire = br#"{"object":"list","model":"synthetic-qwen","id":"synthetic-request-id",
        "data":[{"object":"embedding","index":0,"embedding":[0.5,-0.125]}],
        "usage":{"total_tokens":7}}"#;
    assert!(embeddings::decode_response(wire).is_err());
    assert!(Profile::OpenRouter.decode_response(wire).is_err());
    let response = Profile::DashScope.decode_response(wire).unwrap();
    assert!(!format!("{response:?}").contains("synthetic-request-id"));
    assert_eq!(response.id, Presence::Value("synthetic-request-id".into()));
    let usage = response.task.usage.value().unwrap();
    assert_eq!(usage.reported_input_tokens(), None);
    assert_eq!(usage.input_tokens(), 7);
    assert_eq!(usage.total_tokens(), 7);
    assert!(embeddings::encode_response(&response).is_err());
    let projected = project_response(&response).unwrap();
    assert!(projected.omitted_response_identifier);
    assert_eq!(response.id, Presence::Value("synthetic-request-id".into()));
    assert_eq!(
        embeddings::encode_response(&projected.response).unwrap(),
        json!({"object":"list","model":"synthetic-qwen","data":[
            {"object":"embedding","index":0,"embedding":[0.5,-0.125]}],
            "usage":{"prompt_tokens":7,"total_tokens":7}})
    );
    for usage in [
        json!({"total_tokens":true}),
        json!({"total_tokens":-1}),
        json!({"total_tokens":7,"prompt_tokens":null}),
        json!({}),
        json!({"total_tokens":7,"prompt_tokens":8}),
    ] {
        let mut invalid: serde_json::Value = serde_json::from_slice(wire).unwrap();
        invalid["usage"] = usage;
        assert!(
            Profile::DashScope
                .decode_response(&serde_json::to_vec(&invalid).unwrap())
                .is_err()
        );
    }
    let mut malformed = response;
    malformed.id = Presence::Value(String::new());
    assert!(project_response(&malformed).is_err());
}
