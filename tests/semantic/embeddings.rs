//! Independent text embedding oracles; no catalog, credentials or live model.
use morphiecore::{
    adapter::embeddings::Request,
    protocol::openai::embeddings,
    semantic::{task::embedding::*, value::Presence},
};
use serde_json::{Number, json};

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
fn typed_request_has_an_independent_wire_and_edits_cannot_restore_input() {
    let mut task = EmbeddingRequest::new(vec!["original".into(), "second".into()]).unwrap();
    task.dimensions = Presence::Value(3);
    let mut request = Request::new("public-vector", task);
    request.encoding = Presence::Value(embeddings::Encoding::Float);
    assert_eq!(
        embeddings::encode_request(&request, "upstream-vector").unwrap(),
        json!({"model":"upstream-vector","input":["original","second"],"dimensions":3,"encoding_format":"float"})
    );
    request.task = EmbeddingRequest::new(vec!["edited".into()]).unwrap();
    assert_eq!(
        embeddings::encode_request(&request, "upstream-vector").unwrap(),
        json!({"model":"upstream-vector","input":["edited"],"encoding_format":"float"})
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
fn malformed_results_and_associations_never_become_success() {
    let valid = json!({"object":"list","model":"m","data":[
        {"object":"embedding","index":0,"embedding":[1,2]},
        {"object":"embedding","index":1,"embedding":[3,4]}],
        "usage":{"prompt_tokens":2,"total_tokens":2}});
    let request = EmbeddingRequest::new(vec!["a".into(), "b".into()]).unwrap();
    for pointer in [
        "/data/1/index",
        "/data/1/embedding",
        "/usage/prompt_tokens",
        "/usage/total_tokens",
        "/object",
        "/model",
    ] {
        let mut bad = valid.clone();
        *bad.pointer_mut(pointer).unwrap() = json!(null);
        assert!(embeddings::decode_response(&serde_json::to_vec(&bad).unwrap()).is_err());
    }
    for (pointer, value) in [
        ("/data/1/index", json!(0)),
        ("/data/1/index", json!(2)),
        ("/data/1/embedding", json!([1])),
        ("/data/1/embedding", json!([])),
        ("/data/1/embedding", json!("AAAAAA==")),
        ("/usage/total_tokens", json!(1)),
        ("/usage/prompt_tokens", json!(true)),
        ("/data/1/object", json!("message")),
    ] {
        let mut bad = valid.clone();
        *bad.pointer_mut(pointer).unwrap() = value;
        let result = embeddings::decode_response(&serde_json::to_vec(&bad).unwrap());
        assert!(result.is_err() || result.unwrap().validate_for(&request).is_err());
    }
    let mut short = valid.clone();
    short["data"].as_array_mut().unwrap().pop();
    assert!(
        embeddings::decode_response(&serde_json::to_vec(&short).unwrap())
            .unwrap()
            .validate_for(&request)
            .is_err()
    );
    assert!(embeddings::decode_response(
        br#"{"object":"list","model":"m","data":[{"object":"embedding","index":0,"embedding":[1e400]}],"usage":{"prompt_tokens":1,"total_tokens":1}}"#
    ).is_err());
}

#[test]
fn aggregate_budgets_are_not_multiplied_by_batch_size() {
    use morphiecore::adapter::embeddings::Response;
    assert!(EmbeddingRequest::new(vec!["x".into(); MAX_INPUTS]).is_ok());
    assert!(EmbeddingRequest::new(vec!["x".into(); MAX_INPUTS + 1]).is_err());
    assert!(EmbeddingRequest::new(vec!["x".repeat(MAX_INPUT_BYTES)]).is_ok());
    assert!(EmbeddingRequest::new(vec!["x".repeat(MAX_INPUT_BYTES), "x".into()]).is_err());
    let values = vec![Number::from(0); MAX_DIMENSIONS];
    assert!(EmbeddingVector::new(0, values.clone()).is_ok());
    assert!(EmbeddingVector::new(0, vec![Number::from(0); MAX_DIMENSIONS + 1]).is_err());
    let vectors = (0..MAX_VECTOR_VALUES / MAX_DIMENSIONS + 1)
        .map(|i| EmbeddingVector::new(i, values.clone()).unwrap())
        .collect();
    assert!(EmbeddingResponse::new(vectors, EmbeddingUsage::new(1, 1).unwrap()).is_err());
    let vectors = (0..8)
        .map(|i| EmbeddingVector::new(i, vec![Number::from(0); MAX_VECTOR_VALUES / 8]).unwrap())
        .collect();
    let response = Response {
        model: "m".into(),
        task: EmbeddingResponse::new(vectors, EmbeddingUsage::new(1, 1).unwrap()).unwrap(),
        id: Presence::Absent,
    };
    let wire = embeddings::encode_response(&response).unwrap();
    assert!(embeddings::decode_response(&serde_json::to_vec(&wire).unwrap()).is_ok());
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

#[test]
fn absent_required_usage_stays_absent_and_cannot_be_encoded_as_zero() {
    let mut response = embeddings::decode_response(
        br#"{"object":"list","model":"m","data":[{"object":"embedding","index":0,"embedding":[1]}],"usage":{"prompt_tokens":0,"total_tokens":0}}"#
    ).unwrap();
    for unknown in [Presence::Absent, Presence::Null] {
        response.task.usage = unknown;
        assert!(embeddings::encode_response(&response).is_err());
    }
}
