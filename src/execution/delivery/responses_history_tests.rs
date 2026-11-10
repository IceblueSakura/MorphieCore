//! Native signatures remain typed stored history, never fabricated public reasoning.
use super::*;
use crate::{
    adapter::UpstreamAdapter,
    protocol::{
        ResponseStorage,
        failure::CarrierReason,
        google::generate_content::encode_generation_request,
        native::ReplayTarget,
        openai::{Dialect, GenerationApi, GenerationCodec, TargetContract, responses::managed},
    },
    semantic::{task::generation::*, value::Presence},
};
use serde_json::{Value, json};

fn target() -> ReplayTarget {
    ReplayTarget::new("synthetic-google", "synthetic-model").unwrap()
}
fn client() -> GenerationCodec {
    GenerationCodec::new(
        GenerationApi::Responses,
        Dialect::Standard,
        Some(crate::protocol::replay_envelope::CLIENT_ORIGIN.clone()),
    )
}
fn setup(store: Option<bool>) -> (Attempt, ResponseDelivery) {
    let mut attempt = Attempt::new(
        UpstreamAdapter::GoogleGenerateContent { target: target() },
        2 << 20,
        SseLimits::default(),
    );
    attempt.begin(200, "text/event-stream").unwrap();
    let mut delivery = ResponseDelivery::new(
        ClientAdapter::openai(client(), TargetContract::full()),
        "public",
        SseLimits::default(),
        StreamOptions::default(),
        Obfuscation::Disabled,
    );
    if let Some(store) = store {
        delivery = delivery.with_storage(
            "resp_stored",
            ResponseStorage {
                store: Presence::Value(store),
                ..Default::default()
            },
        );
    }
    let request = client()
        .decode_client_request(br#"{"model":"public","input":"go","max_output_tokens":32}"#)
        .unwrap();
    delivery = delivery.with_request(
        &crate::topology::catalog::GOOGLE_BINDINGS[0].endpoint(),
        &request,
        &request,
    );
    (attempt, delivery)
}
fn source_parts() -> Value {
    json!([
        {"thought":true,"text":"plan","thoughtSignature":"UjE="},
        {"text":"OK","thoughtSignature":"UzE="},
        {"text":"OK","thoughtSignature":"UzI="},
        {"functionCall":{"id":"c","name":"lookup","args":{"n":9007199254740993u64}},"thoughtSignature":"UzM="}
    ])
}
fn push(attempt: &mut Attempt) -> Vec<StreamEvent> {
    let wire = format!(
        "data: {}\n\n",
        json!({"responseId":"r","modelVersion":"synthetic-model",
        "candidates":[{"content":{"role":"model","parts":source_parts()},"finishReason":"MAX_TOKENS"}],
        "usageMetadata":{"promptTokenCount":10,"cachedContentTokenCount":0,"candidatesTokenCount":5,"thoughtsTokenCount":1,"totalTokenCount":16}})
    );
    let (n, events) = attempt.push(wire.as_bytes()).unwrap();
    assert_eq!(n, wire.len());
    events
}
fn check_public(value: &Value) {
    assert_eq!(value["status"], "incomplete");
    let output = value["output"].as_array().unwrap();
    assert_eq!(output.len(), 4);
    assert_eq!(
        output
            .iter()
            .map(|v| v["type"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["reasoning", "message", "message", "function_call"]
    );
    assert_eq!(output[0]["summary"][0]["text"], "plan");
    assert!(
        output[0]["encrypted_content"]
            .as_str()
            .is_some_and(|v| v.starts_with("mc1."))
    );
    assert_eq!(output[1]["content"][0]["text"], "OK");
    assert_eq!(output[2]["content"][0]["text"], "OK");
    let arguments: Value = serde_json::from_str(output[3]["arguments"].as_str().unwrap()).unwrap();
    assert_eq!(arguments, json!({"n":9007199254740993u64}));
    assert!(!value.to_string().contains("mca1."));
}
#[test]
fn missing_and_late_google_ids_keep_delivery_identity_in_json_sse_and_retained_views() {
    for late in [false, true] {
        for stream in [false, true] {
            let (mut attempt, mut delivery) = setup(None);
            let mut frames = [
                json!({"modelVersion":"synthetic-model",
                    "candidates":[{"content":{"role":"model","parts":[{"text":"O"}]}}]}),
                json!({"candidates":[{"content":{"role":"model","parts":[{"text":"K"}]},
                    "finishReason":"STOP"}]}),
            ];
            if late {
                frames[1]["responseId"] = json!("late-upstream");
            }
            let mut output = vec![];
            let mut id = None;
            for frame in frames {
                let (_, events) = attempt
                    .push(format!("data: {frame}\n\n").as_bytes())
                    .unwrap();
                if stream {
                    output.extend(
                        delivery
                            .encode_events(&attempt, &events)
                            .unwrap()
                            .into_bytes()
                            .concat(),
                    );
                } else {
                    delivery.observe_events(&attempt, &events).unwrap();
                }
                id.get_or_insert_with(|| delivery.generated_id.clone().unwrap());
            }
            let id = id.unwrap();
            assert_ne!(id, "late-upstream");
            assert!(!String::from_utf8_lossy(&output).contains("response.completed"));
            attempt.finish().unwrap();
            assert_eq!(
                attempt.response().unwrap().metadata.id.as_deref(),
                late.then_some("late-upstream")
            );
            let delivered: Value = if stream {
                output.extend(
                    delivery
                        .finish_stream(&attempt)
                        .unwrap()
                        .into_bytes()
                        .concat(),
                );
                let events: Vec<Value> = String::from_utf8(output)
                    .unwrap()
                    .lines()
                    .filter_map(|line| line.strip_prefix("data: "))
                    .map(|line| serde_json::from_str(line).unwrap())
                    .collect();
                for event in &events {
                    if let Some(response) = event.get("response") {
                        assert_eq!(response["id"], id);
                    }
                }
                assert_eq!(events.last().unwrap()["type"], "response.completed");
                events.last().unwrap()["response"].clone()
            } else {
                serde_json::from_slice(&delivery.encode_json(&attempt).unwrap()).unwrap()
            };
            assert_eq!(delivered["id"], id);
            assert_eq!(delivered["output"][0]["content"][0]["text"], "OK");
            let (retained, bytes) = delivery.snapshot(&attempt).unwrap();
            assert_eq!(retained.metadata.id.as_deref(), Some(id.as_str()));
            let retained: Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(retained["id"], delivered["id"]);
            assert_eq!(retained["output"], delivered["output"]);
        }
    }
}

#[test]
fn stored_responses_keep_signed_owners_without_public_manifest_in_json_and_sse() {
    for stream in [false, true] {
        let (mut attempt, mut delivery) = setup(Some(true));
        let events = push(&mut attempt);
        let prefix = if stream {
            delivery
                .encode_events(&attempt, &events)
                .map(|batch| batch.into_bytes())
                .unwrap()
                .concat()
        } else {
            delivery.observe_events(&attempt, &events).unwrap();
            vec![]
        };
        assert!(!String::from_utf8_lossy(&prefix).contains("response.incomplete"));
        attempt.finish().unwrap();
        let retained = if stream {
            let suffix = delivery
                .finish_stream(&attempt)
                .map(|batch| batch.into_bytes())
                .unwrap()
                .concat();
            let text = String::from_utf8(suffix).unwrap();
            let terminal: Value = text
                .lines()
                .filter_map(|l| l.strip_prefix("data: "))
                .map(|l| serde_json::from_str::<Value>(l).unwrap())
                .next_back()
                .unwrap();
            assert_eq!(terminal["type"], "response.incomplete");
            check_public(&terminal["response"]);
            let (retained, body) = delivery.snapshot(&attempt).unwrap();
            let mut json = serde_json::from_slice::<Value>(&body).unwrap();
            let mut streamed = terminal["response"].clone();
            json.as_object_mut().unwrap().remove("morphiecore");
            streamed.as_object_mut().unwrap().remove("morphiecore");
            assert_eq!(json, streamed);
            retained
        } else {
            let (retained, body) = delivery
                .encode_json_snapshot(&attempt)
                .map(|(response, batch)| (response, batch.into_bytes().concat()))
                .unwrap();
            let public = serde_json::from_slice::<Value>(&body).unwrap();
            check_public(&public);
            for item in public["output"].as_array().unwrap() {
                let id = item["id"].as_str().unwrap();
                assert!(
                    retained
                        .semantic
                        .items()
                        .iter()
                        .any(|(owner, _)| retained.fidelity.response_item_id(*owner) == Some(id))
                );
            }
            retained
        };
        assert!(delivery.complete(&attempt).is_err());
        delivery.commit().unwrap();
        delivery.complete(&attempt).unwrap();
        assert_eq!(
            retained
                .semantic
                .items()
                .iter()
                .filter(|(_, item)| item.has_replay_attachment())
                .count(),
            3
        );
        let old = client()
            .decode_client_request(br#"{"model":"public","input":"go"}"#)
            .unwrap();
        let (mut next, _) = managed::decode(&client(), br#"{"model":"public","input":[{"type":"function_call_output","call_id":"c","output":"ok"}]}"#,
            Some((&old.task, &retained))).unwrap();
        let sealed_history = next.task.clone();
        let listed = managed::input_items(&next.task, "resp_next", Some("order=asc")).unwrap();
        assert_eq!(listed["data"].as_array().unwrap().len(), 6);
        assert!(!listed.to_string().contains("mca1."));
        assert!(!listed.to_string().contains("thoughtSignature"));
        assert_eq!(
            next.task, sealed_history,
            "public input listing cannot edit hidden history"
        );
        super::super::replay::open_from_client(&mut next).unwrap();
        let (wire, _) =
            encode_generation_request(&next.task.semantic, &next.task.fidelity, &target(), None)
                .unwrap();
        assert_eq!(wire["contents"][1]["parts"], source_parts());
        assert_eq!(
            wire["contents"][2]["parts"],
            json!([{"functionResponse":{"id":"c","name":"lookup","response":{"output":"ok"}}}])
        );
    }
}
#[test]
fn stateless_owner_replay_preserves_signed_parts_calls_and_next_native_request() {
    for stream in [false, true] {
        let (mut attempt, mut delivery) = setup(Some(false));
        delivery.client.enable_owner_replay().unwrap();
        let wire = format!(
            "data: {}\n\n",
            json!({"modelVersion":"synthetic-model",
                "candidates":[{"content":{"role":"model","parts":source_parts()},
                    "finishReason":"STOP"}]})
        );
        let (_, events) = attempt.push(wire.as_bytes()).unwrap();
        let mut output = vec![];
        if stream {
            output.extend(
                delivery
                    .encode_events(&attempt, &events)
                    .unwrap()
                    .into_bytes()
                    .concat(),
            );
        } else {
            delivery.observe_events(&attempt, &events).unwrap();
        }
        attempt.finish().unwrap();
        let body: Value = if stream {
            output.extend(
                delivery
                    .finish_stream(&attempt)
                    .unwrap()
                    .into_bytes()
                    .concat(),
            );
            let events: Vec<Value> = String::from_utf8(output)
                .unwrap()
                .lines()
                .filter_map(|line| line.strip_prefix("data: "))
                .map(|line| serde_json::from_str(line).unwrap())
                .collect();
            let terminal = events.last().unwrap();
            assert_eq!(terminal["type"], "response.completed");
            for event in &events {
                if event["type"] == "response.output_item.done" {
                    let item = &event["item"];
                    let final_item = terminal["response"]["output"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|final_item| final_item["id"] == item["id"])
                        .unwrap();
                    assert_eq!(item["morphiecore_replay"], final_item["morphiecore_replay"]);
                }
            }
            terminal["response"].clone()
        } else {
            serde_json::from_slice(&delivery.encode_json(&attempt).unwrap()).unwrap()
        };
        let owners = body["output"].as_array().unwrap();
        assert_eq!(owners.len(), 4, "no synthetic reasoning owner");
        assert_eq!(owners[1]["content"][0]["text"], "OK");
        assert_eq!(owners[2]["content"][0]["text"], "OK");
        let tokens = [
            (
                &owners[1]["morphiecore_replay"]["parts"][0]["token"],
                "UzE=",
            ),
            (
                &owners[2]["morphiecore_replay"]["parts"][0]["token"],
                "UzI=",
            ),
            (&owners[3]["morphiecore_replay"]["token"], "UzM="),
        ];
        for (value, native) in tokens {
            let (_, _, payload) =
                crate::protocol::replay_envelope::open(value.as_str().unwrap()).unwrap();
            assert_eq!(payload.as_str(), native);
        }
        let mut codec = client();
        codec.adaptation.rules.responses_owner_replay = true;
        let mut input = owners.clone();
        input.insert(0, json!({"role":"user","content":"go"}));
        input.push(json!({"type":"function_call_output","call_id":"c","output":"ok"}));
        let request = json!({"model":"public","store":false,"input":input});
        let standard = client()
            .decode_client_request(request.to_string().as_bytes())
            .unwrap();
        assert!(
            !standard
                .task
                .semantic
                .items()
                .iter()
                .any(|(_, item)| item.has_replay_attachment())
        );
        let mut next = codec
            .decode_client_request(request.to_string().as_bytes())
            .unwrap();
        super::super::replay::open_from_client(&mut next).unwrap();
        let (native, _) =
            encode_generation_request(&next.task.semantic, &next.task.fidelity, &target(), None)
                .unwrap();
        assert_eq!(native["contents"][1]["parts"], source_parts());
        let mut edited = next.task.semantic.items().to_vec();
        for (_, item) in &mut edited {
            if let Item::Message(message) = item
                && message.parts.iter().any(|part| part.replay.is_some())
            {
                message.parts[0].content = ContentPart::Text(
                    crate::semantic::value::Text::new("edited", "text", 64)
                        .unwrap()
                        .into(),
                );
                break;
            }
        }
        next.task.semantic = next.task.semantic.with_items(edited).unwrap();
        assert!(
            encode_generation_request(&next.task.semantic, &next.task.fidelity, &target(), None,)
                .is_err()
        );
    }
}

#[test]
fn stateless_responses_reject_hidden_signatures_and_poison_delivery() {
    let (mut attempt, _) = setup(None);
    push(&mut attempt);
    attempt.finish().unwrap();
    let mut source = attempt.response().unwrap().clone();
    super::super::replay::seal_for_client(
        &mut source,
        &crate::protocol::replay_envelope::CLIENT_ORIGIN,
    )
    .unwrap();
    source.metadata.storage = Some(ResponseStorage {
        store: Presence::Value(true),
        ..Default::default()
    });
    let client = ClientAdapter::openai(client(), TargetContract::full());
    assert!(
        matches!(client.snapshot(source),
        Err(crate::protocol::ConversionFailure { cause: crate::protocol::ConversionCause::Codec(CodecError::Unsupported(reason)), .. })
            if matches!(reason,CarrierReason::Replay)),
        "reported storage does not authorize execution-owned retention"
    );
    for store in [None, Some(false)] {
        for stream in [false, true] {
            let (mut attempt, mut delivery) = setup(store);
            let events = push(&mut attempt);
            let result = if stream {
                delivery
                    .encode_events(&attempt, &events)
                    .map(|batch| batch.into_bytes())
                    .map(|_| ())
            } else {
                attempt.finish().unwrap();
                delivery.encode_json(&attempt).map(|_| ())
            };
            assert!(
                matches!(result, Err(AttemptError::Codec(CodecError::Unsupported(reason)))
                    | Err(AttemptError::Conversion(crate::protocol::ConversionFailure { cause: crate::protocol::ConversionCause::Codec(CodecError::Unsupported(reason)), .. }))
                    if matches!(reason,CarrierReason::Replay)),
                "{result:?}"
            );
            assert!(delivery.commit().is_err());
            assert!(delivery.encode_json(&attempt).is_err());
        }
    }
}
