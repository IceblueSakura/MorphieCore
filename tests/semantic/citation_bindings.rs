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
        ResourceDeclaration {
            body: ResourceBody::Text(text(s)),
            conditions: Default::default(),
        },
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
            role: MessageRole::Assistant,
            status: ItemLifecycle::Completed,
            phase: None,
            parts: vec![Part {
                id: PartId::new(id),
                replay: None,
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
        openai::{Profile, TargetCaps, lowering::lower_response},
    };
    let table = ResourceTable::new(vec![(
        ResourceId::new(1),
        ResourceDeclaration {
            body: ResourceBody::Media(ResourceLocation::OpaqueReference(text(
                "synthetic-document",
            ))),
            conditions: Default::default(),
        },
    )])
    .unwrap();
    let native = Citation {
        kind: CitationKind::Citation,
        claim: ClaimAnchor::Unreported,
        source: ResourceId::new(1),
        coordinates: SourceCoordinates::Unreported,
        label: Some(text("document")),
        reported_ordinal: Some(0),
    };
    let metadata =
        morphiecore::protocol::openai::envelope::decode_response(&crate::wire::response(2))
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
            Profile::Responses,
            TargetCaps::full()
        )
        .is_ok()
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
        assert_eq!(
            annotation.source_bounds(&table).unwrap(),
            SourceBounds::Unreported
        );
        let response = GenerationResponse::from_resources(
            vec![message(1, vec![annotation])],
            Outcome::Completed,
            table.clone(),
        )
        .unwrap();
        for profile in [Profile::Responses, Profile::Chat] {
            assert!(
                lower_response(
                    &response,
                    &FidelityRecords::default(),
                    &morphiecore::protocol::openai::envelope::decode_response(
                        &crate::wire::response(2)
                    )
                    .unwrap()
                    .metadata,
                    profile,
                    TargetCaps::full()
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
fn source_selection_edits_replay_cache_and_original_usage_share_one_dependency_owner() {
    use morphiecore::{
        protocol::fidelity::FidelityRecords,
        semantic::{
            cache::{CachePrefixContext, CachePrefixIntent, CachePrefixScope},
            context::ExecutionHints,
            value::ReplayOrigin,
        },
    };
    let table = sources("A😀B");
    let annotation = cite(
        &table,
        SourceCoordinates::Text(TextRange {
            unit: TextUnit::UnicodeScalars,
            start: 0,
            end: 3,
        }),
    );
    let mut owner = message(1, vec![annotation]);
    let Item::Message(m) = &mut owner.1 else {
        panic!()
    };
    m.parts[0].replay = Some(ReplayValue::final_value(
        ReplayFormat::GoogleGenerateContentPart,
        text("synthetic-token"),
    ));
    let unused = ResourceDeclaration {
        body: ResourceBody::Text(text("unused")),
        conditions: Default::default(),
    };
    let response = GenerationResponse::from_resources(
        vec![owner],
        Outcome::Completed,
        table
            .clone()
            .insert(ResourceId::new(2), unused.clone())
            .unwrap(),
    )
    .unwrap()
    .with_usage(Usage::operation(3, 5, 8))
    .unwrap();
    let request = ClientManaged::new(Default::default())
        .unwrap()
        .select_response(&response)
        .unwrap()
        .build(&[])
        .unwrap();
    assert_eq!(request.resources().len(), 1);
    let replay_owner = ReplayOwner::Part {
        item: ItemId::new(1),
        part: PartId::new(1),
    };
    let origin = ReplayOrigin::new("synthetic-source").unwrap();
    let mut fidelity = FidelityRecords::default();
    fidelity
        .record_attachment(replay_owner, &request, origin.clone())
        .unwrap();
    let dependency = RequestDependencyProof::capture(
        &request,
        HistoryDependency::Owners(vec![ItemId::new(1)]),
        SettingsDependency::All,
    )
    .unwrap();
    fidelity
        .bind_attachment_dependency(replay_owner, dependency, &request)
        .unwrap();
    let hints = ExecutionHints::default();
    let context = CachePrefixContext {
        model: "synthetic",
        hints: &hints,
        grouping: None,
        conversation: None,
    };
    let scope = CachePrefixScope::new("synthetic").unwrap();
    let cache = CachePrefixIntent::through(ItemId::new(1))
        .capture(&request, context, &scope)
        .unwrap();
    let unrelated = request
        .clone()
        .with_resources(table.clone().insert(ResourceId::new(2), unused).unwrap())
        .unwrap();
    assert!(cache.check(&unrelated, context, &scope).is_ok());
    assert!(fidelity.attachment_matches(replay_owner, &unrelated, Some(&origin)));
    assert!(
        request
            .clone()
            .with_resources(sources("replacement"))
            .is_err()
    );
    assert!(
        request
            .clone()
            .with_resources(ResourceTable::default())
            .is_err()
    );
    for next_table in [table.clone(), sources("replacement")] {
        let mut items = request.items().to_vec();
        let Item::Message(m) = &mut items[0].1 else {
            panic!()
        };
        let ContentPart::Text(t) = &m.parts[0].content else {
            panic!()
        };
        let mut value = t.annotations()[0].value().clone();
        value.label = Some(text("changed label"));
        m.parts[0].content = ContentPart::Text(
            TextContent::new(
                text(t.as_str()),
                vec![Annotation::new(value, &next_table).unwrap()],
                Presence::Absent,
            )
            .unwrap(),
        );
        let rebound =
            GenerationRequest::from_resources(items, request.settings().clone(), next_table)
                .unwrap();
        assert!(cache.check(&rebound, context, &scope).is_err());
        assert!(!fidelity.attachment_matches(replay_owner, &rebound, Some(&origin)));
    }
    let mut changed_conditions = table.get(ResourceId::new(1)).unwrap().clone();
    changed_conditions.conditions.expires_at = Some(99);
    assert!(
        request
            .clone()
            .with_resources(
                ResourceTable::new(vec![(ResourceId::new(1), changed_conditions)]).unwrap()
            )
            .is_err()
    );
    let mut items = response.items().to_vec();
    let Item::Message(m) = &mut items[0].1 else {
        panic!()
    };
    let ContentPart::Text(t) = &m.parts[0].content else {
        panic!()
    };
    m.parts[0].content = ContentPart::Text(t.clone().replace_text(text("edited claim")));
    let edited = response.clone().with_items(items).unwrap();
    assert_eq!(edited.usage_reports(), response.usage_reports());
    let branch = ClientManaged::new(Default::default())
        .unwrap()
        .select_response(&edited)
        .unwrap()
        .build(&[])
        .unwrap();
    assert!(branch.resources().is_empty());
    assert!(cache.check(&branch, context, &scope).is_err());
    assert!(!fidelity.attachment_matches(replay_owner, &branch, Some(&origin)));
    // A second branch changes configuration, not either citation anchor.
    let mut settings = request.settings().clone();
    settings.instructions = Presence::Value(text("branch configuration"));
    let configured = request
        .clone()
        .with_configuration(
            ConfigurationSnapshot::new(ConfigurationId::new(LocalScope::new(9), 2), settings)
                .unwrap(),
        )
        .unwrap();
    assert_eq!(configured.items(), request.items());
    assert_eq!(configured.resources(), request.resources());
    assert!(cache.check(&configured, context, &scope).is_err());
    assert!(!fidelity.attachment_matches(replay_owner, &configured, Some(&origin)));
    let recaptured = RequestDependencyProof::capture(
        &configured,
        HistoryDependency::Owners(vec![ItemId::new(1)]),
        SettingsDependency::All,
    )
    .unwrap();
    assert!(
        fidelity
            .bind_attachment_dependency(replay_owner, recaptured, &configured)
            .is_err()
    );
    assert!(fidelity.attachment_matches(replay_owner, &request, Some(&origin)));
    for candidate in [&request, &configured] {
        for profile in [
            morphiecore::protocol::openai::Profile::Chat,
            morphiecore::protocol::openai::Profile::Responses,
        ] {
            let mut contract = morphiecore::protocol::openai::TargetCaps::full();
            contract.replay_origin = Some(origin.clone());
            assert!(
                morphiecore::protocol::openai::lowering::lower_request(
                    candidate, &fidelity, profile, contract
                )
                .is_err()
            );
        }
    }
    assert_eq!(response.usage().unwrap().output_tokens, Some(5));
}
#[test]
fn native_file_reports_preserve_list_ordinals_and_namespaced_identity_without_fake_claims() {
    use morphiecore::protocol::openai::{Adapter, Dialect, Profile, TargetCaps};
    use serde_json::json;
    let adapter = Adapter::new(Profile::Responses, Dialect::Standard, None);
    let mut wire = crate::wire::response(2);
    let annotations = json!([
        {"type":"file_citation","file_id":"synthetic-file","filename":"","index":999},
        {"type":"container_file_citation","container_id":"synthetic-container","file_id":"synthetic-file","filename":"report","start_index":0,"end_index":1},
        {"type":"file_path","file_id":"synthetic-artifact","index":7}
    ]);
    wire["output"][0]["content"][0]["annotations"] = annotations.clone();
    let decoded = adapter
        .decode_response(&serde_json::to_vec(&wire).unwrap())
        .unwrap();
    assert_eq!(decoded.semantic.resources().len(), 3);
    let Item::Message(m) = &decoded.semantic.items()[0].1 else {
        panic!()
    };
    let ContentPart::Text(t) = &m.parts[0].content else {
        panic!()
    };
    assert_eq!(t.annotations()[0].value().claim, ClaimAnchor::Unreported);
    assert_eq!(t.annotations()[0].value().reported_ordinal, Some(999));
    assert_eq!(
        t.annotations()[0].value().label.as_ref().unwrap().as_str(),
        ""
    );
    assert_eq!(
        t.annotations()[2].value().kind,
        CitationKind::ArtifactReference
    );
    assert!(
        matches!(&decoded.semantic.resources().get(t.annotations()[1].value().source).unwrap().body,
        ResourceBody::Media(ResourceLocation::NamespacedReference { namespace, id })
        if namespace.as_str()=="synthetic-container" && id.as_str()=="synthetic-file")
    );
    let encoded = adapter
        .encode_response(&decoded, &TargetCaps::full())
        .unwrap();
    assert_eq!(
        encoded["output"][0]["content"][0]["annotations"],
        annotations
    );
    let request = ClientManaged::new(Default::default())
        .unwrap()
        .select_response(&decoded.semantic)
        .unwrap()
        .build(&[])
        .unwrap();
    let needs = GenerationRequirements::derive(&request);
    assert_eq!(needs.file_inputs, 0);
    assert_eq!(needs.image_inputs, 0);
    assert!(needs.text_metadata);
}
