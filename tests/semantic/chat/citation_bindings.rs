use morphiecore::semantic::{
    task::generation::*,
    value::{Presence, Text},
};
fn text(s: &str) -> Text {
    Text::allowing_empty(s, "synthetic", MAX_TEXT_BYTES).unwrap()
}
fn sources(s: &str) -> ResourceTable {
    ResourceTable::new(vec![(
        ResourceId::new(1),
        ResourceLocation::OpaqueReference(text(s)),
    )])
    .unwrap()
}
fn cite(table: &ResourceTable, coordinates: SourceCoordinates) -> Annotation {
    Annotation::new(
        Citation {
            kind: CitationKind::Citation,
            claim: ClaimAnchor::Whole,
            source: ResourceId::new(1),
            coordinates,
            label: None,
            reported_ordinal: None,
        },
        table,
    )
    .unwrap()
}
fn message(id: u64, annotations: Vec<Annotation>) -> (ItemId, Item) {
    (
        ItemId::new(id),
        Item::Message(Message {
            participant: None,
            role: MessageRole::Assistant,
            status: ItemLifecycle::Completed,
            phase: None,
            parts: vec![Part {
                replay: None,
                id: PartId::new(id),
                cache: None,
                content: ContentPart::Text(
                    TextContent::new(text("claim"), annotations, Presence::Absent).unwrap(),
                ),
            }],
        }),
    )
}
#[test]
fn source_coordinate_shapes_do_not_invent_document_extents_or_wire_carriers() {
    use morphiecore::protocol::{
        fidelity::FidelityRecords,
        openai::{GenerationApi, TargetContract, lowering::lower_response},
    };
    let table = ResourceTable::new(vec![(
        ResourceId::new(1),
        ResourceLocation::Url(text("https://example.invalid/document")),
    )])
    .unwrap();
    let native = Citation {
        kind: CitationKind::Citation,
        claim: ClaimAnchor::Range(TextRange {
            unit: TextUnit::UnicodeScalars,
            start: 0,
            end: 1,
        }),
        source: ResourceId::new(1),
        coordinates: SourceCoordinates::Unreported,
        label: Some(text("document")),
        reported_ordinal: None,
    };
    let metadata = morphiecore::protocol::openai::responses::envelope::decode_response(
        &crate::wire::response(2),
    )
    .unwrap()
    .metadata;
    let positive = GenerationResponse::from_resources(
        vec![message(
            1,
            vec![Annotation::new(native.clone(), &table).unwrap()],
        )],
        Outcome::Completed,
        table.clone(),
    )
    .unwrap();
    assert!(
        lower_response(
            &positive,
            &FidelityRecords::default(),
            &metadata,
            GenerationApi::Responses,
            TargetContract::full()
        )
        .is_ok()
    );
    // Citations describe generated text; no request carrier attaches them to user input.
    let (id, Item::Message(mut user)) =
        message(1, vec![Annotation::new(native.clone(), &table).unwrap()])
    else {
        panic!("message")
    };
    user.role = MessageRole::User;
    assert_eq!(
        GenerationRequest::from_resources(
            vec![(id, Item::Message(user))],
            GenerationSettings::default(),
            table.clone()
        ),
        Err(GenerationError::TextMetadataInUserMessage)
    );
    for coordinates in [
        SourceCoordinates::Pages { start: 1, end: 3 },
        SourceCoordinates::Blocks { start: 0, end: 2 },
        SourceCoordinates::Text(TextRange {
            unit: TextUnit::UnicodeScalars,
            start: 0,
            end: 999,
        }),
    ] {
        let annotation = Annotation::new(
            Citation {
                coordinates,
                ..native.clone()
            },
            &table,
        )
        .unwrap();
        let response = GenerationResponse::from_resources(
            vec![message(1, vec![annotation])],
            Outcome::Completed,
            table.clone(),
        )
        .unwrap();
        for profile in [GenerationApi::Responses, GenerationApi::Chat] {
            assert!(
                lower_response(
                    &response,
                    &FidelityRecords::default(),
                    &morphiecore::protocol::openai::responses::envelope::decode_response(
                        &crate::wire::response(2)
                    )
                    .unwrap()
                    .metadata,
                    profile,
                    TargetContract::full()
                )
                .is_err()
            );
        }
    }
    for coordinates in [
        SourceCoordinates::Pages { start: 0, end: 2 },
        SourceCoordinates::Blocks { start: 2, end: 2 },
        SourceCoordinates::Text(TextRange {
            unit: TextUnit::Utf8Bytes,
            start: 5,
            end: 4,
        }),
    ] {
        let mut value = cite(&table, SourceCoordinates::Unreported).value().clone();
        value.coordinates = coordinates;
        assert!(Annotation::new(value, &table).is_err());
    }
    assert!(
        TextRange {
            unit: TextUnit::Utf8Bytes,
            start: 1,
            end: 5
        }
        .validate("A😀B")
        .is_ok()
    );
    assert!(
        TextRange {
            unit: TextUnit::UnicodeScalars,
            start: 1,
            end: 5
        }
        .validate("A😀B")
        .is_err()
    );
}
#[test]
fn point_claims_use_text_boundaries_not_list_positions_and_clear_on_edits() {
    let table = sources("synthetic-document");
    let annotation = |unit, offset, ordinal| {
        Annotation::new(
            Citation {
                kind: CitationKind::Citation,
                claim: ClaimAnchor::Point(TextPoint { unit, offset }),
                source: ResourceId::new(1),
                coordinates: SourceCoordinates::Unreported,
                label: Some(text("document")),
                reported_ordinal: Some(ordinal),
            },
            &table,
        )
        .unwrap()
    };
    for (unit, offset, valid) in [
        (TextUnit::Utf8Bytes, 0, true),
        (TextUnit::Utf8Bytes, 2, true),
        (TextUnit::Utf8Bytes, 3, false),
        (TextUnit::Utf8Bytes, 6, true),
        (TextUnit::Utf8Bytes, 7, true),
        (TextUnit::Utf8Bytes, 8, false),
        (TextUnit::UnicodeScalars, 2, true),
        (TextUnit::UnicodeScalars, 3, true),
        (TextUnit::UnicodeScalars, 4, false),
    ] {
        let result = TextContent::new(
            text("é😀X"),
            vec![annotation(unit, offset, 99)],
            Presence::Absent,
        );
        assert_eq!(result.is_ok(), valid, "{unit:?} {offset}");
        if !valid {
            assert_eq!(result.unwrap_err(), GenerationError::InvalidCitation);
        }
    }
    let first = annotation(TextUnit::UnicodeScalars, 2, 0);
    let second = annotation(TextUnit::UnicodeScalars, 2, 7);
    let value = TextContent::new(
        text("é😀X"),
        vec![first.clone(), second.clone()],
        Presence::Absent,
    )
    .unwrap();
    assert_eq!(value.annotations(), &[first.clone(), second]);
    assert_eq!(value.clone().replace_text(text("é😀X")), value);
    assert!(value.replace_text(text("changed")).is_plain());
    assert_eq!(
        Annotation::new(
            Citation {
                claim: ClaimAnchor::Point(TextPoint {
                    unit: TextUnit::Utf8Bytes,
                    offset: MAX_TEXT_BYTES + 1,
                }),
                ..first.value().clone()
            },
            &table,
        )
        .unwrap_err(),
        GenerationError::Limit
    );
}

#[test]
fn unresolved_file_citation_units_reject_native_json_history_and_old_ir_shapes() {
    use morphiecore::protocol::{
        CodecError,
        fidelity::FidelityRecords,
        openai::{
            Dialect, GenerationApi, GenerationCodec, TargetContract, lowering::lower_response,
        },
    };
    use serde_json::json;
    let adapter = GenerationCodec::new(GenerationApi::Responses, Dialect::Standard, None);
    let annotation =
        json!({"type":"file_citation","file_id":"synthetic-file","filename":"","index":2});
    let mut wire = crate::wire::response(2);
    wire["output"][0]["content"][0]["annotations"] = json!([annotation.clone()]);
    let expected =
        CodecError::Unsupported(morphiecore::protocol::failure::CarrierReason::Unavailable);
    assert_eq!(
        adapter
            .decode_response(wire.to_string().as_bytes())
            .unwrap_err(),
        expected
    );
    let history = json!({"model":"m","input":[{"role":"assistant","content":[
        {"type":"output_text","text":"é😀X","annotations":[annotation]}]}]});
    assert_eq!(
        adapter
            .decode_request(history.to_string().as_bytes())
            .unwrap_err(),
        expected
    );
    let table = sources("synthetic-file");
    for claim in [
        ClaimAnchor::Unreported,
        ClaimAnchor::Point(TextPoint {
            unit: TextUnit::UnicodeScalars,
            offset: 2,
        }),
    ] {
        let value = Annotation::new(
            Citation {
                kind: CitationKind::Citation,
                claim,
                source: ResourceId::new(1),
                coordinates: SourceCoordinates::Unreported,
                label: Some(text("document")),
                reported_ordinal: Some(2),
            },
            &table,
        )
        .unwrap();
        let response = GenerationResponse::from_resources(
            vec![message(1, vec![value])],
            Outcome::Completed,
            table.clone(),
        )
        .unwrap();
        let metadata = adapter
            .decode_response(crate::wire::response(2).to_string().as_bytes())
            .unwrap()
            .metadata;
        assert_eq!(
            lower_response(
                &response,
                &FidelityRecords::default(),
                &metadata,
                GenerationApi::Responses,
                TargetContract::full(),
            )
            .err(),
            Some(morphiecore::protocol::conversion::generation::RepresentationError::TextMetadata)
        );
    }
}

#[test]
fn selection_keeps_cited_sources_and_rejects_a_changed_source() {
    let table = sources("synthetic-document");
    let response = GenerationResponse::from_resources(
        vec![message(
            1,
            vec![cite(&table, SourceCoordinates::Unreported)],
        )],
        Outcome::Completed,
        table
            .clone()
            .insert(
                ResourceId::new(2),
                ResourceLocation::OpaqueReference(text("unused")),
            )
            .unwrap(),
    )
    .unwrap();
    let request = ClientManaged::new(Default::default())
        .unwrap()
        .select_response(&response)
        .unwrap()
        .build()
        .unwrap();
    assert_eq!(request.resources(), &table);
    assert!(
        request
            .clone()
            .with_resources(sources("replacement"))
            .is_err()
    );
    assert!(request.with_resources(ResourceTable::default()).is_err());
}
#[test]
fn native_file_reports_preserve_list_ordinals_and_namespaced_identity_without_fake_claims() {
    use morphiecore::protocol::openai::{Dialect, GenerationApi, GenerationCodec, TargetContract};
    use serde_json::json;
    let adapter = GenerationCodec::new(GenerationApi::Responses, Dialect::Standard, None);
    let mut wire = crate::wire::response(2);
    let annotations = json!([
        {"type":"container_file_citation","container_id":"synthetic-container","file_id":"synthetic-file","filename":"report","start_index":0,"end_index":1},
        {"type":"file_path","file_id":"synthetic-artifact","index":7}
    ]);
    wire["output"][0]["content"][0]["annotations"] = annotations.clone();
    let decoded = adapter
        .decode_response(&serde_json::to_vec(&wire).unwrap())
        .unwrap();
    assert_eq!(decoded.semantic.resources().len(), 2);
    let Item::Message(m) = &decoded.semantic.items()[0].1 else {
        panic!()
    };
    let ContentPart::Text(t) = &m.parts[0].content else {
        panic!()
    };
    assert_eq!(t.annotations()[1].value().claim, ClaimAnchor::Unreported);
    assert_eq!(t.annotations()[1].value().reported_ordinal, Some(7));
    assert_eq!(
        t.annotations()[1].value().kind,
        CitationKind::ArtifactReference
    );
    assert!(
        matches!(decoded.semantic.resources().get(t.annotations()[0].value().source).unwrap(),
        ResourceLocation::NamespacedReference { namespace, id }
        if namespace.as_str()=="synthetic-container" && id.as_str()=="synthetic-file")
    );
    let encoded = adapter
        .encode_response(&decoded, &TargetContract::full())
        .unwrap();
    assert_eq!(
        encoded["output"][0]["content"][0]["annotations"],
        annotations
    );
    let request = ClientManaged::new(Default::default())
        .unwrap()
        .select_response(&decoded.semantic)
        .unwrap()
        .build()
        .unwrap();
    let needs = GenerationRequirements::derive(&request);
    assert_eq!(needs.file_inputs, 0);
    assert_eq!(needs.image_inputs, 0);
    assert!(needs.text_metadata);
}
