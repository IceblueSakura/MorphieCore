//! Declared dialects and supplied reference graphs; no resolver I/O or schema rewriting.
use morphiecore::{
    lowering::generation::{GenerationRepresentationContract, lower_request},
    protocol::{fidelity::FidelityRecords, openai::Profile},
    semantic::{
        task::generation::*,
        value::{Presence, Text},
    },
};
use serde_json::json;
fn text(v: &str) -> Text {
    Text::new(v, "synthetic", MAX_TEXT_BYTES).unwrap()
}
fn resource(id: &str, value: serde_json::Value) -> SchemaResource {
    SchemaResource {
        id: text(id),
        value,
    }
}
#[test]
fn supplied_external_and_local_recursive_references_preserve_one_authority() {
    let root = json!({"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object",
        "properties":{"node":{"$ref":"https://schema.invalid/node#/$defs/node"}}});
    let external = json!({"$defs":{"node":{"type":"object","properties":{
        "next":{"$ref":"#/$defs/node"},"value":{"type":"integer"}}}}});
    let schema = SchemaDocument::new(
        SchemaDialect::Draft202012,
        root.clone(),
        vec![resource("https://schema.invalid/node", external.clone())],
    )
    .unwrap();
    assert_eq!(schema.root(), &root);
    assert_eq!(schema.resources()[0].value, external);
    let request = GenerationRequest::from_settings(
        vec![],
        GenerationSettings {
            instructions: Presence::Value(text("hello")),
            text: TextOptions {
                presence: true,
                format: Presence::Value(OutputConstraint::JsonSchema {
                    name: text("answer"),
                    description: None,
                    schema,
                    strict: Some(false),
                }),
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .unwrap();
    for profile in [Profile::Responses, Profile::Chat] {
        assert!(
            lower_request(
                &request,
                &FidelityRecords::default(),
                profile,
                GenerationRepresentationContract::full()
            )
            .is_err()
        );
    }
}
#[test]
fn dialect_declarations_reference_positions_and_duplicate_resources_fail_closed() {
    let dialect = SchemaDialect::Draft202012;
    for (root, resources) in [
        (
            json!({"$ref":"https://not-provided.invalid/schema"}),
            vec![],
        ),
        (
            json!({"$schema":"https://json-schema.org/draft/2019-09/schema","type":"string"}),
            vec![],
        ),
        (
            json!({"$id":"https://rebase.invalid/","type":"string"}),
            vec![],
        ),
        (json!({"$ref":"#/$defs/missing"}), vec![]),
        (
            json!({"default":{"type":"string"},"$ref":"#/default"}),
            vec![],
        ),
        (
            json!({"$ref":"urn:synthetic:one"}),
            vec![
                resource("urn:synthetic:one", json!({"type":"string"})),
                resource("urn:synthetic:one", json!({"type":"integer"})),
            ],
        ),
    ] {
        assert!(SchemaDocument::new(dialect, root, resources).is_err());
    }
    SchemaDocument::new(dialect, json!(true), vec![]).unwrap();
    assert!(SchemaDocument::new(SchemaDialect::Unspecified, json!(true), vec![]).is_err());
    assert!(
        SchemaDocument::new(
            SchemaDialect::Unspecified,
            json!({"$schema":"https://json-schema.org/draft/2020-12/schema","type":"string"}),
            vec![]
        )
        .is_err()
    );
}
#[test]
fn physical_reference_and_resource_budgets_are_independent_of_data_depth() {
    for count in [
        MAX_SCHEMA_REFERENCES - 1,
        MAX_SCHEMA_REFERENCES,
        MAX_SCHEMA_REFERENCES + 1,
    ] {
        let properties: serde_json::Map<String, serde_json::Value> = (0..count)
            .map(|n| (format!("p{n}"), json!({"$ref":"#"})))
            .collect();
        let result = SchemaDocument::new(
            SchemaDialect::Draft202012,
            json!({"type":"object","properties":properties}),
            vec![],
        );
        assert_eq!(result.is_ok(), count <= MAX_SCHEMA_REFERENCES);
    }
    for count in [
        MAX_SCHEMA_RESOURCES - 1,
        MAX_SCHEMA_RESOURCES,
        MAX_SCHEMA_RESOURCES + 1,
    ] {
        let resources = (0..count)
            .map(|n| {
                resource(
                    &format!("urn:synthetic:{n}"),
                    json!({"$ref":format!("urn:synthetic:{}", (n+1)%count)}),
                )
            })
            .collect();
        let result = SchemaDocument::new(
            SchemaDialect::Draft202012,
            json!({"type":"string"}),
            resources,
        );
        assert_eq!(result.is_ok(), count <= MAX_SCHEMA_RESOURCES);
    }
    let mut deep = json!(0);
    for _ in 0..65 {
        deep = json!([deep]);
    }
    assert_eq!(
        SchemaDocument::new(SchemaDialect::Draft202012, json!({"default":deep}), vec![])
            .unwrap_err(),
        GenerationError::Limit
    );
}
#[test]
fn deleting_a_supplied_definition_invalidates_its_ref_without_a_cached_body() {
    let mut schema = SchemaDocument::new(
        SchemaDialect::Draft202012,
        json!({"$defs":{"a":{"type":"string"}},"$ref":"#/$defs/a"}),
        vec![],
    )
    .unwrap();
    schema
        .root_mut()
        .as_object_mut()
        .unwrap()
        .shift_remove("$defs");
    assert_eq!(schema.validate(), Err(GenerationError::InvalidSchema));
}

#[test]
fn function_parameter_and_output_dialects_cannot_be_erased_by_echo_or_event_codecs() {
    use morphiecore::{
        lowering::generation::lower_response, protocol::openai::events::EventEncoder,
    };
    for output in [false, true] {
        let schema =
            SchemaDocument::new(SchemaDialect::Draft202012, json!({"type":"object"}), vec![])
                .unwrap();
        let settings = GenerationSettings {
            instructions: Presence::Value(text("hello")),
            tools: Some(vec![ToolDefinition::Function(FunctionTool {
                name: text("lookup"),
                description: None,
                parameters: (!output).then(|| schema.clone()),
                output_schema: output.then_some(schema),
                strict: FunctionStrictness::Explicit(false),
                dispatch: Default::default(),
            })]),
            ..Default::default()
        };
        let request = GenerationRequest::from_settings(vec![], settings.clone()).unwrap();
        let fidelity = FidelityRecords::default();
        let mut metadata = crate::events_support::metadata();
        metadata.context.settings = Some(settings);
        let response = GenerationResponse::new(vec![], Outcome::Completed).unwrap();
        assert!(
            lower_request(
                &request,
                &fidelity,
                Profile::Responses,
                GenerationRepresentationContract::full()
            )
            .is_err()
        );
        assert!(
            lower_response(
                &response,
                &fidelity,
                &metadata,
                Profile::Responses,
                GenerationRepresentationContract::full()
            )
            .is_err()
        );
        let mut encoder = EventEncoder::new(Profile::Responses, metadata).unwrap();
        assert!(encoder.encode(&StreamEvent::Started, &fidelity).is_err());
    }
}

#[test]
fn supplied_documents_share_an_exact_aggregate_byte_budget() {
    for delta in [-1isize, 0, 1] {
        // Root {"type":"string"} is 17 bytes; each resource has an 18-byte
        // {"description":""} shell and a 15-byte urn:synthetic:N identifier.
        let resources = (0..4)
            .map(|n| {
                let padding =
                    (MAX_TEXT_BYTES as isize - 33 + if n == 3 { delta - 17 } else { 0 }) as usize;
                resource(
                    &format!("urn:synthetic:{n}"),
                    json!({"description":"x".repeat(padding)}),
                )
            })
            .collect();
        let schema = SchemaDocument::new(
            SchemaDialect::Draft202012,
            json!({"type":"string"}),
            resources,
        );
        if delta > 0 {
            assert_eq!(schema.unwrap_err(), GenerationError::Limit);
        } else {
            assert_eq!(
                schema.unwrap().validate().unwrap(),
                (MAX_TOTAL_BYTES as isize + delta) as usize
            );
        }
    }
}
