use super::*;

fn predicate(name: Option<&str>, instructions: &str) -> Predicate {
    Predicate::new(name.map(str::to_owned), instructions.to_owned()).unwrap()
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
