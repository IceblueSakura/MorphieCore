//! Frozen media tasks: one wire, binding and accounting contract each.
mod embeddings {
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
            json!({"encoding_format":"binary"}),
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
        let EmbeddingValues::ExactJson(values) = response.vectors()[0].values() else {
            panic!("exact JSON")
        };
        assert_eq!(values[1].to_string(), "0.12345678901234567890123456789");
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
        use morphiecore::{
            adapter::embeddings::{Encoding, Profile},
            lowering::embeddings::{Rule, project_response},
        };
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
        let projected = project_response(&response, Encoding::Float).unwrap();
        assert_eq!(
            projected
                .receipts
                .iter()
                .map(|r| r.rule)
                .collect::<Vec<_>>(),
            vec![Rule::OmitResponseId]
        );
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
        assert!(project_response(&malformed, Encoding::Float).is_err());
    }

    #[test]
    fn decimal_delivery_rounds_directly_to_binary32_and_preserves_source() {
        use morphiecore::{
            adapter::embeddings::{Encoding, Profile},
            lowering::embeddings::{Rule, project_response},
        };
        for (decimal, bits) in [
            ("0.1", 0x3dcccccd),
            ("1.000000059604644775390625", 0x3f800000),
            ("1.0000000596046447753906250000000001", 0x3f800001),
            ("1.000000178813934326171875", 0x3f800002),
            ("1.0000001788139343261718749999999999", 0x3f800001),
            ("1e-45", 1),
            ("8e-46", 1),
            ("3.4028235e38", 0x7f7fffff),
            ("-0", 0x80000000),
            ("-0.0", 0x80000000),
            ("-0e-999", 0x80000000),
            ("0e10", 0),
        ] {
            let wire = format!(
                r#"{{"object":"list","model":"reported","data":[{{"object":"embedding","index":0,"embedding":[{decimal}]}}],"usage":{{"prompt_tokens":1,"total_tokens":1}}}}"#
            );
            let source = Profile::Standard.decode_response(wire.as_bytes()).unwrap();
            let snapshot = source.clone();
            let float = project_response(&source, Encoding::Float).unwrap();
            assert!(float.receipts.is_empty());
            assert_eq!(float.response, source);
            let projected = project_response(&source, Encoding::Base64).unwrap();
            assert_eq!(source, snapshot);
            assert_eq!(
                projected.response.vectors()[0].values(),
                &EmbeddingValues::Binary32(vec![bits]),
                "{decimal}"
            );
            assert_eq!(
                projected
                    .receipts
                    .iter()
                    .map(|r| r.rule)
                    .collect::<Vec<_>>(),
                vec![Rule::RoundBinary32]
            );
        }
        for decimal in [
            "7e-46",
            "-7e-46",
            "1e-1000",
            "3.4028236e38",
            "-3.4028236e38",
        ] {
            let source = morphiecore::adapter::embeddings::Response {
                model: "reported".into(),
                id: Presence::Absent,
                task: EmbeddingResponse::new(
                    vec![EmbeddingVector::new(0, vec![decimal.parse().unwrap()]).unwrap()],
                    EmbeddingUsage::new(1, 1).unwrap(),
                )
                .unwrap(),
            };
            assert!(
                project_response(&source, Encoding::Base64).is_err(),
                "{decimal}"
            );
            assert!(
                embeddings::encode_response(&source).is_ok(),
                "float keeps {decimal}"
            );
        }
    }

    #[test]
    fn binary_carriers_have_independent_bytes_and_binary64_visible_json() {
        use morphiecore::{
            adapter::embeddings::{Encoding, Profile},
            lowering::embeddings::project_response,
        };
        for (encoded, bits, json_number) in [
            ("AAAAAA==", 0, "0.0"),
            ("AAAAgA==", 0x80000000, "-0.0"),
            ("AQAAAA==", 1, "1.401298464324817e-45"),
            ("zczMPQ==", 0x3dcccccd, "0.10000000149011612"),
        ] {
            let wire = json!({"object":"list","model":"reported","data":[{"object":"embedding","index":0,"embedding":encoded}],"usage":{"prompt_tokens":1,"total_tokens":1}});
            let source = Profile::Standard
                .decode_response_as(&serde_json::to_vec(&wire).unwrap(), Encoding::Base64)
                .unwrap();
            assert_eq!(
                source.vectors()[0].values(),
                &EmbeddingValues::Binary32(vec![bits])
            );
            for encoding in [Encoding::Float, Encoding::Base64] {
                let projected = project_response(&source, encoding).unwrap();
                assert!(projected.receipts.is_empty());
                let output = embeddings::encode_response_as(&projected.response, encoding).unwrap();
                assert_eq!(
                    output["data"][0]["embedding"],
                    if encoding == Encoding::Base64 {
                        json!(encoded)
                    } else {
                        serde_json::from_str::<serde_json::Value>(&format!("[{json_number}]"))
                            .unwrap()
                    }
                );
            }
        }
        for bad in [
            "",
            "AAAA",
            "AAAAAA",
            "AAAAAA===",
            "AAAAAB==",
            "AACAfw==",
            "AACA/w==",
            "AADAfw==",
        ] {
            let wire = json!({"object":"list","model":"reported","data":[{"object":"embedding","index":0,"embedding":bad}],"usage":{"prompt_tokens":1,"total_tokens":1}});
            assert!(
                Profile::Standard
                    .decode_response_as(&serde_json::to_vec(&wire).unwrap(), Encoding::Base64)
                    .is_err(),
                "{bad}"
            );
        }
        let decimal = EmbeddingVector::new(0, vec!["0.1".parse().unwrap()]).unwrap();
        let response = morphiecore::adapter::embeddings::Response {
            model: "reported".into(),
            id: Presence::Absent,
            task: EmbeddingResponse::new(vec![decimal], EmbeddingUsage::new(1, 1).unwrap())
                .unwrap(),
        };
        assert_eq!(
            embeddings::encode_response_as(
                &project_response(&response, Encoding::Base64)
                    .unwrap()
                    .response,
                Encoding::Base64
            )
            .unwrap()["data"][0]["embedding"],
            "zczMPQ=="
        );
    }

    #[test]
    fn target_expectations_resolve_defaults_and_do_not_weaken_wire_validation() {
        use morphiecore::{
            adapter::embeddings::{Encoding, Profile, Request},
            lowering::embeddings::{Contract, DimensionSupport, EncodingSupport},
        };
        let mut contract = Contract {
            max_inputs: 128,
            default_dimensions: 1536,
            dimensions: DimensionSupport::UpTo(1536),
            encodings: EncodingSupport::Base64,
            user: false,
        };
        let mut request = Request::new(
            "selected",
            EmbeddingRequest::new(vec!["x".into(); 39]).unwrap(),
        );
        let prepared = contract.prepare(&request).unwrap();
        assert_eq!(request.encoding, Presence::Absent);
        assert_eq!(prepared.target.task.dimensions, Presence::Absent);
        assert_eq!(prepared.expectation.upstream_encoding, Encoding::Base64);
        assert_eq!(prepared.expectation.client_encoding, Encoding::Float);
        request.task = EmbeddingRequest::new(vec!["x".into(); 40]).unwrap();
        assert!(contract.prepare(&request).is_err());
        request.task.dimensions = Presence::Value(1536);
        assert!(contract.prepare(&request).is_err());
        request.task.dimensions = Presence::Value(128);
        assert!(contract.prepare(&request).is_ok());
        contract.dimensions = DimensionSupport::Range {
            min: 128,
            max: 1536,
        };
        request.task.dimensions = Presence::Value(127);
        assert!(contract.prepare(&request).is_err());
        let wire = br#"{"object":"list","model":"reported","unknown":{},"data":[{"object":"embedding","index":0,"embedding":[0.1],"unknown":true}],"usage":{"prompt_tokens":1,"total_tokens":1,"unknown":[]} }"#;
        assert!(
            Profile::Standard
                .decode_response_as(wire, Encoding::Float)
                .is_ok()
        );
        assert!(
            Profile::Standard
                .decode_response_as(wire, Encoding::Base64)
                .is_err()
        );
        let bad = String::from_utf8(wire.to_vec())
            .unwrap()
            .replace("\"prompt_tokens\":1", "\"prompt_tokens\":null");
        assert!(Profile::Standard.decode_response(bad.as_bytes()).is_err());
        request.task = EmbeddingRequest::new(vec!["x".into()]).unwrap();
        let expected = contract.prepare(&request).unwrap().expectation;
        let response = Profile::Standard.decode_response(wire).unwrap();
        assert!(
            expected.validate_response(&response).is_err(),
            "omitted default width must still match"
        );
    }

    #[test]
    fn numeric_projection_preserves_unreported_usage_but_strict_delivery_rejects_it() {
        use morphiecore::{
            adapter::embeddings::{Encoding, Response},
            lowering::embeddings::project_response,
        };
        for vector in [
            EmbeddingVector::new(0, vec!["0.1".parse().unwrap()]).unwrap(),
            EmbeddingVector::from_binary32(0, vec![0x3dcccccd]).unwrap(),
        ] {
            let mut source = Response {
                model: "reported".into(),
                id: Presence::Absent,
                task: EmbeddingResponse::new(vec![vector], EmbeddingUsage::new(1, 1).unwrap())
                    .unwrap(),
            };
            source.task.usage = Presence::Absent;
            let snapshot = source.clone();
            for encoding in [Encoding::Float, Encoding::Base64] {
                let projected = project_response(&source, encoding).unwrap();
                assert_eq!(projected.response.task.usage, Presence::Absent);
                assert!(embeddings::encode_response_as(&projected.response, encoding).is_err());
                assert_eq!(source, snapshot);
            }
        }
    }
}
mod image_binding {
    use morphiecore::{
        execution::images::prepare,
        provider::{AuthScheme, ProviderDefinition, ProviderId, SecretMaterial, TrustedOrigin},
        topology::{self, CompiledTopology, ModelId, TopologyError},
    };
    const ORIGIN: &str = "https://synthetic.invalid";
    fn provider() -> ProviderDefinition {
        ProviderDefinition {
            id: ProviderId::new("fixture").unwrap(),
            origin: TrustedOrigin::parse(ORIGIN).unwrap(),
            endpoints: [].into(),
            credential_pool: None,
            auth: AuthScheme::Bearer,
        }
    }
    fn topology() -> CompiledTopology {
        topology::compile(vec![provider()], vec![], vec![], vec![], vec![]).unwrap()
    }
    #[test]
    fn image_routes_require_explicit_operation_identity_and_bounded_execution() {
        let (operation, route) = super::super::image_support::binding(ORIGIN);
        assert!(topology().with_images(vec![], vec![route.clone()]).is_err());
        for case in 0..7 {
            let mut route = route.clone();
            match case {
                0 => {
                    route.endpoint.target.origin =
                        TrustedOrigin::parse("https://other.invalid").unwrap()
                }
                1 => route.endpoint.canonical_model = ModelId::new("other").unwrap(),
                2 => route.endpoint.execution.streaming = true,
                3 => route.endpoint.execution.retry_before_commit = true,
                4 => route.endpoint.execution.response_body_limit = 0,
                5 => route.endpoint.upstream_model = "".into(),
                _ => {
                    route.endpoint.target.path =
                        morphiecore::provider::EndpointPath::new("/other").unwrap()
                }
            }
            assert!(
                topology()
                    .with_images(vec![operation.clone()], vec![route])
                    .is_err()
            );
        }
        assert_eq!(
            topology()
                .with_images(vec![operation.clone()], vec![route.clone(), route.clone()])
                .unwrap_err(),
            TopologyError::DuplicateRoute
        );
        let existing = topology::compile(
            vec![provider()],
            vec![],
            vec![],
            vec![],
            vec![topology::CanonicalModel {
                id: route.canonical_model.clone(),
                task: topology::TaskKind::Generation,
                contract: topology::GenerationSemanticContract::full(),
            }],
        )
        .unwrap();
        assert_eq!(
            existing
                .with_images(vec![operation.clone()], vec![route.clone()])
                .unwrap_err(),
            TopologyError::TaskMismatch
        );
        let compiled = topology()
            .with_images(vec![operation], vec![route])
            .unwrap();
        assert!(compiled.model("public-image").is_none());
        let route = compiled.image_route("public-image").unwrap();
        let request = morphiecore::adapter::images::Request::new(
            "public-image",
            morphiecore::semantic::task::image_generation::ImageGenerationRequest::new("square")
                .unwrap(),
        );
        let prepared = prepare(
            route,
            &provider(),
            &SecretMaterial::new("synthetic-key").unwrap(),
            &request,
        )
        .unwrap();
        assert_eq!(prepared.origin, ORIGIN);
        assert_eq!(prepared.path, "/images/generations");
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&prepared.body).unwrap(),
            serde_json::json!({"model":"private-image","prompt":"square"})
        );
        assert!(!format!("{prepared:?}").contains("synthetic-key"));
        let mut changed = request;
        changed.model = "another-label".into();
        assert!(
            prepare(
                route,
                &provider(),
                &SecretMaterial::new("synthetic-key").unwrap(),
                &changed
            )
            .is_err()
        );
    }
}
mod image_controls {
    //! Independent static generation controls, not provider capability inference.
    use morphiecore::{
        adapter::images::Profile,
        protocol::{openai::images, openrouter_images},
        semantic::{
            task::image_generation::*,
            value::{ImageFormat, Presence},
        },
    };
    use serde_json::json;

    #[test]
    fn standard_image_controls_decode_into_distinct_intents_and_identity() {
        let value = json!({"model":"image-model","prompt":"synthetic","size":"1536x1024","quality":"high","background":"transparent","output_format":"webp","output_compression":0,"moderation":"low","user":"synthetic-private-user"});
        let request = images::decode_request(&serde_json::to_vec(&value).unwrap()).unwrap();
        assert_eq!(
            request.task.size,
            Presence::Value(ImageSizeRequest::Exact(ImageSize {
                width: 1536,
                height: 1024
            }))
        );
        assert_eq!(
            request.task.quality,
            Presence::Value(ImageQualityRequest::Exact(ImageQuality::High))
        );
        assert_eq!(
            request.task.background,
            Presence::Value(ImageBackgroundRequest::Exact(ImageBackground::Transparent))
        );
        assert_eq!(request.task.compression, Presence::Value(0));
        assert_eq!(
            request.task.moderation,
            Presence::Value(ImageModeration::Low)
        );
        assert_eq!(
            request.identity.user,
            Presence::Value("synthetic-private-user".into())
        );
        assert!(!format!("{request:?}").contains("synthetic-private-user"));
    }

    #[test]
    fn openrouter_maps_only_declared_controls_with_fixed_provider_options() {
        let wire = json!({"model":"gpt-image-2.5-flare","prompt":"square","quality":"high","background":"transparent","moderation":"low","user":"synthetic-user"});
        let mut request = images::decode_request(&serde_json::to_vec(&wire).unwrap()).unwrap();
        assert_eq!(
            openrouter_images::encode_request(&request, "openai/gpt-image-2.5-flare").unwrap(),
            json!({"model":"openai/gpt-image-2.5-flare","prompt":"square","n":1,"stream":false,"quality":"high","background":"transparent","user":"synthetic-user","provider":{"only":["openai"],"allow_fallbacks":false,"options":{"openai":{"moderation":"low"}}}})
        );
        request.task.size = Presence::Value(ImageSizeRequest::Exact(ImageSize {
            width: 1024,
            height: 1024,
        }));
        assert!(
            Profile::OpenRouterFlare
                .encode_request(&request, "openai/gpt-image-2.5-flare")
                .is_err()
        );
        request.task.size = Presence::Absent;
        request.task.format = Presence::Value(ImageFormat::Webp);
        request.task.compression = Presence::Value(80);
        assert!(
            Profile::OpenRouterFlare
                .encode_request(&request, "openai/gpt-image-2.5-flare")
                .is_err()
        );
    }
}
mod image_generation {
    use morphiecore::{
        protocol::openai::images,
        semantic::{task::image_generation::*, value::Presence},
    };
    use serde_json::json;

    #[test]
    fn image_generation_request_rejects_unsupported_semantics_and_duplicate_keys() {
        for patch in [
            json!({"n":11}),
            json!({"stream":true}),
            json!({"output_format":"bmp"}),
            json!({"response_format":"b64_json"}),
            json!({"size":"not-a-size"}),
            json!({"provider":null}),
            json!({"_openbridge":{}}),
            json!({"prompt":null}),
        ] {
            let mut v = json!({"model":"public-image","prompt":"square"});
            v.as_object_mut()
                .unwrap()
                .extend(patch.as_object().unwrap().clone());
            assert!(
                images::decode_request(&serde_json::to_vec(&v).unwrap()).is_err(),
                "{patch}"
            );
        }
        assert!(images::decode_request(br#"{"model":"a","prompt":"x","prompt":"y"}"#).is_err());
        for fields in [
            json!({}),
            json!({"n":null,"stream":null,"output_format":null}),
        ] {
            let mut v = json!({"model":"public-image","prompt":"square"});
            v.as_object_mut()
                .unwrap()
                .extend(fields.as_object().unwrap().clone());
            let request = images::decode_request(&serde_json::to_vec(&v).unwrap()).unwrap();
            assert_eq!(images::encode_request(&request, "public-image").unwrap(), v);
        }
    }

    #[test]
    fn image_generation_reported_values_are_not_request_defaults() {
        let v = json!({"created":123,"data":[{"b64_json":"AQID"}],"output_format":"png", "size":"1536x1024",
        "background":"opaque","quality":"high","usage":{"input_tokens":3,"input_tokens_details":{"text_tokens":3,"image_tokens":0},"output_tokens":5,"total_tokens":8}});
        let mut response = images::decode_response(&serde_json::to_vec(&v).unwrap()).unwrap();
        assert_eq!(response.images[0].data.as_base64(), "AQID");
        assert_eq!(response.images[0].size.value().unwrap().width, 1536);
        response.images[0].data = ImageData::new("BAUG").unwrap();
        let mut expected = v;
        expected["data"][0]["b64_json"] = json!("BAUG");
        assert_eq!(images::encode_response(&response).unwrap(), expected);
        response.usage = Presence::Absent;
        response.images[0].size = Presence::Absent;
        let edited = images::encode_response(&response).unwrap();
        assert!(edited.get("usage").is_none());
        assert!(edited.get("size").is_none());
        let response = ImageGenerationResponse::new(
            123,
            vec![GeneratedImage::new(ImageData::new("AQID").unwrap())],
        )
        .unwrap();
        assert_eq!(
            images::encode_response(&response).unwrap(),
            json!({"created":123,"data":[{"b64_json":"AQID"}]})
        );
        assert!(!format!("{response:?}").contains("AQID"));
        for v in [
            json!({"created":123,"data":[{"b64_json":"AQID"}]}),
            json!({"created":123,"data":[{"b64_json":"AQID"}],"output_format":null,"size":null,"background":null,"quality":null,"usage":null}),
        ] {
            let r = images::decode_response(&serde_json::to_vec(&v).unwrap()).unwrap();
            assert_eq!(images::encode_response(&r).unwrap(), v);
        }
    }
}
mod openrouter_images {
    use morphiecore::{
        lowering::images::{AccountingPolicy, project_response},
        protocol::{openai::images, openrouter_images},
        semantic::value::Presence,
    };
    use serde_json::json;
    #[test]
    fn sparse_image_accounting_stays_in_ir_and_has_explicit_downstream_loss() {
        let bytes=br#"{"created":42,"data":[{"b64_json":"AQID","media_type":"image/png"}],"usage":{"prompt_tokens":7,"completion_tokens":9,"total_tokens":16,"cost":0.01234567890123456789,"is_byok":false,"prompt_tokens_details":{"cached_tokens":2},"completion_tokens_details":{"image_tokens":8,"reasoning_tokens":1}}}"#;
        let response = openrouter_images::decode_response(bytes).unwrap();
        let usage = response.usage.value().unwrap();
        assert_eq!(usage.input, Presence::Value(7));
        assert!(usage.input_details.value().unwrap().text.is_absent());
        assert_eq!(
            usage.input_details.value().unwrap().cached.value(),
            Some(&2)
        );
        assert_eq!(
            usage.output_details.value().unwrap().image.value(),
            Some(&8)
        );
        assert_eq!(
            usage.billing.cost.value().unwrap().amount().to_string(),
            "0.01234567890123456789"
        );
        assert!(project_response(&response, AccountingPolicy::Strict).is_err());
        let projected =
            project_response(&response, AccountingPolicy::OmitUnrepresentableAccounting).unwrap();
        assert!(projected.loss.usage_omitted);
        assert!(projected.loss.billing_omitted);
        assert_eq!(
            images::encode_response(&projected.response).unwrap(),
            json!({"created":42,"data":[{"b64_json":"AQID"}],"output_format":"png"})
        );
        assert_eq!(
            response.usage.value().unwrap().input,
            Presence::Value(7),
            "projection cannot mutate the source"
        );
    }
}
mod speech {
    use morphiecore::{
        adapter::speech::Request,
        protocol::openai::speech,
        semantic::{
            task::speech_synthesis::SpeechRequest,
            value::{AudioEncoding, MAX_AUDIO_BYTES},
        },
    };
    use serde_json::json;

    #[test]
    fn speech_presence_unknown_fields_and_unsupported_branches_fail_closed() {
        let base = json!({"model":"public","input":"hello","voice":"alloy"});
        for (key, value) in [
            ("model", json!(null)),
            ("input", json!("")),
            ("voice", json!({"id":"voice_fixture"})),
            ("instructions", json!(null)),
            ("speed", json!(null)),
            ("response_format", json!(null)),
            ("stream_format", json!(null)),
            ("stream_format", json!("sse")),
            ("stream", json!(false)),
            ("provider", json!(null)),
            ("response_format", json!("pcm16")),
            ("speed", json!("1")),
            ("speed", json!(4.1)),
        ] {
            let mut wire = base.clone();
            wire[key] = value;
            assert!(
                speech::decode_request(wire.to_string().as_bytes()).is_err(),
                "{key}"
            );
        }
        for key in ["model", "input", "voice"] {
            let mut wire = base.clone();
            wire.as_object_mut().unwrap().remove(key);
            assert!(speech::decode_request(wire.to_string().as_bytes()).is_err());
        }
        assert!(
            speech::decode_request(br#"{"model":"a","input":"x","voice":"a","voice":"b"}"#)
                .is_err()
        );
        assert!(SpeechRequest::new("你".repeat(4096), "alloy").is_ok());
        assert!(SpeechRequest::new("你".repeat(4097), "alloy").is_err());
        let mut task = SpeechRequest::new("old", "alloy").unwrap();
        assert!(task.set_input("").is_err());
        assert_eq!(task.input(), "old");
        task.instructions = Some("x".repeat(4097));
        assert!(task.validate().is_err());
    }

    #[test]
    fn binary_reports_are_independent_of_requested_format_and_have_no_chat_metadata() {
        let task = SpeechRequest::new("hello", "alloy").unwrap();
        let result = speech::decode_response(
            bytes::Bytes::from_static(b"synthetic audio"),
            "application/octet-stream",
            &task,
        )
        .unwrap();
        assert_eq!(result.data().as_ref(), b"synthetic audio");
        assert_eq!(result.encoding(), None);
        assert_eq!(
            speech::response_content_type(&result),
            "application/octet-stream"
        );
        let mp3 = speech::decode_response(
            bytes::Bytes::from_static(b"synthetic audio"),
            "audio/mpeg",
            &task,
        )
        .unwrap();
        assert_eq!(mp3.encoding(), Some(AudioEncoding::Mp3));
        assert_eq!(speech::response_content_type(&mp3), "audio/mpeg");
        for mime in [
            "audio/wav",
            "application/json",
            "text/event-stream",
            "audio/mpeg; charset=utf-8",
            "audio/mpeg, audio/mpeg",
        ] {
            assert!(
                speech::decode_response(bytes::Bytes::from_static(b"error"), mime, &task).is_err()
            );
        }
        assert!(speech::decode_response(bytes::Bytes::new(), "audio/mpeg", &task).is_err());
        assert!(
            speech::decode_response(vec![0; MAX_AUDIO_BYTES + 1].into(), "audio/mpeg", &task)
                .is_err()
        );
        assert!(!format!("{result:?}").contains("synthetic audio"));
        let mut task = task;
        for (label, encoding, mime) in [
            ("mp3", AudioEncoding::Mp3, "audio/mpeg"),
            ("opus", AudioEncoding::Opus, "audio/opus"),
            ("aac", AudioEncoding::Aac, "audio/aac"),
            ("flac", AudioEncoding::Flac, "audio/flac"),
            ("wav", AudioEncoding::Wav, "audio/wav"),
            ("pcm", AudioEncoding::PcmS16Le24KhzMono, "audio/pcm"),
        ] {
            task.format = Some(encoding);
            let request = Request::new("fixture", task.clone());
            assert_eq!(
                speech::encode_request(&request, "upstream").unwrap()["response_format"],
                label
            );
            assert_eq!(
                speech::decode_response(bytes::Bytes::from_static(b"\0\0"), mime, &task)
                    .unwrap()
                    .encoding(),
                Some(encoding)
            );
        }
        task.format = Some(AudioEncoding::Wav);
        let alias =
            speech::decode_response(bytes::Bytes::from_static(b"\0\0"), "audio/x-wav", &task)
                .unwrap();
        assert_eq!(alias.encoding(), Some(AudioEncoding::Wav));
        assert_eq!(speech::response_content_type(&alias), "audio/wav");
        assert_eq!(alias.data().as_ref(), b"\0\0");
    }
}
mod tokenplan_speech {
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
        use morphiecore::{
            execution::speech::prepare, provider::SecretMaterial, topology::catalog,
        };
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
}
mod tokenplan_asr {
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
        assert!(
            AudioInput::new(bytes::Bytes::from(vec![0; (1 << 20) + 1]), InputFormat::Mp3).is_err()
        );
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
                transcription::decode_request(
                    media,
                    upload(&fields, "Content-Type: audio/wav\r\n")
                )
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
}
