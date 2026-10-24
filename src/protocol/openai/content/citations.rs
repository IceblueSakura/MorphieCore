//! Native annotation syntax stays outside the semantic model.
//! https://github.com/openai/openai-python/blob/547ecb1e4fc233f19030736c29fee4374dfbaca1/src/openai/types/responses/response_output_text.py
use crate::protocol::openai::content::intake::Items;
use crate::protocol::openai::{CodecError, Profile, shape::*};
use crate::semantic::{task::generation::*, value::Text};
use serde::{Deserialize, Serialize};
use serde_json::Value;
#[derive(Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
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
fn known_fields(kind: &str) -> &'static [&'static str] {
    match kind {
        "url_citation" => &["type", "start_index", "end_index", "title", "url"],
        "file_citation" => &["type", "file_id", "filename", "index"],
        "container_file_citation" => &[
            "type",
            "container_id",
            "file_id",
            "filename",
            "start_index",
            "end_index",
        ],
        "file_path" => &["type", "file_id", "index"],
        _ => &[],
    }
}
pub(in crate::protocol::openai) fn read(
    value: &Value,
    items: &mut Items,
) -> Result<Annotation, CodecError> {
    let o = object(value)?;
    let kind = o.get("type").and_then(Value::as_str).unwrap_or_default();
    items.direction.fields(o, known_fields(kind))?;
    let wire: WireAnnotation = serde_json::from_value(value.clone())
        .map_err(|_| CodecError::Invalid(crate::protocol::failure::ShapeReason::InvalidValue))?;
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
        // The pinned source calls this a text insertion position, not a list ordinal.
        // Native units are unresolved; do not invent a point from the integer.
        WireAnnotation::FileCitation { .. } => {
            return Err(CodecError::Unsupported(
                crate::protocol::failure::CarrierReason::Unavailable,
            ));
        }
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
    let source = items.declare(location)?;
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
    let location = resources.get(value.source).ok_or(CodecError::Invalid(
        crate::protocol::failure::ShapeReason::InvalidValue,
    ))?;
    if value.coordinates != SourceCoordinates::Unreported || !context(location) {
        return Err(CodecError::Unsupported(
            crate::protocol::failure::CarrierReason::Unavailable,
        ));
    }
    let label = || {
        value
            .label
            .as_ref()
            .map(|v| v.as_str().to_owned())
            .ok_or(CodecError::Unsupported(
                crate::protocol::failure::CarrierReason::Unavailable,
            ))
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
            ResourceLocation::OpaqueReference(_),
            Some(_),
        )
        | (
            CitationKind::Citation,
            ClaimAnchor::Point(_),
            ResourceLocation::OpaqueReference(_),
            _,
        ) => Err(CodecError::Unsupported(
            crate::protocol::failure::CarrierReason::Unavailable,
        )),
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
        _ => Err(CodecError::Unsupported(
            crate::protocol::failure::CarrierReason::Unavailable,
        )),
    }
}
pub(in crate::protocol::openai) fn write(
    annotation: &Annotation,
    resources: &ResourceTable,
) -> Result<Value, CodecError> {
    serde_json::to_value(wire(annotation, resources)?)
        .map_err(|_| CodecError::Invalid(crate::protocol::failure::ShapeReason::InvalidValue))
}
pub(in crate::protocol::openai) fn check_snapshot(
    annotation: &Annotation,
    value: &Value,
    items: &Items,
) -> Result<(), CodecError> {
    let o = object(value)?;
    let kind = o.get("type").and_then(Value::as_str).unwrap_or_default();
    items.direction.fields(o, known_fields(kind))?;
    let reported: WireAnnotation = serde_json::from_value(value.clone())
        .map_err(|_| CodecError::Invalid(crate::protocol::failure::ShapeReason::InvalidValue))?;
    if matches!(reported, WireAnnotation::FileCitation { .. }) {
        return Err(CodecError::Unsupported(
            crate::protocol::failure::CarrierReason::Unavailable,
        ));
    }
    // Compare known facts, not raw extension keys, without redeclaring resources.
    if wire(annotation, &items.resources)? != reported {
        return Err(CodecError::Invalid(
            crate::protocol::failure::ShapeReason::SnapshotAnnotations,
        ));
    }
    Ok(())
}
pub(crate) fn check(
    annotation: &Annotation,
    resources: &ResourceTable,
    profile: Profile,
) -> Result<(), CodecError> {
    let value = wire(annotation, resources)?;
    if profile == Profile::Chat && !matches!(value, WireAnnotation::UrlCitation { .. }) {
        return Err(CodecError::Unsupported(
            crate::protocol::failure::CarrierReason::Unavailable,
        ));
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
                            return Err(CodecError::Unsupported(
                                crate::protocol::failure::CarrierReason::Unavailable,
                            ));
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
pub(crate) fn context(resource: &ResourceLocation) -> bool {
    !matches!(resource, ResourceLocation::Inline { .. })
}
