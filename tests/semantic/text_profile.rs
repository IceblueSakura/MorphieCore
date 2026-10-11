//! Independent field and transformation contracts for stateless Responses text.
use crate::wire;
use morphiecore::{
    protocol::openai::{
        DecodedRequest, Profile, TargetCaps as Contract, chat, envelope,
        events::EventDecoder,
        lowering::{lower_request, lower_response},
        responses,
    },
    semantic::{
        task::generation::*,
        value::{Presence, ReplayOrigin, Text},
    },
};
use serde_json::{Value, json};
fn origin() -> ReplayOrigin {
    ReplayOrigin::new("synthetic-loopback").unwrap()
}
fn contract() -> Contract {
    let mut c = Contract::full();
    c.replay_origin = Some(origin());
    c
}
fn text(s: &str) -> Text {
    Text::allowing_empty(s, "synthetic", MAX_TEXT_BYTES).unwrap()
}
fn request_wire(d: &DecodedRequest) -> Value {
    responses::encode_generation(
        &lower_request(&d.semantic, &d.fidelity, Profile::Responses, contract()).unwrap(),
    )
    .unwrap()
}
fn chat_wire(d: &DecodedRequest) -> Value {
    chat::encode_generation(
        &lower_request(&d.semantic, &d.fidelity, Profile::Chat, contract()).unwrap(),
    )
    .unwrap()
}

#[test]
fn response_format_shells_map_to_one_owner_with_independent_shapes() {
    let closed = json!({"type":"object","properties":{"zeta":{"type":"string"}},"required":["zeta"],"additionalProperties":false});
    let full = OutputConstraint::JsonSchema {
        name: text("answer"),
        description: Some(text("pick")),
        schema: closed.clone().into(),
        strict: Some(true),
    };
    let bare = OutputConstraint::JsonSchema {
        name: text("answer"),
        description: None,
        schema: closed.clone().into(),
        strict: None,
    };
    for (format, chat_format, responses_text, structured) in [
        (None, None, None, false),
        (
            Some(Presence::Null),
            Some(Value::Null),
            Some(json!({"format":null})),
            false,
        ),
        (
            Some(Presence::Value(OutputConstraint::Text)),
            Some(json!({"type":"text"})),
            Some(json!({"format":{"type":"text"}})),
            false,
        ),
        (
            Some(Presence::Value(OutputConstraint::JsonObject)),
            Some(json!({"type":"json_object"})),
            Some(json!({"format":{"type":"json_object"}})),
            true,
        ),
        (
            Some(Presence::Value(full.clone())),
            Some(
                json!({"type":"json_schema","json_schema":{"name":"answer","description":"pick","schema":closed,"strict":true}}),
            ),
            Some(
                json!({"format":{"type":"json_schema","name":"answer","description":"pick","schema":closed,"strict":true}}),
            ),
            true,
        ),
        (
            Some(Presence::Value(bare.clone())),
            Some(json!({"type":"json_schema","json_schema":{"name":"answer","schema":closed}})),
            Some(json!({"format":{"type":"json_schema","name":"answer","schema":closed}})),
            true,
        ),
    ] {
        let options = TextOptions {
            presence: format.is_some(),
            format: format.unwrap_or(Presence::Absent),
            verbosity: Presence::Absent,
        };
        let mut source = json!({"messages":[{"role":"user","content":"hello"}]});
        if let Some(value) = &chat_format {
            source["response_format"] = value.clone();
        }
        let d = chat::decode_generation(&source).unwrap();
        assert_eq!(d.semantic.text_options(), &options);
        assert_eq!(chat_wire(&d).get("response_format"), chat_format.as_ref());
        let mut source = json!({"input":"hello"});
        if let Some(value) = &responses_text {
            source["text"] = value.clone();
        }
        let r = responses::decode_generation(&source).unwrap();
        assert_eq!(r.semantic.text_options(), &options);
        assert_eq!(request_wire(&r).get("text"), responses_text.as_ref());
        assert_eq!(
            GenerationRequirements::derive(&d.semantic).structured_output,
            structured
        );
        let mut limited = contract();
        limited.semantics.structured_output = false;
        for profile in [Profile::Chat, Profile::Responses] {
            assert_eq!(
                lower_request(&d.semantic, &d.fidelity, profile, limited.clone()).is_err(),
                structured
            );
        }
    }
}

#[test]
fn instruction_status_is_checked_in_history_and_reported_echoes() {
    for role in ["system", "developer"] {
        for status in [
            None,
            Some(json!("completed")),
            Some(Value::Null),
            Some(json!(false)),
            Some(json!("unknown")),
            Some(json!("in_progress")),
            Some(json!("incomplete")),
        ] {
            let lifecycle = matches!(&status, Some(value) if value == &json!("in_progress") || value == &json!("incomplete"));
            let accepted = status.is_none() || status == Some(json!("completed")) || lifecycle;
            let mut item = json!({"type":"message","role":role,"content":[{"type":"input_text","text":"Be precise"}]});
            if let Some(status) = status.clone() {
                item["status"] = status;
            }
            let decoded = responses::decode_generation(&json!({"input":[item.clone()]}));
            assert_eq!(decoded.is_ok(), accepted, "{item}");
            if let Ok(d) = decoded {
                let encoded = request_wire(&d)["input"][0].clone();
                if lifecycle {
                    assert_eq!(encoded["status"], item["status"]);
                    assert_eq!(encoded["type"], json!("message"));
                } else {
                    assert!(encoded.get("status").is_none());
                    assert_eq!(encoded["content"], json!("Be precise"));
                }
            }
            let mut response = wire::response(2);
            response["instructions"] = json!([item]);
            let decoded = envelope::decode_response(&response);
            assert_eq!(decoded.is_ok(), accepted);
            if let Ok(d) = decoded {
                let output = envelope::encode_response(
                    &lower_response(
                        &d.semantic,
                        &d.fidelity,
                        &d.metadata,
                        Profile::Responses,
                        contract(),
                    )
                    .unwrap(),
                )
                .unwrap();
                let encoded = &output["instructions"][0];
                if lifecycle {
                    assert_eq!(encoded["status"], item["status"]);
                } else {
                    assert!(encoded.get("status").is_none());
                    assert_eq!(encoded["content"], json!("Be precise"));
                }
            }
        }
    }
}

#[test]
fn custom_tools_and_text_array_results_are_not_function_arguments() {
    let mut d=responses::decode_generation(&json!({"input":[{"type":"custom_tool_call","call_id":"c","name":"sql","input":"SELECT 1"},{"type":"custom_tool_call_output","call_id":"c","output":[{"type":"input_text","text":"1"},{"type":"input_text","text":""}]}],"tools":[{"type":"custom","name":"sql","format":{"type":"grammar","syntax":"regex","definition":"SELECT [0-9]+"}}],"tool_choice":{"type":"custom","name":"sql"}})).unwrap();
    assert!(matches!(&d.semantic.items()[0].1,Item::CustomCall(c) if c.input=="SELECT 1"));
    assert!(GenerationRequirements::derive(&d.semantic).custom_tools);
    let mut items = d.semantic.items().to_vec();
    items[0].0 = ItemId::new(99);
    items[1].0 = ItemId::new(100);
    let Item::CustomCall(c) = &mut items[0].1 else {
        panic!()
    };
    c.input = "not JSON, not executed".into();
    c.call_id = text("edited-custom-call");
    let Item::CustomResult(r) = &mut items[1].1 else {
        panic!()
    };
    // Explicit synthetic report for the new call, never automatic result relinking.
    r.call_id = text("edited-custom-call");
    r.output = ToolOutput::Text("changed".into());
    d.semantic = d.semantic.with_items(items).unwrap();
    let out = request_wire(&d);
    assert_eq!(out["input"][0]["input"], "not JSON, not executed");
    assert!(out["input"][0].get("arguments").is_none());
    assert_eq!(out["input"][1]["output"], "changed");
    assert!(lower_request(&d.semantic, &d.fidelity, Profile::Chat, contract()).is_err());
    let mut mismatch = out;
    mismatch["input"][1]["type"] = json!("function_call_output");
    assert!(responses::decode_generation(&mismatch).is_err());
}
#[test]
fn null_empty_false_and_default_controls_are_deliberate_not_unknown_passthrough() {
    let d=responses::decode_generation(&json!({"input":"","instructions":null,"reasoning":{"effort":null,"summary":null},"tools":[],"tool_choice":"none","parallel_tool_calls":false,"text":{"format":{"type":"text"},"verbosity":null}})).unwrap();
    let out = request_wire(&d);
    assert_eq!(out["instructions"], Value::Null);
    assert_eq!(out["reasoning"], json!({"effort":null,"summary":null}));
    assert_eq!(out["parallel_tool_calls"], false);
    assert_eq!(out["tools"], json!([]));
    for bad in [
        json!({"top_logprobs":21}),
        json!({"top_p":1.1}),
        json!({"temperature":-1}),
        json!({"text":{"format":{"type":"json_schema","name":"bad name","schema":{}}}}),
        json!({"tools":[{"type":"custom","name":"x","format":{"type":"grammar","syntax":"unknown","definition":"x"}}]}),
        json!({"tools":[{"type":"function","name":"x","async":"yes"}]}),
        json!({"reasoning":{"unknown":true}}),
        json!({"text":null}),
        json!({"tools":null}),
        json!({"tool_choice":null}),
        json!({"reasoning":{"mode":null}}),
    ] {
        let mut value = json!({"input":"hello"});
        value
            .as_object_mut()
            .unwrap()
            .extend(bad.as_object().unwrap().clone());
        assert!(responses::decode_generation(&value).is_err());
    }
}

#[test]
fn probabilities_annotations_and_snapshots_cannot_rewrite_observed_values() {
    for field in ["probability", "annotation", "terminal"] {
        let mut values = wire::events(1);
        match field {
            "probability" => {
                let v = values
                    .iter_mut()
                    .find(|v| v["type"] == "response.content_part.done")
                    .unwrap();
                v["part"]["logprobs"][0]["logprob"] = json!(-0.75);
            }
            "annotation" => {
                let v = values
                    .iter_mut()
                    .find(|v| v["type"] == "response.output_text.annotation.added")
                    .unwrap();
                v["annotation_index"] = json!(1);
            }
            _ => {
                values.last_mut().unwrap()["response"]["output"][3]["content"][0]["text"] =
                    json!("resurrected");
            }
        }
        let mut d = EventDecoder::new(Profile::Responses).with_replay_origin(origin());
        assert!(values.iter().any(|v| d.push(v).is_err()), "{field}");
        assert!(d.finish().is_err());
    }
}
fn responses_round_trip(request: &Value) -> Value {
    let d = envelope::decode_request(request).unwrap();
    envelope::encode_request(
        &lower_request(
            &d.task.semantic,
            &d.task.fidelity,
            Profile::Responses,
            contract(),
        )
        .unwrap(),
        &d.context,
    )
    .unwrap()
}
fn response_round_trip(response: &Value) -> Value {
    let d = envelope::decode_response(response).unwrap();
    envelope::encode_response(
        &lower_response(
            &d.semantic,
            &d.fidelity,
            &d.metadata,
            Profile::Responses,
            contract(),
        )
        .unwrap(),
    )
    .unwrap()
}
#[test]
fn reported_cache_context_is_an_independent_response_fact() {
    let mut response = wire::response(2);
    response["prompt_cache_options"] = json!({"mode":"explicit","ttl":"30m","prewarm":true});
    response["prompt_cache_diagnostics"] = json!({"type":"cache_miss","reason":"input_changed","cache_missed_tokens":3,"comparison_reusable_tokens":2});
    let out = response_round_trip(&response);
    assert_eq!(
        out["prompt_cache_options"],
        json!({"mode":"explicit","ttl":"30m","prewarm":true})
    );
    assert_eq!(
        out["prompt_cache_diagnostics"],
        response["prompt_cache_diagnostics"]
    );
    // The effective tier is a reported fact and stays independent of the request hint.
    assert_eq!(out["service_tier"], json!("default"));
    assert_eq!(
        responses_round_trip(&wire::request(false))["service_tier"],
        json!("auto")
    );
    // Nothing is invented for absent echoes.
    let mut bare = wire::response(2);
    for key in [
        "prompt_cache_key",
        "prompt_cache_retention",
        "prompt_cache_options",
        "prompt_cache_diagnostics",
        "safety_identifier",
        "user",
        "metadata",
    ] {
        bare.as_object_mut().unwrap().remove(key);
    }
    let out = response_round_trip(&bare);
    for key in [
        "prompt_cache_key",
        "prompt_cache_retention",
        "prompt_cache_options",
        "prompt_cache_diagnostics",
        "safety_identifier",
        "user",
        "metadata",
    ] {
        assert!(out.get(key).is_none(), "{key}");
    }
}
