//! Shared bounded readers for native usage breakdowns; native names stay here.
use super::CodecError;
use crate::protocol::failure::ShapeReason;
use crate::semantic::task::generation::*;
use serde_json::Value;

pub(crate) fn modality_tokens(
    value: &Value,
    bucket: UsageBucket,
    google: bool,
) -> Result<Vec<ModalityTokens>, CodecError> {
    let values = value
        .as_array()
        .ok_or(CodecError::Invalid(ShapeReason::Usage))?;
    if values.len() > 7 {
        return Err(CodecError::Limit);
    }
    values
        .iter()
        .map(|value| {
            let o = value
                .as_object()
                .ok_or(CodecError::Invalid(ShapeReason::Usage))?;
            let modality = if !google && o.get("modality").is_none_or(Value::is_null) {
                None
            } else {
                Some(match o.get("modality").and_then(Value::as_str) {
                    Some("TEXT") if google => TokenModality::Text,
                    Some("IMAGE") if google => TokenModality::Image,
                    Some("AUDIO") if google => TokenModality::Audio,
                    Some("VIDEO") if google => TokenModality::Video,
                    Some("DOCUMENT") if google => TokenModality::Document,
                    Some("MODALITY_UNSPECIFIED") if google => TokenModality::Unspecified,
                    None if google && !o.contains_key("modality") => TokenModality::Unspecified,
                    Some("text") if !google => TokenModality::Text,
                    Some("image") if !google => TokenModality::Image,
                    Some("audio") if !google => TokenModality::Audio,
                    Some("video") if !google => TokenModality::Video,
                    Some("document") if !google => TokenModality::Document,
                    _ => {
                        return Err(CodecError::Invalid(ShapeReason::Usage));
                    }
                })
            };
            let key = if google { "tokenCount" } else { "tokens" };
            let tokens = match o.get(key) {
                None if google => Some(0),
                None | Some(Value::Null) if !google => None,
                Some(Value::String(v)) if google => {
                    Some(v.parse().map_err(|_| CodecError::invalid())?)
                }
                Some(v) => Some(v.as_u64().ok_or(CodecError::invalid())?),
                None => {
                    return Err(CodecError::invalid());
                }
            };
            Ok(ModalityTokens {
                bucket,
                modality,
                tokens,
                defaulted: google && !o.contains_key(key),
            })
        })
        .collect()
}

pub(crate) fn grounding_counts(value: &Value) -> Result<Vec<ToolUseCount>, CodecError> {
    let values = value.as_array().ok_or(CodecError::invalid())?;
    if values.len() > 4 {
        return Err(CodecError::Limit);
    }
    values
        .iter()
        .map(|value| {
            let o = value.as_object().ok_or(CodecError::invalid())?;
            let kind = match o.get("type").and_then(Value::as_str) {
                None if o.get("type").is_none_or(Value::is_null) => ToolUsageKind::Unreported,
                Some("google_search") => ToolUsageKind::WebSearch,
                Some("google_maps") => ToolUsageKind::MapSearch,
                Some("retrieval") => ToolUsageKind::Retrieval,
                _ => {
                    return Err(CodecError::invalid());
                }
            };
            let count = match o.get("count") {
                None | Some(Value::Null) => None,
                Some(v) => Some(v.as_u64().ok_or(CodecError::invalid())?),
            };
            Ok(ToolUseCount { kind, count })
        })
        .collect()
}

pub(crate) fn report_loss(direction: &'static str) -> crate::lowering::generation::Effect {
    crate::lowering::generation::Effect::Field {
        rule: crate::lowering::generation::RuleId::new("generation.usage-report-view.v1"),
        field: ("generation.usage.reports").into(),
        direction: (direction).into(),
        action: ("project").into(),
        owner: crate::lowering::generation::Owner::Root,
    }
}
