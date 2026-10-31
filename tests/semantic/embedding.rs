//! Independent request and report oracles for scoped inputs and native Google projection.
use morphiecore::{
    adapter::embeddings::UpstreamAdapter,
    protocol::{
        embedding::{
            Encoding, Request, TargetRequest,
            projection::{
                Action, Contract, DeliveryPolicy, DimensionSupport, EncodingSupport, ReportPolicy,
                Rule, UserPolicy, project_response,
            },
        },
        google::embeddings as google,
        openai::embeddings as openai,
    },
    semantic::task::embedding::*,
};
use serde_json::{Value, json};
fn contract() -> Contract {
    Contract {
        max_inputs: 128,
        default_dimensions: 3072,
        dimensions: DimensionSupport::Range {
            min: 128,
            max: 3072,
        },
        encodings: EncodingSupport::Float,
        tokenizer: None,
        max_input_tokens: Some(8192),
        joint_text: true,
        user: UserPolicy::Omit,
        reports: ReportPolicy::Compatible,
    }
}
fn policy() -> DeliveryPolicy {
    DeliveryPolicy {
        model: "public-vector".into(),
        encoding: Encoding::Base64,
        reports: ReportPolicy::Compatible,
        estimated_input_tokens: std::num::NonZeroU64::new(5).unwrap(),
    }
}
#[test]
fn usage_estimates_count_samples_not_parts_or_vector_dimensions() {
    for (parts, expected) in [
        (vec![vec!["ab", "cd"]], 1),
        (vec![vec!["ab"], vec!["cd"]], 2),
        (vec![vec!["12345"], vec!["中🙂"]], 4),
    ] {
        let task = EmbeddingRequest::new(
            parts
                .into_iter()
                .map(|parts| {
                    EmbeddingSample::new(
                        EmbeddingInput::TextParts(parts.into_iter().map(str::to_owned).collect()),
                        None,
                    )
                    .unwrap()
                })
                .collect(),
        )
        .unwrap();
        let snapshot = task.clone();
        let prepared = contract()
            .prepare(&Request::new("m", task.clone()))
            .unwrap();
        assert_eq!(prepared.delivery.estimated_input_tokens.get(), expected);
        let smaller = contract()
            .prepare(&Request::new("m", task.with_dimensions(Some(128)).unwrap()))
            .unwrap();
        assert_eq!(
            smaller.delivery.estimated_input_tokens,
            prepared.delivery.estimated_input_tokens
        );
        assert_eq!(task, snapshot);
    }
    let task = EmbeddingRequest::new(
        [3, 2]
            .into_iter()
            .map(|n| {
                EmbeddingSample::new(
                    EmbeddingInput::TokenIds {
                        tokenizer: TokenizerScope::Cl100kBase,
                        ids: vec![9; n],
                    },
                    None,
                )
                .unwrap()
            })
            .collect(),
    )
    .unwrap();
    let mut target = contract();
    target.tokenizer = Some(TokenizerScope::Cl100kBase);
    assert_eq!(
        target
            .prepare(&Request::new("m", task))
            .unwrap()
            .delivery
            .estimated_input_tokens
            .get(),
        5
    );
}

#[test]
fn complete_usage_preserves_reports_and_bounds_only_missing_estimates() {
    use AccountingBasis::{InputOnly, Unspecified};
    for (input, total, basis, expected, rule, actions) in [
        (
            None,
            None,
            Unspecified,
            (5, 5),
            Some(Rule::EstimateUsage),
            vec![Action::InputEstimated, Action::TotalEstimated],
        ),
        (
            Some(2),
            None,
            Unspecified,
            (2, 5),
            Some(Rule::EstimateUsage),
            vec![Action::TotalEstimated],
        ),
        (
            Some(9),
            None,
            Unspecified,
            (9, 9),
            Some(Rule::EstimateUsage),
            vec![Action::TotalEstimated],
        ),
        (
            None,
            Some(3),
            Unspecified,
            (3, 3),
            Some(Rule::EstimateUsage),
            vec![Action::InputEstimated],
        ),
        (
            None,
            Some(9),
            Unspecified,
            (5, 9),
            Some(Rule::EstimateUsage),
            vec![Action::InputEstimated],
        ),
        (
            Some(0),
            None,
            Unspecified,
            (0, 5),
            Some(Rule::EstimateUsage),
            vec![Action::TotalEstimated],
        ),
        (
            None,
            Some(0),
            Unspecified,
            (0, 0),
            Some(Rule::EstimateUsage),
            vec![Action::InputEstimated],
        ),
        (Some(0), Some(0), Unspecified, (0, 0), None, vec![]),
        (Some(2), Some(9), Unspecified, (2, 9), None, vec![]),
        (
            Some(7),
            None,
            InputOnly,
            (7, 7),
            Some(Rule::DeriveTotal),
            vec![Action::TotalDerived],
        ),
        (
            None,
            Some(7),
            InputOnly,
            (7, 7),
            Some(Rule::InputFromTotal),
            vec![Action::InputInterpreted],
        ),
        (
            Some(u64::MAX),
            None,
            Unspecified,
            (u64::MAX, u64::MAX),
            Some(Rule::EstimateUsage),
            vec![Action::TotalEstimated],
        ),
    ] {
        let source = morphiecore::protocol::embedding::Response {
            model: Some("reported".into()),
            id: None,
            task: EmbeddingResponse::new(
                vec![EmbeddingVector::from_binary32(0, vec![0]).unwrap()],
                Some(EmbeddingUsage::reported(input, total, basis, None).unwrap()),
            )
            .unwrap(),
        };
        let snapshot = source.clone();
        let result = project_response(&source, &policy()).unwrap();
        let usage = result.response.usage();
        assert_eq!((usage.input_tokens, usage.total_tokens), expected);
        assert_eq!(source, snapshot);
        assert_eq!(
            result
                .adaptations
                .iter()
                .map(|r| r.rule)
                .collect::<Vec<_>>(),
            rule.into_iter().collect::<Vec<_>>()
        );
        if let Some(receipt) = result.adaptations.first() {
            assert_eq!(receipt.actions, actions);
        }
        assert_eq!(
            openai::encode_response_as(&result.response, Encoding::Base64).unwrap()["usage"],
            json!({"prompt_tokens":expected.0,"total_tokens":expected.1})
        );
    }
}

#[test]
fn google_entries_preserve_sample_boundaries_and_per_sample_dimensions() {
    let task = EmbeddingRequest::new(vec![
        EmbeddingSample::new(
            EmbeddingInput::TextParts(vec!["first".into(), "same sample".into()]),
            Some(128),
        )
        .unwrap(),
        EmbeddingSample::new(EmbeddingInput::TextParts(vec!["second".into()]), Some(256)).unwrap(),
    ])
    .unwrap();
    let mut request = Request::new("public-vector", task);
    request.encoding = Some(Encoding::Base64);
    request.identity.user = Some("private-user".into());
    let snapshot = request.clone();
    let prepared = contract().prepare(&request).unwrap();
    assert_eq!(prepared.expectation.dimensions, vec![128, 256]);
    for (widths, valid) in [([256, 128], true), ([128, 256], false)] {
        let response = morphiecore::protocol::embedding::Response {
            model: None,
            id: None,
            task: EmbeddingResponse::new(
                vec![
                    EmbeddingVector::from_binary32(1, vec![0; widths[0]]).unwrap(),
                    EmbeddingVector::from_binary32(0, vec![0; widths[1]]).unwrap(),
                ],
                None,
            )
            .unwrap(),
        };
        assert_eq!(
            prepared.expectation.validate_response(&response).is_ok(),
            valid
        );
    }
    assert_eq!(
        prepared
            .adaptations
            .iter()
            .map(|r| r.rule)
            .collect::<Vec<_>>(),
        vec![Rule::OmitUser]
    );
    assert_eq!(
        google::encode_request(&prepared.target, "gemini-embedding-2").unwrap(),
        json!({"requests":[
            {"model":"models/gemini-embedding-2","content":{"parts":[{"text":"first"},{"text":"same sample"}]},"embedContentConfig":{"outputDimensionality":128,"autoTruncate":false}},
            {"model":"models/gemini-embedding-2","content":{"parts":[{"text":"second"}]},"embedContentConfig":{"outputDimensionality":256,"autoTruncate":false}}
        ]})
    );
    assert_eq!(request, snapshot);
    assert!(openai::encode_request(&prepared.target, "other").is_err());
    let default = Request::new(
        "public-vector",
        EmbeddingRequest::text(vec!["x".into(); 19]).unwrap(),
    );
    assert!(contract().prepare(&default).is_ok());
    let over = Request::new(
        "public-vector",
        EmbeddingRequest::text(vec!["x".into(); 20]).unwrap(),
    );
    assert!(contract().prepare(&over).is_err());
    let prepared = contract()
        .prepare(&Request::new(
            "public-vector",
            EmbeddingRequest::text(vec!["x".into()]).unwrap(),
        ))
        .unwrap();
    assert_eq!(prepared.expectation.dimensions, vec![3072]);
    assert!(google::encode_request(&prepared.target,"gemini-embedding-2").unwrap()["requests"][0]["embedContentConfig"].get("outputDimensionality").is_none());
}
#[test]
fn token_shapes_require_trusted_scope_and_keep_integer_identity() {
    for (input, expected) in [
        (json!([1, 2, 3]), json!([[1, 2, 3]])),
        (json!([[1, 2], [3]]), json!([[1, 2], [3]])),
    ] {
        let wire =
            serde_json::to_vec(&json!({"model":"public-vector","input":input,"dimensions":128}))
                .unwrap();
        assert!(
            openai::parse_request(&wire)
                .unwrap()
                .into_request(None)
                .is_err()
        );
        let request = openai::parse_request(&wire)
            .unwrap()
            .into_request(Some(TokenizerScope::Cl100kBase))
            .unwrap();
        for (sample, ids) in request
            .task
            .samples()
            .iter()
            .zip(expected.as_array().unwrap())
        {
            assert_eq!(sample.dimensions(), Some(128));
            assert_eq!(
                sample.input(),
                &EmbeddingInput::TokenIds {
                    tokenizer: TokenizerScope::Cl100kBase,
                    ids: ids
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|v| v.as_u64().unwrap() as u32)
                        .collect()
                }
            );
        }
        let mut target = contract();
        target.tokenizer = Some(TokenizerScope::Cl100kBase);
        target.joint_text = false;
        let prepared = target.prepare(&request).unwrap();
        assert_eq!(
            openai::encode_request(&prepared.target, "native").unwrap(),
            json!({"model":"native","input":expected,"dimensions":128,"encoding_format":"float"})
        );
        assert!(contract().prepare(&request).is_err());
        assert!(google::encode_request(&prepared.target, "gemini-embedding-2").is_err());
    }
    for input in [
        json!([-1]),
        json!([1.5]),
        json!([true]),
        json!([4294967296u64]),
        json!([[]]),
        json!([1, "x"]),
        json!([[1], "x"]),
    ] {
        let bytes = serde_json::to_vec(&json!({"model":"m","input":input})).unwrap();
        assert!(openai::parse_request(&bytes).is_err());
    }
    let mut c = contract();
    c.tokenizer = Some(TokenizerScope::Cl100kBase);
    for id in [100256, 100257, 100258, 100259, 100260, 100276, u32::MAX] {
        assert!(
            EmbeddingSample::new(
                EmbeddingInput::TokenIds {
                    tokenizer: TokenizerScope::Cl100kBase,
                    ids: vec![id]
                },
                None
            )
            .is_err()
        );
    }
    let valid = EmbeddingRequest::new(vec![
        EmbeddingSample::new(
            EmbeddingInput::TokenIds {
                tokenizer: TokenizerScope::Cl100kBase,
                ids: vec![100255, 0],
            },
            Some(128),
        )
        .unwrap(),
    ])
    .unwrap();
    let target = TargetRequest {
        task: valid,
        encoding: Encoding::Float,
        user: None,
    };
    assert_eq!(
        openai::encode_request(&target, "native").unwrap(),
        json!({"model":"native","input":[[100255,0]],"dimensions":128,"encoding_format":"float"})
    );
    let sample = EmbeddingSample::new(
        EmbeddingInput::TokenIds {
            tokenizer: TokenizerScope::Cl100kBase,
            ids: vec![1; 8193],
        },
        Some(128),
    )
    .unwrap();
    assert!(
        c.prepare(&Request::new(
            "m",
            EmbeddingRequest::new(vec![sample]).unwrap()
        ))
        .is_err()
    );
}
#[test]
fn google_usage_is_batch_scoped_and_projection_does_not_invent_source_facts() {
    // A real aggregate outranks the deliberately different request estimate.
    let wire=br#"{"embeddings":[{"values":[0.1,-0.125]},{"values":[0,1]}],"usageMetadata":{"promptTokenCount":7,"promptTokenDetails":[{"modality":"TEXT","tokenCount":6}]},"unknown":true}"#;
    let source = google::decode_response(wire).unwrap();
    let snapshot = source.clone();
    assert_eq!(source.model(), None);
    assert_eq!(
        source
            .vectors()
            .iter()
            .map(|v| v.index())
            .collect::<Vec<_>>(),
        vec![0, 1]
    );
    let usage = source.task.usage().unwrap();
    assert_eq!(usage.reported_input_tokens(), Some(7));
    assert_eq!(usage.reported_total_tokens(), None);
    assert_eq!(usage.details().unwrap()[0].tokens, 6);
    let delivered = project_response(&source, &policy()).unwrap();
    assert_eq!(
        openai::encode_response_as(&delivered.response, Encoding::Base64).unwrap(),
        json!({"object":"list","model":"public-vector","data":[{"object":"embedding","index":0,"embedding":"zczMPQAAAL4="},{"object":"embedding","index":1,"embedding":"AAAAAAAAgD8="}],"usage":{"prompt_tokens":7,"total_tokens":7}})
    );
    let derivations: Vec<_> = delivered
        .adaptations
        .iter()
        .filter(|r| r.rule == Rule::DeriveTotal)
        .collect();
    assert_eq!(derivations.len(), 1);
    assert_eq!(
        derivations[0].actions,
        vec![Action::TotalDerived, Action::DetailsOmitted]
    );
    assert_eq!(source, snapshot);
    let details=google::decode_response(br#"{"embeddings":[{"values":[1]}],"usageMetadata":{"promptTokenDetails":[{"modality":"TEXT","tokenCount":99}]}}"#).unwrap();
    let delivered = project_response(&details, &policy()).unwrap();
    assert_eq!(delivered.response.usage().input_tokens, 5);
    assert_eq!(delivered.response.usage().total_tokens, 5);
    assert!(
        delivered
            .adaptations
            .iter()
            .any(|r| r.rule == Rule::EstimateUsage
                && r.actions
                    == [
                        Action::InputEstimated,
                        Action::TotalEstimated,
                        Action::DetailsOmitted
                    ])
    );
}
#[test]
fn strict_openai_wire_rejects_unprojected_loss() {
    // Native serialization still requires actual reports, not a request fallback.
    use morphiecore::protocol::embedding::Response;
    let wire=br#"{"object":"list","model":"m","data":[{"object":"embedding","index":0,"embedding":[1]},{"object":"embedding","index":1,"embedding":[1,2]}],"usage":{"prompt_tokens":1,"total_tokens":1}}"#;
    assert!(openai::decode_response(wire).is_err());
    assert!(UpstreamAdapter::OpenRouter.decode_response(wire).is_err());
    let heterogeneous = Response {
        model: Some("m".into()),
        id: None,
        task: EmbeddingResponse::new(
            vec![
                EmbeddingVector::from_binary32(0, vec![0]).unwrap(),
                EmbeddingVector::from_binary32(1, vec![0, 0]).unwrap(),
            ],
            Some(EmbeddingUsage::new(1, 1).unwrap()),
        )
        .unwrap(),
    };
    assert!(openai::encode_response(&heterogeneous).is_err());
    let detailed = Response {
        model: Some("m".into()),
        id: None,
        task: EmbeddingResponse::new(
            vec![EmbeddingVector::from_binary32(0, vec![0]).unwrap()],
            Some(
                EmbeddingUsage::reported(
                    Some(1),
                    Some(1),
                    AccountingBasis::InputOnly,
                    Some(vec![ModalityTokenCount {
                        modality: InputModality::Text,
                        tokens: 1,
                    }]),
                )
                .unwrap(),
            ),
        )
        .unwrap(),
    };
    assert!(openai::encode_response(&detailed).is_err());
    let delivered = project_response(&detailed, &policy()).unwrap();
    assert!(openai::encode_response_as(&delivered.response, Encoding::Base64).is_ok());
    assert!(
        delivered
            .adaptations
            .iter()
            .any(|r| r.rule == Rule::OmitDetails)
    );
    let total_only=br#"{"object":"list","data":[{"object":"embedding","index":0,"embedding":[1]}],"usage":{"total_tokens":7}}"#;
    let source = UpstreamAdapter::OpenRouter
        .decode_response(total_only)
        .unwrap();
    assert_eq!(
        source.task.usage().unwrap().basis(),
        AccountingBasis::Unspecified
    );
    let delivered = project_response(&source, &policy()).unwrap();
    assert_eq!(delivered.response.usage().input_tokens, 5);
    assert_eq!(delivered.response.usage().total_tokens, 7);
    assert!(
        delivered
            .adaptations
            .iter()
            .any(|r| r.rule == Rule::EstimateUsage)
    );
}
#[test]
fn compatible_encodings_and_missing_reports_do_not_relax_malformed_values() {
    for value in [json!([0.1]), json!("zczMPQ==")] {
        let bytes = serde_json::to_vec(
            &json!({"object":"list","data":[{"object":"embedding","index":0,"embedding":value}]}),
        )
        .unwrap();
        let source = UpstreamAdapter::OpenRouter
            .decode_response_as(&bytes, Encoding::Base64)
            .unwrap();
        let delivered = project_response(&source, &policy()).unwrap();
        assert_eq!(
            openai::encode_response_as(&delivered.response, Encoding::Base64).unwrap()["data"][0]["embedding"],
            "zczMPQ=="
        );
        assert_eq!(delivered.response.usage().input_tokens, 5);
        assert_eq!(delivered.response.usage().total_tokens, 5);
        assert!(
            UpstreamAdapter::Standard
                .decode_response_as(&bytes, Encoding::Base64)
                .is_err()
        );
    }
    for usage in [
        Value::Null,
        json!({"promptTokenCount":null}),
        json!({"promptTokenCount":-1}),
        json!({"promptTokenCount":true}),
        json!({"promptTokenDetails":[{"modality":"TEXT","tokenCount":null}]}),
    ] {
        let wire = json!({"embeddings":[{"values":[0.1]}],"usageMetadata":usage});
        assert!(google::decode_response(&serde_json::to_vec(&wire).unwrap()).is_err());
    }
    for bad in [
        json!([]),
        json!([{"values":[]}]),
        json!([{"values":[1],"shape":[1,1]}]),
        json!([{"values":[1],"shape":null}]),
        json!([{"values":[true]}]),
    ] {
        assert!(
            google::decode_response(&serde_json::to_vec(&json!({"embeddings":bad})).unwrap())
                .is_err()
        );
    }
    let task = EmbeddingRequest::new(vec![
        EmbeddingSample::new(EmbeddingInput::TextParts(vec!["a".into()]), Some(128)).unwrap(),
        EmbeddingSample::new(EmbeddingInput::TextParts(vec!["b".into()]), Some(256)).unwrap(),
    ])
    .unwrap();
    assert!(
        openai::encode_request(
            &TargetRequest {
                task,
                encoding: Encoding::Float,
                user: None
            },
            "m"
        )
        .is_err()
    );
}
