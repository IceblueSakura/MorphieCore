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
#[test]
fn source_bindings_and_output_edits_keep_units_and_unknown_extents_distinct() {
    let table = sources("A😀B");
    let bytes = TextRange {
        unit: TextUnit::Utf8Bytes,
        start: 1,
        end: 5,
    };
    let a = cite(&table, SourceCoordinates::Text(bytes));
    assert_eq!(a.source_bounds(&table).unwrap(), SourceBounds::Verified);
    assert!(a.check_source(&sources("other")).is_err());
    assert!(a.check_source(&ResourceTable::default()).is_err());
    let value = TextContent::new(text("claim"), vec![a.clone()], Presence::Absent).unwrap();
    assert!(
        value
            .replace_text(text("replacement"))
            .annotations()
            .is_empty()
    );
    assert!(
        Annotation::new(
            Citation {
                coordinates: SourceCoordinates::Text(TextRange { start: 2, ..bytes }),
                ..a.value().clone()
            },
            &table
        )
        .is_err()
    );
    let mut unknown = a.value().clone();
    unknown.claim = ClaimAnchor::Unreported;
    unknown.coordinates = SourceCoordinates::Unreported;
    unknown.reported_ordinal = Some(999);
    TextContent::new(
        text(""),
        vec![Annotation::new(unknown, &table).unwrap()],
        Presence::Absent,
    )
    .unwrap();
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
    use morphiecore::{
        lowering::generation::{GenerationRepresentationContract, lower_response},
        protocol::{fidelity::FidelityRecords, openai::Profile},
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
            GenerationRepresentationContract::full()
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
                    GenerationRepresentationContract::full()
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
fn citation_graph_budget_counts_edges_across_owners_without_multiplying_source_text() {
    let table = sources("shared source");
    let a = cite(&table, SourceCoordinates::Unreported);
    for count in [MAX_ITEMS - 1, MAX_ITEMS, MAX_ITEMS + 1] {
        let items = vec![
            message(1, vec![a.clone(); MAX_ITEMS / 2]),
            message(2, vec![a.clone(); count - MAX_ITEMS / 2]),
        ];
        let result = GenerationRequest::from_resources(items, Default::default(), table.clone());
        assert_eq!(result.is_ok(), count <= MAX_ITEMS, "{count}");
        let mut events = vec![
            StreamEvent::Started,
            StreamEvent::ResourceDeclared {
                id: ResourceId::new(1),
                resource: table.get(ResourceId::new(1)).unwrap().clone(),
            },
        ];
        for (owner, n) in [(1, MAX_ITEMS / 2), (2, count - MAX_ITEMS / 2)] {
            let item = ItemId::new(owner);
            let part = PartId::new(owner);
            events.extend([
                StreamEvent::ItemStarted {
                    item,
                    kind: ItemKind::Message { phase: None },
                    replay: None,
                },
                StreamEvent::PartStarted {
                    item,
                    part,
                    kind: PartKind::Text,
                },
                StreamEvent::Delta {
                    item,
                    part,
                    fragment: "claim".into(),
                    logprobs: vec![],
                },
                StreamEvent::ValueFinished { item, part },
                StreamEvent::TextMetadata {
                    item,
                    part,
                    annotations: vec![a.clone(); n],
                    logprobs: Presence::Absent,
                },
                StreamEvent::PartFinished { item, part },
                StreamEvent::ItemFinished {
                    item,
                    status: ItemLifecycle::Completed,
                    replay: None,
                },
            ]);
        }
        events.push(StreamEvent::Terminal {
            terminal: StreamTerminal::Completed,
            details: Default::default(),
        });
        let state = events.into_iter().try_fold(StreamState::default(), reduce);
        assert_eq!(state.is_ok(), count <= MAX_ITEMS);
        if let Ok(state) = state {
            assert_eq!(materialize(&state).unwrap().resources(), &table);
        }
    }
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
            let mut contract =
                morphiecore::lowering::generation::GenerationRepresentationContract::full();
            contract.replay_origin = Some(origin.clone());
            assert!(
                morphiecore::lowering::generation::lower_request(
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
    use morphiecore::{
        adapter::{Adapter, Dialect},
        lowering::generation::GenerationRepresentationContract,
        protocol::openai::Profile,
    };
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
        .encode_response(&decoded, &GenerationRepresentationContract::full())
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
#[test]
fn interleaved_source_declarations_and_early_annotations_match_independent_wire_snapshots() {
    use morphiecore::protocol::openai::{
        Profile,
        events::{EventDecoder, EventEncoder},
    };
    use serde_json::json;
    let base = crate::wire::events(2);
    let find = |name: &str| base.iter().find(|v| v["type"] == name).unwrap().clone();
    let mut parts = vec![
        crate::wire::output_text("A😀B"),
        crate::wire::output_text("A😀B"),
    ];
    for (index, part) in parts.iter_mut().enumerate() {
        part["annotations"][0]["url"] = json!(format!("https://example.test/{index}"));
        part["annotations"][0]["end_index"] = json!(3);
    }
    let mut frames = vec![find("response.created"), find("response.output_item.added")];
    for index in 0..2 {
        let mut added = find("response.content_part.added");
        added["content_index"] = json!(index);
        frames.push(added);
    }
    // Opposite arrival order, before text: source IDs differ from a static parse.
    for index in [1, 0] {
        let mut added = find("response.output_text.annotation.added");
        added["content_index"] = json!(index);
        added["annotation"] = parts[index]["annotations"][0].clone();
        frames.push(added);
    }
    for (index, part) in parts.iter().enumerate() {
        let mut delta = find("response.output_text.delta");
        delta["content_index"] = json!(index);
        delta["delta"] = part["text"].clone();
        delta["logprobs"] = json!([]);
        frames.push(delta);
        let mut done = find("response.output_text.done");
        done["content_index"] = json!(index);
        done["text"] = part["text"].clone();
        done["logprobs"] = json!([]);
        frames.push(done);
        let mut closed = find("response.content_part.done");
        closed["content_index"] = json!(index);
        closed["part"] = part.clone();
        frames.push(closed);
    }
    let mut item = find("response.output_item.done");
    item["item"]["content"] = json!(parts);
    frames.push(item.clone());
    let mut terminal = find("response.completed");
    terminal["response"]["output"] = json!([item["item"]]);
    frames.push(terminal.clone());
    for (index, frame) in frames.iter_mut().enumerate() {
        frame["sequence_number"] = json!(index);
    }
    let mut decoder = EventDecoder::new(Profile::Responses);
    let mut state = StreamState::default();
    let mut events = vec![];
    for frame in &frames {
        for event in decoder.push(frame).unwrap() {
            state = reduce(state, event.clone()).unwrap();
            events.push(event);
        }
    }
    decoder.finish().unwrap();
    let response = materialize(&state).unwrap();
    let Item::Message(m) = &response.items()[0].1 else {
        panic!()
    };
    let ContentPart::Text(t) = &m.parts[0].content else {
        panic!()
    };
    assert_eq!(t.annotations()[0].value().source, ResourceId::new(2));
    let mut encoder =
        EventEncoder::new(Profile::Responses, decoder.metadata().unwrap().clone()).unwrap();
    let mut encoded = vec![];
    for event in &events {
        encoded.extend(encoder.encode(event, decoder.fidelity()).unwrap());
    }
    encoder.finish().unwrap();
    assert_eq!(
        encoded.last().unwrap()["response"]["output"],
        terminal["response"]["output"]
    );
    // Provider facts must bind to the event-owned values after wire-equivalent local ID allocation.
    let adapter = morphiecore::adapter::Adapter::new(
        Profile::Responses,
        morphiecore::adapter::Dialect::Bailian,
        Some(morphiecore::semantic::value::ReplayOrigin::new("synthetic-citations").unwrap()),
    );
    let mut provider_decoder = adapter.event_decoder();
    let mut provider_state = StreamState::default();
    for mut frame in frames.clone() {
        if let Some(response) = frame.get_mut("response") {
            response["frequency_penalty"] = json!(0.25);
        }
        for event in provider_decoder.push(&frame).unwrap() {
            provider_state = reduce(provider_state, event).unwrap();
        }
    }
    provider_decoder.finish().unwrap();
    let observed = morphiecore::protocol::DecodedResponse {
        semantic: materialize(&provider_state).unwrap(),
        metadata: provider_decoder.metadata().unwrap().clone(),
        fidelity: provider_decoder.fidelity().clone(),
    };
    let projected = adapter
        .encode_response(
            &observed,
            &morphiecore::lowering::generation::GenerationRepresentationContract::full(),
        )
        .unwrap();
    assert_eq!(projected["frequency_penalty"], json!(0.25));
    let at = events
        .iter()
        .position(|e| matches!(e, StreamEvent::ResourceDeclared { .. }))
        .unwrap();
    let missing = events
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != at)
        .try_fold(StreamState::default(), |s, (_, e)| reduce(s, e.clone()));
    assert!(missing.is_err());
    let late = events
        .iter()
        .find(|e| matches!(e, StreamEvent::AnnotationAdded { .. }))
        .unwrap();
    assert!(reduce(state, late.clone()).is_err());
}
