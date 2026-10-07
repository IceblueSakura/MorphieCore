//! Native annotation syntax stays outside the semantic model.
//! https://github.com/openai/openai-python/blob/be9d66628ad7377bd36fe5a76ae6d735843f0e76/src/openai/types/responses/response_output_text.py
use super::{CodecError, Profile, common::*};
use crate::semantic::{task::generation::*, value::Text};
use serde::{Deserialize, Serialize};
use serde_json::Value;
#[derive(Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum WireAnnotation {
    UrlCitation {
        start_index: usize,
        end_index: usize,
        title: String,
        url: String,
    },
    FileCitation {
        file_id: String,
        filename: String,
        index: usize,
    },
    ContainerFileCitation {
        container_id: String,
        file_id: String,
        filename: String,
        start_index: usize,
        end_index: usize,
    },
    FilePath {
        file_id: String,
        index: usize,
    },
}
fn label(value: String) -> Result<Text, CodecError> {
    Text::allowing_empty(value, "citation label", MAX_TEXT_BYTES).map_err(|_| CodecError::Limit)
}
fn span(start: usize, end: usize) -> ClaimAnchor {
    ClaimAnchor::Range(TextRange {
        unit: TextUnit::UnicodeScalars,
        start,
        end,
    })
}
pub(super) fn read(value: &Value, items: &mut Items) -> Result<Annotation, CodecError> {
    let wire: WireAnnotation =
        serde_json::from_value(value.clone()).map_err(|_| CodecError::Invalid("annotation"))?;
    let (location, claim, name, ordinal, kind) = match wire {
        WireAnnotation::UrlCitation {
            start_index,
            end_index,
            title,
            url,
        } => (
            ResourceLocation::Url(text(&url, "citation URL", MAX_RESOURCE_URL_BYTES)?),
            span(start_index, end_index),
            Some(label(title)?),
            None,
            CitationKind::Citation,
        ),
        WireAnnotation::FileCitation {
            file_id,
            filename,
            index,
        } => (
            ResourceLocation::OpaqueReference(text(&file_id, "citation file", 256)?),
            ClaimAnchor::Unreported,
            Some(label(filename)?),
            Some(index),
            CitationKind::Citation,
        ),
        WireAnnotation::ContainerFileCitation {
            container_id,
            file_id,
            filename,
            start_index,
            end_index,
        } => (
            ResourceLocation::NamespacedReference {
                namespace: text(&container_id, "citation namespace", 256)?,
                id: text(&file_id, "citation file", 256)?,
            },
            span(start_index, end_index),
            Some(label(filename)?),
            None,
            CitationKind::Citation,
        ),
        WireAnnotation::FilePath { file_id, index } => (
            ResourceLocation::OpaqueReference(text(&file_id, "file reference", 256)?),
            ClaimAnchor::Unreported,
            None,
            Some(index),
            CitationKind::ArtifactReference,
        ),
    };
    let source = items.declare(ResourceDeclaration {
        body: ResourceBody::Media(location),
        conditions: Default::default(),
    })?;
    Ok(Annotation::new(
        Citation {
            kind,
            claim,
            source,
            coordinates: SourceCoordinates::Unreported,
            label: name,
            reported_ordinal: ordinal,
        },
        &items.resources,
    )?)
}
fn wire(annotation: &Annotation, resources: &ResourceTable) -> Result<WireAnnotation, CodecError> {
    annotation.check_source(resources)?;
    let value = annotation.value();
    let declaration = resources
        .get(value.source)
        .ok_or(CodecError::Invalid("citation source"))?;
    if value.coordinates != SourceCoordinates::Unreported || !context(declaration) {
        return Err(CodecError::Unsupported(
            "citation source coordinates or conditions".into(),
        ));
    }
    let ResourceBody::Media(location) = &declaration.body else {
        return Err(CodecError::Unsupported("citation source carrier".into()));
    };
    let label = || {
        value
            .label
            .as_ref()
            .map(|v| v.as_str().to_owned())
            .ok_or(CodecError::Unsupported("citation label".into()))
    };
    match (value.kind, value.claim, location, value.reported_ordinal) {
        (
            CitationKind::Citation,
            ClaimAnchor::Range(TextRange {
                unit: TextUnit::UnicodeScalars,
                start,
                end,
            }),
            ResourceLocation::Url(url),
            None,
        ) => Ok(WireAnnotation::UrlCitation {
            start_index: start,
            end_index: end,
            title: label()?,
            url: url.as_str().into(),
        }),
        (
            CitationKind::Citation,
            ClaimAnchor::Unreported,
            ResourceLocation::OpaqueReference(file),
            Some(index),
        ) => Ok(WireAnnotation::FileCitation {
            file_id: file.as_str().into(),
            filename: label()?,
            index,
        }),
        (
            CitationKind::Citation,
            ClaimAnchor::Range(TextRange {
                unit: TextUnit::UnicodeScalars,
                start,
                end,
            }),
            ResourceLocation::NamespacedReference { namespace, id },
            None,
        ) => Ok(WireAnnotation::ContainerFileCitation {
            container_id: namespace.as_str().into(),
            file_id: id.as_str().into(),
            filename: label()?,
            start_index: start,
            end_index: end,
        }),
        (
            CitationKind::ArtifactReference,
            ClaimAnchor::Unreported,
            ResourceLocation::OpaqueReference(file),
            Some(index),
        ) if value.label.is_none() => Ok(WireAnnotation::FilePath {
            file_id: file.as_str().into(),
            index,
        }),
        _ => Err(CodecError::Unsupported("citation carrier".into())),
    }
}
pub(super) fn write(
    annotation: &Annotation,
    resources: &ResourceTable,
) -> Result<Value, CodecError> {
    serde_json::to_value(wire(annotation, resources)?)
        .map_err(|_| CodecError::Invalid("annotation"))
}
pub(crate) fn check(
    annotation: &Annotation,
    resources: &ResourceTable,
    profile: Profile,
) -> Result<(), CodecError> {
    let value = wire(annotation, resources)?;
    if profile == Profile::Chat && !matches!(value, WireAnnotation::UrlCitation { .. }) {
        return Err(CodecError::Unsupported("Chat citation".into()));
    }
    Ok(())
}
pub(crate) fn check_items(
    items: &[(ItemId, Item)],
    resources: &ResourceTable,
    profile: Profile,
) -> Result<(), CodecError> {
    for (_, item) in items {
        if let Item::Message(message) = item {
            for part in &message.parts {
                if let ContentPart::Text(text) = &part.content {
                    for annotation in text.annotations() {
                        if message.role != MessageRole::Assistant {
                            return Err(CodecError::Unsupported("citation owner".into()));
                        }
                        check(annotation, resources, profile)?;
                    }
                }
            }
        }
    }
    Ok(())
}
/// Inert context only: never a new public event or media-output admission.
pub(crate) fn context(resource: &ResourceDeclaration) -> bool {
    resource.conditions == ResourceConditions::default()
        && matches!(
            resource.body,
            ResourceBody::Media(
                ResourceLocation::Url(_)
                    | ResourceLocation::OpaqueReference(_)
                    | ResourceLocation::NamespacedReference { .. }
            )
        )
}
