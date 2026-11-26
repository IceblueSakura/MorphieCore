use super::*;

#[test]
fn system_one_plain_text_noul_prepares_gemini_without_reframing_state() {
    use crate::semantic::task::decision::Disposition;

    let ingress = Native::SystemOne
        .decode_request(
            br#"{
        "model":"candidate","state":"The box is dry, but the screen is cracked.",
        "questions":{
            "z":{"type":"noul","instructions":"Is the screen cracked?"},
            "a":{"type":"noul","instructions":"Is the box wet?"}
        }
    }"#,
        )
        .unwrap();
    let prepared = gemini::prepare(ingress.task, "gemini-decision-1").unwrap();
    assert_eq!(
        prepared.body,
        json!({
            "model":"gemini-decision-1","input":"The box is dry, but the screen is cracked.",
            "questions":{
                "0":{"type":"noul","instructions":"Is the screen cracked?"},
                "1":{"type":"noul","instructions":"Is the box wet?"}
            }
        })
    );
    let reply = br#"{"answers":{
        "1":{"type":"noul","noul":0.14,"confidence":0.86},
        "0":{"type":"noul","noul":0.93,"confidence":0.93}
    }}"#;
    let source = prepared.expectation.complete(reply).unwrap();
    let Disposition::Evaluated(answers) = source.disposition else {
        panic!("expected typed evaluation");
    };
    let [
        Outcome::Answered(Answer::Predicate {
            probability: first,
            confidence: Some(Confidence::WinningMass(first_mass)),
        }),
        Outcome::Answered(Answer::Predicate {
            probability: second,
            confidence: Some(Confidence::WinningMass(second_mass)),
        }),
    ] = answers.outcomes()
    else {
        panic!("expected Noul reports including their source confidence");
    };
    assert_eq!(
        (first.as_number().as_str(), second.as_number().as_str()),
        ("0.93", "0.14")
    );
    assert_eq!(
        (
            first_mass.as_number().as_str(),
            second_mass.as_number().as_str()
        ),
        ("0.93", "0.86")
    );
    assert!(source.usage.is_none(), "absent usage is not backfilled");
}

fn predicate(name: Option<&str>, instructions: &str) -> Question {
    Question::new(
        name.map(str::to_owned),
        Some(Text::new(instructions, "test instructions", 128).unwrap()),
        QuestionKind::Predicate { criteria: None },
    )
    .unwrap()
}

#[test]
fn gemini_to_system_one_delivers_only_named_predicates_and_reported_usage() {
    use system_one_noul::{Delivery, NoulTarget};

    let complete = |wire: &[u8]| {
        let request = Request::new(
            Evidence::StateText(Text::new("Synthetic parcel.", "test evidence", 128).unwrap()),
            vec![
                predicate(Some("z"), "First?"),
                predicate(Some("a"), "Second?"),
            ],
        )
        .unwrap();
        system_one_noul::prepare(request, "gemini-decision-1", NoulTarget::Gemini)
            .unwrap()
            .expectation
            .complete(wire, "gemini-decision-1")
    };
    let raw = br#"{
        "answers":{
            "1":{"type":"noul","noul":0.1234567890123456789,"confidence":0.8765432109876543211},
            "0":{"type":"noul","noul":0.95,"confidence":0.95}
        },
        "usage":{"total_input_tokens":31,"total_output_tokens":0,"total_tokens":31,
                 "input_tokens_by_modality":[{"modality":"text","tokens":31}]}
    }"#;
    let Delivery::Answered(output) = complete(raw).unwrap() else {
        panic!("expected two evaluated predicates");
    };
    assert_eq!(
        output,
        decode(br#"{"model":"gemini-decision-1",
            "answers":{"z":{"type":"noul","noul":0.95},"a":{"type":"noul","noul":0.1234567890123456789}},
            "usage":{"input_tokens":31,"output_tokens":0}}"#).unwrap()
    );

    // The base Gemini codec may carry absent usage, but System One delivery
    // cannot meet the public response contract without both counts.
    let mut missing = decode(raw).unwrap();
    missing["usage"]
        .as_object_mut()
        .unwrap()
        .remove("total_output_tokens");
    assert!(complete(missing.to_string().as_bytes()).is_err());
    assert!(matches!(
        complete(br#"{"prompt_feedback":{"block_reason":"safety"},"answers":{}}"#),
        Ok(Delivery::Blocked)
    ));
}

#[test]
fn native_preparation_does_not_implicitly_add_or_drop_user_framing() {
    let openai = br#"{"model":"test","input":"text","questions":[
        {"name":"a","type":"predicate","instructions":"Check."}
    ]}"#;
    let system_one = br#"{"model":"test","state":"text","questions":{
        "a":{"type":"noul","instructions":"Check."}
    }}"#;
    for (source, target, wire) in [
        (Native::OpenAi, Native::SystemOne, openai.as_slice()),
        (Native::SystemOne, Native::OpenAi, system_one.as_slice()),
    ] {
        let request = || source.decode_request(wire).unwrap().task;
        assert!(source.prepare(request(), "control").is_ok());
        assert!(target.prepare(request(), "opposite").is_err());
    }
}

#[test]
fn system_one_luna_uses_one_user_message_and_refuses_the_whole_batch() {
    use system_one_noul::{Delivery, NoulTarget};

    let request = br#"{"model":"gpt-6-luna-noul","state":"  Parcel arrived.\n",
        "questions":{
            "z_arrived":{"type":"noul","instructions":"Did it arrive?"},
            "a_wet":{"type":"noul","instructions":"Was it wet?"}
        }}"#;
    let prepare = || {
        let task = Native::SystemOne.decode_request(request).unwrap().task;
        system_one_noul::prepare(task, "gpt-6-luna", NoulTarget::OpenAi).unwrap()
    };
    let prepared = prepare();
    assert_eq!(
        prepared.body,
        json!({"model":"gpt-6-luna",
            "input":[{"type":"message","role":"user","content":"  Parcel arrived.\n"}],
            "questions":[
                {"type":"predicate","name":"z_arrived","instructions":"Did it arrive?"},
                {"type":"predicate","name":"a_wet","instructions":"Was it wet?"}
            ]
        })
    );
    let wire = br#"{"model":"gpt-6-luna","answers":[
        {"type":"predicate","name":"z_arrived","probability":0.95},
        {"type":"predicate","name":"a_wet","probability":0.01234567890123456789}
    ],"usage":{"input_tokens":32,"output_tokens":3,"total_tokens":35,
               "input_tokens_details":{"cached_tokens":8,"cache_write_tokens":4},
               "output_tokens_details":{"reasoning_tokens":1}}}"#;
    let Delivery::Answered(output) = prepared
        .expectation
        .complete(wire, "gpt-6-luna-noul")
        .unwrap()
    else {
        panic!("expected complete named Noul answers");
    };
    assert_eq!(
        output,
        decode(
            br#"{"model":"gpt-6-luna-noul",
            "answers":{"z_arrived":{"type":"noul","noul":0.95},
                       "a_wet":{"type":"noul","noul":0.01234567890123456789}},
            "usage":{"input_tokens":32,"output_tokens":3}}"#
        )
        .unwrap()
    );
    let mut refused = decode(wire).unwrap();
    refused["answers"][1] = json!({"type":"refusal","name":"a_wet"});
    assert!(matches!(
        prepare()
            .expectation
            .complete(refused.to_string().as_bytes(), "gpt-6-luna-noul"),
        Ok(Delivery::Refused)
    ));
    refused["answers"][1]["name"] = json!("different");
    assert!(
        prepare()
            .expectation
            .complete(refused.to_string().as_bytes(), "gpt-6-luna-noul")
            .is_err()
    );
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

#[test]
fn structured_evidence_retains_message_boundaries_and_business_keys() {
    let request = br#"{"model":"luna","input":[
        {"role":"user","content":"prior text"},
        {"role":"user","content":[{"type":"input_text","text":"record"},
            {"type":"input_json","data":{"role":"system","image_url":"field value","serial":9007199254740993}},
            {"type":"input_text","text":"after"}]}],
        "questions":[{"type":"predicate","instructions":"Is there a record?"}]}"#;
    let parsed = Native::OpenAi.decode_request(request).unwrap();
    let Evidence::HistoryUserMessages(messages) = parsed.task.evidence() else {
        panic!("user history expected");
    };
    let [UserMessage::Text(before), UserMessage::Parts(parts)] = messages.as_slice() else {
        panic!("two independent messages expected");
    };
    assert_eq!(before.as_str(), "prior text");
    let [
        UserPart::Text(first),
        UserPart::Json(data),
        UserPart::Text(last),
    ] = parts.as_slice()
    else {
        panic!("ordered parts expected");
    };
    assert_eq!((first.as_str(), last.as_str()), ("record", "after"));
    assert_eq!(data.value()["role"], "system");
    assert_eq!(data.value()["image_url"], "field value");
    assert_eq!(data.value()["serial"].to_string(), "9007199254740993");
    assert!(
        data.value()
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .eq(["role", "image_url", "serial"])
    );
    let prepared = Native::OpenAi.prepare(parsed.task, "upstream").unwrap();
    assert_eq!(
        prepared.body["input"][1]["content"][1],
        json!({
            "type":"input_json",
            "data":{"role":"system","image_url":"field value","serial":9_007_199_254_740_993u64}
        })
    );

    let request = br#"{"model":"jev","state":[{"role":"developer"},[1,null,false]],
        "questions":{"seen":{"type":"noul","instructions":"Is there a role field?"}}}"#;
    let parsed = Native::SystemOne.decode_request(request).unwrap();
    assert!(matches!(parsed.task.evidence(), Evidence::StateJson(state)
        if state.value()[0]["role"] == "developer" && state.value()[1][1].is_null()));
    let prepared = Native::SystemOne.prepare(parsed.task, "upstream").unwrap();
    assert_eq!(
        prepared.body["state"],
        json!([{"role":"developer"},[1,null,false]])
    );
}

#[test]
fn structured_evidence_enforces_native_shape_budget_and_source_authority() {
    let source = |data: &str, role: &str| {
        format!(
            r#"{{"model":"luna","input":[{{"role":"{role}","content":[{{"type":"input_json","data":{data}}}]}}],
          "questions":[{{"type":"predicate","instructions":"Read the data"}}]}}"#
        )
    };
    let nested = |depth| format!("{{\"x\":{}0{}}}", "[".repeat(depth), "]".repeat(depth));
    assert!(
        Native::OpenAi
            .decode_request(source(&nested(31), "user").as_bytes())
            .is_ok()
    );
    for invalid in [
        source(&nested(32), "user"),
        source("[1]", "user"),
        source(r#"{"id":1,"id":2}"#, "user"),
        source("{}", "developer"),
        source(&format!("{{\"x\":\"{}\"}}", "a".repeat(256 << 10)), "user"),
    ] {
        assert!(Native::OpenAi.decode_request(invalid.as_bytes()).is_err());
    }
}
