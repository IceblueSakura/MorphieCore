use super::*;
use crate::semantic::task::decision::BinaryCriteria;

fn t(text: &str) -> Text {
    Text::allowing_empty(text, "fixture", MAX_TEXT_BYTES).unwrap()
}

fn request(evidence: Evidence) -> Request {
    let question = |kind| Question::new(Some("same".into()), Some(t("Evaluate.")), kind).unwrap();
    Request::new(
        evidence,
        vec![
            question(QuestionKind::Predicate {
                criteria: Some(BinaryCriteria {
                    when_true: Some(t("Delivered damaged")),
                    when_false: None,
                }),
            }),
            question(QuestionKind::SingleChoice(
                ["é", "2", "10"]
                    .map(|id| ChoiceOption {
                        value: ChoiceValue::String(t(id)),
                        criterion: (id == "é").then(|| t("Escalate")),
                    })
                    .into(),
            )),
            question(QuestionKind::OrdinalScore(
                ["Routine", "Review", "Urgent"]
                    .map(|text| ScoreLevel {
                        label: None,
                        criterion: Some(t(text)),
                    })
                    .into(),
            )),
        ],
    )
    .unwrap()
}

#[test]
fn prepares_ordered_text_parts_and_preserves_native_criteria() {
    let result = prepare(
        request(Evidence::TextParts(vec![t("a "), t("broken parcel")])),
        "gemini-decision-1",
    )
    .unwrap();
    assert_eq!(
        result.body,
        json!({
            "model":"gemini-decision-1",
            "input":[{"type":"text","text":"a "},{"type":"text","text":"broken parcel"}],
            "questions":{
                "0":{"type":"noul","instructions":"Evaluate.","criteria":{"true_description":"Delivered damaged"}},
                "1":{"type":"choice","instructions":"Evaluate.","criteria":{"10":"","2":"","é":"Escalate"}},
                "2":{"type":"score","instructions":"Evaluate.","criteria":["Routine","Review","Urgent"]}
            }
        })
    );

    for evidence in [
        Evidence::HistoryUserText(t("retain the role")),
        Evidence::StateJson(StructuredEvidence::new(json!({"record":"do not stringify"})).unwrap()),
    ] {
        assert!(prepare(request(evidence), "gemini-decision-1").is_err());
    }
    let typed_options = Question::new(
        None,
        Some(t("Choose")),
        QuestionKind::SingleChoice(vec![
            ChoiceOption {
                value: ChoiceValue::Bool(true),
                criterion: None,
            },
            ChoiceOption {
                value: ChoiceValue::String(t("true")),
                criterion: None,
            },
        ]),
    )
    .unwrap();
    let request = Request::new(Evidence::StateText(t("text")), vec![typed_options]).unwrap();
    assert!(prepare(request, "gemini-decision-1").is_err());
}

fn report() -> Value {
    // Keyed reports are deliberately shuffled relative to source slots.
    json!({
        "answers":{
            "2":{"type":"score","score":1.12,"legend":{"0":"Routine","1":"Review","2":"Urgent"},
                 "probabilities":{"2":0.15,"0":0.03,"1":0.83},"confidence":0.83},
            "0":{"type":"noul","noul":0.03,"confidence":0.97},
            "1":{"type":"choice","choice":"10","probabilities":{"2":0.4,"é":0.2,"10":0.4},"confidence":0.4}
        },
        "usage":{"total_input_tokens":100,"input_tokens_by_modality":[{"modality":"text","tokens":100}],
                 "total_output_tokens":0,"total_tokens":100}
    })
}

fn complete(bytes: &[u8]) -> Result<Evaluation, CodecError> {
    prepare(
        request(Evidence::StateText(t("Synthetic input."))),
        "gemini-decision-1",
    )?
    .expectation
    .complete(bytes)
}

#[test]
fn validates_gemini_ties_then_restores_source_slots_without_recomputing_reports() {
    let result = complete(report().to_string().as_bytes()).unwrap();
    let Disposition::Evaluated(answers) = result.disposition else {
        panic!("expected evaluation")
    };
    let [
        Outcome::Answered(Answer::Predicate {
            probability: p,
            confidence: Some(Confidence::WinningMass(c)),
        }),
        Outcome::Answered(Answer::SingleChoice {
            selected,
            probabilities,
            confidence: Confidence::WinningMass(_),
        }),
        Outcome::Answered(Answer::OrdinalScore {
            score,
            probabilities: levels,
            confidence: Confidence::WinningMass(_),
        }),
    ] = answers.outcomes()
    else {
        panic!("expected three typed answers")
    };
    assert_eq!(
        (p.as_number().as_f64(), c.as_number().as_f64()),
        (Some(0.03), Some(0.97))
    );
    assert_eq!(*selected, 2); // "10" sorts before "2", but is the third source option.
    assert_eq!(
        probabilities
            .iter()
            .map(|p| mass(p).unwrap())
            .collect::<Vec<_>>(),
        [0.2, 0.4, 0.4]
    );
    assert_eq!(score.as_f64(), Some(1.12));
    assert_eq!(
        levels.iter().map(|p| mass(p).unwrap()).collect::<Vec<_>>(),
        [0.03, 0.83, 0.15]
    );
    let report = result.usage.unwrap();
    assert_eq!(
        (
            report.input_tokens,
            report.output_tokens,
            report.total_tokens
        ),
        (Some(100), Some(0), Some(100))
    );
    assert_eq!(
        report.input_by_modality.unwrap(),
        [ModalityTokenCount {
            modality: InputModality::Text,
            tokens: 100
        }]
    );
}

#[test]
fn rejects_inconsistent_reports_instead_of_repairing_them() {
    for (path, invalid) in [
        ("/answers/0/noul", json!(0)),
        ("/answers/0/confidence", json!(0.03)),
        ("/answers/1/choice", json!("2")),
        ("/answers/1/probabilities/é", json!(0.8)),
        ("/answers/2/legend/1", json!("reworded rubric")),
        ("/answers/2/score", json!(1.4)),
        ("/answers/2/confidence", json!(0.4)),
        ("/usage/total_output_tokens", json!(-1)),
    ] {
        let mut value = report();
        *value.pointer_mut(path).unwrap() = invalid;
        assert!(complete(value.to_string().as_bytes()).is_err(), "{path}");
    }
    let mut value = report();
    let answers = value["answers"].as_object_mut().unwrap();
    answers.remove("0");
    answers.insert(
        "unknown".into(),
        json!({"type":"noul","noul":0.03,"confidence":0.97}),
    );
    assert!(complete(value.to_string().as_bytes()).is_err());
    let duplicate = report()
        .to_string()
        .replacen("\"answers\":{", "\"answers\":{\"0\":null,", 1);
    assert!(complete(duplicate.as_bytes()).is_err());
}

#[test]
fn blocked_requires_an_explicit_reason_and_no_answers() {
    let result = complete(br#"{"prompt_feedback":{"block_reason":"safety","block_reason_message":"source-only"},"answers":{}}"#).unwrap();
    let Disposition::Blocked(block) = result.disposition else {
        panic!("expected whole-request block")
    };
    assert_eq!(block.reason.as_str(), "safety");
    assert_eq!(block.message.unwrap().as_str(), "source-only");
    assert!(result.usage.is_none());
    for incomplete in [
        br#"{"answers":{}}"#.as_slice(),
        br#"{"answers":{},"prompt_feedback":{"block_reason":"safety"}"#,
        br#"{"answers":{"0":{}},"prompt_feedback":{"block_reason":"safety"}}"#,
    ] {
        assert!(complete(incomplete).is_err());
    }
}
