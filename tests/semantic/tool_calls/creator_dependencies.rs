use crate::events_support::*;
use morphiecore::{
    protocol::{
        fidelity::FidelityRecords,
        openai::{Profile, events::EventEncoder, responses},
    },
    semantic::task::generation::*,
};
use serde_json::json;
#[test]
fn unrelated_resources_preserve_creators_but_owned_filename_and_location_edits_do_not() {
    let wire = envelope(
        "completed",
        json!([
            {"id":"r1","type":"function_call_output","call_id":"c1","status":"completed","output":"one","created_by":"actor1"},
            {"id":"r2","type":"function_call_output","call_id":"c2","status":"completed",
                "output":[{"type":"input_file","file_url":"https://example.test/file.pdf","filename":"before.pdf"}],"created_by":"actor2"}
        ]),
    );
    let parsed = responses::decode_response(&wire).unwrap();
    let (one, first) = &parsed.semantic.items()[0];
    let (two, second) = &parsed.semantic.items()[1];
    assert_eq!(
        parsed
            .fidelity
            .created_by(*one, first, parsed.semantic.resources()),
        Some("actor1")
    );
    assert_eq!(
        parsed
            .fidelity
            .created_by(*two, second, parsed.semantic.resources()),
        Some("actor2")
    );
    let mut edited = second.clone();
    let Item::ToolResult(result) = &mut edited else {
        unreachable!()
    };
    let ToolOutput::Parts(parts) = &mut result.output else {
        unreachable!()
    };
    let InputPart::Resource(resource) = &mut parts[0].content else {
        unreachable!()
    };
    let resource_id = resource.id;
    let ResourceDescription::File(file) = &mut resource.description else {
        unreachable!()
    };
    file.filename = Some(text("after.pdf"));
    assert_eq!(
        parsed
            .fidelity
            .created_by(*two, &edited, parsed.semantic.resources()),
        None
    );
    let changed = ResourceTable::default()
        .insert(
            resource_id,
            ResourceLocation::Url(text("https://example.test/changed.pdf")),
        )
        .unwrap();
    assert_eq!(parsed.fidelity.created_by(*two, second, &changed), None);
    assert_eq!(
        parsed.fidelity.created_by(*one, first, &changed),
        Some("actor1")
    );
}
fn result(status: ItemLifecycle) -> ToolResult {
    ToolResult {
        call_id: Some(text("c")),
        name: None,
        output: "".into(),
        status: Some(status),
        is_error: None,
        execution: None,
        context: Default::default(),
        cache: None,
    }
}
#[test]
fn typed_event_encoding_cannot_change_or_erase_an_already_published_creator() {
    for conflict in [false, true] {
        let id = ItemId::new(1);
        let resources = ResourceTable::default();
        let mut initial = FidelityRecords::default();
        initial.record_response_item_id(id, "result").unwrap();
        initial
            .observe_created_by(
                id,
                &Item::ToolResult(result(ItemLifecycle::InProgress)),
                &resources,
                Some(&json!("A")),
            )
            .unwrap();
        let mut encoder = EventEncoder::new(Profile::Responses, metadata())
            .unwrap()
            .with_contract(contract());
        encoder.encode(&StreamEvent::Started, &initial).unwrap();
        let opened = encoder
            .encode(
                &StreamEvent::ItemStarted {
                    item: id,
                    kind: ItemKind::ToolResult(result(ItemLifecycle::InProgress)),
                    replay: None,
                },
                &initial,
            )
            .unwrap();
        assert_eq!(opened[0]["item"]["created_by"], "A");
        let mut next = FidelityRecords::default();
        next.record_response_item_id(id, "result").unwrap();
        if conflict {
            next.observe_created_by(
                id,
                &Item::ToolResult(result(ItemLifecycle::Completed)),
                &resources,
                Some(&json!("B")),
            )
            .unwrap();
        }
        let closed = encoder.encode(
            &StreamEvent::ItemFinished {
                item: id,
                status: ItemLifecycle::Completed,
                replay: None,
            },
            &next,
        );
        if conflict {
            assert!(closed.is_err());
            assert!(
                encoder
                    .encode(&terminal(StreamTerminal::Completed), &next)
                    .is_err()
            );
        } else {
            assert_eq!(closed.unwrap()[0]["item"]["created_by"], "A");
            let end = encoder
                .encode(&terminal(StreamTerminal::Completed), &next)
                .unwrap();
            assert_eq!(end[0]["response"]["output"][0]["created_by"], "A");
        }
    }
}
