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
            check_public(&serde_json::from_slice::<Value>(&body).unwrap());
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
