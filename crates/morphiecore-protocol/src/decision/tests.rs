use super::*;

fn predicate(name: Option<&str>, instructions: &str) -> Question {
    Question::new(
        name.map(str::to_owned),
        Some(Text::new(instructions, "test instructions", 128).unwrap()),
        QuestionKind::Predicate,
    )
    .unwrap()
}

#[test]
fn native_ingress_keeps_order_separate_from_openai_names_and_system_one_keys() {
    let openai = Native::OpenAi
        .decode_request(
            br#"{"model":"public","input":"red","questions":[
          {"type":"predicate","name":"same","instructions":"first"},
          {"type":"predicate","name":"same","instructions":"second"},
          {"type":"predicate","instructions":"third"}]}"#,
        )
        .unwrap();
    assert_eq!(openai.model, "public");
    assert!(matches!(openai.task.evidence(), Evidence::HistoryUserText(t) if t.as_str() == "red"));
    assert!(
        openai.task.questions()
            == [
                predicate(Some("same"), "first"),
                predicate(Some("same"), "second"),
                predicate(None, "third"),
            ]
    );

    let typesafe = Native::SystemOne
        .decode_request(
            br#"{"model":"public","state":"green","questions":{
          "second":{"type":"noul","instructions":"two"},
          "first":{"type":"noul","instructions":"one"}}}"#,
        )
        .unwrap();
    assert!(matches!(typesafe.task.evidence(), Evidence::StateText(t) if t.as_str() == "green"));
    assert!(
        typesafe.task.questions()
            == [
                predicate(Some("second"), "two"),
                predicate(Some("first"), "one")
            ]
    );

    for invalid in [
        br#"{"model":"x","state":"s","questions":{"q":{"type":"noul","instructions":"t"},"q":{"type":"noul","instructions":"u"}}}"#.as_slice(),
        br#"{"model":"x","state":"s","questions":{"q":{"type":"noul","instructions":"t","criteria":{"true":"yes"}}}}"#,
    ] {
        assert!(Native::SystemOne.decode_request(invalid).is_err());
    }
}

fn openai_request() -> Prepared {
    let evidence = Evidence::HistoryUserText(Text::new("evidence", "text", 128).unwrap());
    Native::OpenAi
        .prepare(
            Request::new(
                evidence,
                vec![predicate(None, "a"), predicate(Some("r"), "b")],
            )
            .unwrap(),
            "upstream",
        )
        .unwrap()
}

#[test]
fn openai_projection_requires_full_ordered_association_and_preserves_refusal() {
    let prepared = openai_request();
    assert_eq!(
        prepared.body,
        json!({"model":"upstream","input":"evidence","questions":[
            {"type":"predicate","instructions":"a"},
            {"type":"predicate","name":"r","instructions":"b"}
        ]})
    );
    let raw = r#"{"model":"upstream","answers":[
          {"type":"predicate","name":null,"probability":0.1234567890123456789},
          {"type":"refusal","name":"r"}],"usage":{"input_tokens":29,"output_tokens":0,
          "total_tokens":29,"input_tokens_details":{"cached_tokens":12,"cache_write_tokens":3},
          "output_tokens_details":{"reasoning_tokens":0}}}"#;
    let delivered = prepared
        .expectation
        .complete(raw.as_bytes(), "public")
        .unwrap();
    assert_eq!(delivered["model"], "public");
    assert_eq!(
        delivered["answers"][0]["probability"].to_string(),
        "0.1234567890123456789"
    );
    assert_eq!(
        delivered["answers"][1],
        json!({"type":"refusal","name":"r"})
    );
    assert_eq!(
        delivered["usage"]["input_tokens_details"]["cache_write_tokens"],
        3
    );

    for broken in [
        raw.replace(r#""name":"r""#, r#""name":"other""#),
        raw.replace(",\n          {\"type\":\"refusal\",\"name\":\"r\"}", ""),
        raw.replace("0.1234567890123456789", "1.0000000000000000001"),
        format!("{raw}{{}}"),
    ] {
        assert_ne!(broken, raw);
        assert!(
            openai_request()
                .expectation
                .complete(broken.as_bytes(), "public")
                .is_err()
        );
    }
}

fn system_one_request() -> Prepared {
    let evidence = Evidence::StateText(Text::new("evidence", "text", 128).unwrap());
    Native::SystemOne
        .prepare(
            Request::new(
                evidence,
                vec![
                    predicate(Some("a"), "first"),
                    predicate(Some("z"), "second"),
                ],
            )
            .unwrap(),
            "jev-1.13.0",
        )
        .unwrap()
}

#[test]
fn system_one_projection_restores_order_from_a_shuffled_native_map() {
    let prepared = system_one_request();
    assert_eq!(
        prepared.body,
        json!({"model":"jev-1.13.0","state":"evidence","questions":{
          "a":{"type":"noul","instructions":"first"},"z":{"type":"noul","instructions":"second"}
        }})
    );
    let raw = br#"{"model":"jev-1.13.0","answers":{"z":{"type":"noul","noul":0.8},"a":{"type":"noul","noul":0.11}},
        "usage":{"input_tokens":15,"output_tokens":2}}"#;
    let delivered = prepared.expectation.complete(raw, "public").unwrap();
    let answers = delivered["answers"].as_object().unwrap();
    assert!(answers.keys().map(String::as_str).eq(["a", "z"]));
    assert_eq!(answers["a"]["noul"], json!(0.11));
    assert_eq!(answers["z"]["noul"], json!(0.8));
    assert_eq!(
        delivered["usage"],
        json!({"input_tokens":15,"output_tokens":2})
    );

    let wrong_key = String::from_utf8_lossy(raw).replace(r#""z""#, r#""b""#);
    assert!(
        system_one_request()
            .expectation
            .complete(wrong_key.as_bytes(), "public")
            .is_err()
    );
}

#[test]
fn openai_choice_binds_full_distribution_by_typed_value() {
    let request = br#"{
        "model":"luna","input":"evidence","questions":[{"type":"choice","name":"action",
          "instructions":"Classify","choices":[{"value":true},{"value":"true","description":"Flag"},{"value":false}]}]}"#;
    let prepared = || {
        let source = Native::OpenAi.decode_request(request).unwrap();
        Native::OpenAi.prepare(source.task, "upstream").unwrap()
    };
    let actual = prepared().body;
    assert_eq!(
        actual["questions"][0]["choices"],
        json!([{"value":true},{"value":"true","description":"Flag"},{"value":false}])
    );
    let wire = json!({
        "model":"upstream","answers":[{"type":"choice","name":"action","choice":"true",
            "confidence":0.37,"probabilities":[
                {"value":false,"probability":0.15},
                {"value":"true","probability":0.75},
                {"value":true,"probability":0.1}]}],
        "usage":{"input_tokens":5,"output_tokens":0,"total_tokens":5,
            "input_tokens_details":{"cached_tokens":0,"cache_write_tokens":0},
            "output_tokens_details":{"reasoning_tokens":0}}
    });
    let output = prepared()
        .expectation
        .complete(wire.to_string().as_bytes(), "luna")
        .unwrap();
    assert_eq!(output["answers"][0]["choice"], "true");
    assert_eq!(output["answers"][0]["confidence"], json!(0.37));
    assert_eq!(
        output["answers"][0]["probabilities"],
        json!([
            {"value":true,"probability":0.1},{"value":"true","probability":0.75},
            {"value":false,"probability":0.15}
        ])
    );
    for (pointer, invalid) in [
        ("/answers/0/choice", json!(true)), // cannot be a winner even under rounding
        ("/answers/0/probabilities/2/value", json!("true")), // duplicate typed value
        ("/answers/0/probabilities/2/probability", json!(0.2)), // unusable mass
    ] {
        let mut wrong = wire.clone();
        *wrong.pointer_mut(pointer).unwrap() = invalid;
        assert!(
            prepared()
                .expectation
                .complete(wrong.to_string().as_bytes(), "luna")
                .is_err()
        );
    }
}

#[test]
fn system_one_score_checks_rounded_weighted_mass_and_rubric_without_repair() {
    let labels: Vec<_> = (0..10).map(|i| format!("rating {i}")).collect();
    let input = json!({"model":"jev","state":"evidence","questions":{
        "severity":{"type":"score","criteria":labels}
    }});
    let prepared = || {
        let source = Native::SystemOne
            .decode_request(input.to_string().as_bytes())
            .unwrap();
        Native::SystemOne.prepare(source.task, "upstream").unwrap()
    };
    assert_eq!(
        prepared().body["questions"]["severity"],
        input["questions"]["severity"]
    );
    let probabilities = (0..10)
        .rev()
        .map(|i| (i.to_string(), json!(0.1)))
        .collect::<Map<_, _>>();
    let legend = labels
        .iter()
        .enumerate()
        .rev()
        .map(|(i, label)| (i.to_string(), json!(label)))
        .collect::<Map<_, _>>();
    let wire = json!({"model":"upstream","answers":{"severity":{
        "type":"score","score":4.55,"confidence":0.47,
        "legend":legend,"probabilities":probabilities
    }},"usage":{"input_tokens":8,"output_tokens":0}});

    let output = prepared()
        .expectation
        .complete(wire.to_string().as_bytes(), "jev")
        .unwrap();
    let answer = &output["answers"]["severity"];
    assert_eq!(answer["score"], json!(4.55)); // rounded report, not recomputed 4.5
    assert_eq!(answer["confidence"], json!(0.47));
    assert!(
        answer["probabilities"]
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .eq((0..10).map(|i| i.to_string()))
    );
    for (pointer, invalid) in [
        ("/answers/severity/score", json!(5.0)),
        ("/answers/severity/legend/9", json!("another rubric")),
    ] {
        let mut wrong = wire.clone();
        *wrong.pointer_mut(pointer).unwrap() = invalid;
        assert!(
            prepared()
                .expectation
                .complete(wrong.to_string().as_bytes(), "jev")
                .is_err()
        );
    }
}
