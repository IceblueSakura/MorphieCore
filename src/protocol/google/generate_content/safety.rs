//! Probability filtering and reported assessments, not severity or inferred decisions.
//! https://ai.google.dev/api/generate-content#SafetySetting
use crate::{
    protocol::CodecError,
    semantic::{task::generation::*, value::Text},
};
use serde_json::{Map, Value, json};

fn string<'a>(o: &'a Map<String, Value>, key: &'static str) -> Result<Option<&'a str>, CodecError> {
    o.get(key)
        .map(|v| {
            v.as_str().ok_or(CodecError::Invalid(
                crate::protocol::failure::ShapeReason::InvalidValue,
            ))
        })
        .transpose()
}
fn category(value: &str) -> Result<SafetyCategory, CodecError> {
    Ok(match value {
        "HARM_CATEGORY_UNSPECIFIED" => SafetyCategory::Unspecified,
        "HARM_CATEGORY_HARASSMENT" => SafetyCategory::Harassment,
        "HARM_CATEGORY_HATE_SPEECH" => SafetyCategory::HateSpeech,
        "HARM_CATEGORY_SEXUALLY_EXPLICIT" => SafetyCategory::SexuallyExplicit,
        "HARM_CATEGORY_DANGEROUS_CONTENT" => SafetyCategory::DangerousContent,
        "HARM_CATEGORY_CIVIC_INTEGRITY" => SafetyCategory::CivicIntegrity,
        "HARM_CATEGORY_JAILBREAK" => SafetyCategory::Jailbreak,
        _ => {
            return Err(CodecError::Invalid(
                crate::protocol::failure::ShapeReason::InvalidValue,
            ));
        }
    })
}
pub(super) fn settings(values: &[SafetySetting]) -> Vec<Value> {
    values
        .iter()
        .map(|setting| {
            json!({
                "category":match setting.category {
                    SafetyCategory::Unspecified => "HARM_CATEGORY_UNSPECIFIED",
                    SafetyCategory::Harassment => "HARM_CATEGORY_HARASSMENT",
                    SafetyCategory::HateSpeech => "HARM_CATEGORY_HATE_SPEECH",
                    SafetyCategory::SexuallyExplicit => "HARM_CATEGORY_SEXUALLY_EXPLICIT",
                    SafetyCategory::DangerousContent => "HARM_CATEGORY_DANGEROUS_CONTENT",
                    SafetyCategory::CivicIntegrity => "HARM_CATEGORY_CIVIC_INTEGRITY",
                    SafetyCategory::Jailbreak => "HARM_CATEGORY_JAILBREAK",
                },
                "threshold":match setting.threshold {
                    SafetyThreshold::TargetDefault => "HARM_BLOCK_THRESHOLD_UNSPECIFIED",
                    SafetyThreshold::Disabled => "OFF",
                    SafetyThreshold::BlockNone => "BLOCK_NONE",
                    SafetyThreshold::LowAndAbove => "BLOCK_LOW_AND_ABOVE",
                    SafetyThreshold::MediumAndAbove => "BLOCK_MEDIUM_AND_ABOVE",
                    SafetyThreshold::HighOnly => "BLOCK_ONLY_HIGH",
                }
            })
        })
        .collect()
}
fn probability(value: &str) -> Result<SafetyProbability, CodecError> {
    Ok(match value {
        "HARM_PROBABILITY_UNSPECIFIED" => SafetyProbability::Unspecified,
        "NEGLIGIBLE" => SafetyProbability::Negligible,
        "LOW" => SafetyProbability::Low,
        "MEDIUM" => SafetyProbability::Medium,
        "HIGH" => SafetyProbability::High,
        _ => {
            return Err(CodecError::Invalid(
                crate::protocol::failure::ShapeReason::InvalidValue,
            ));
        }
    })
}
fn input_reason(value: &str) -> Result<SafetyBlockReason, CodecError> {
    Ok(match value {
        "BLOCK_REASON_UNSPECIFIED" | "BLOCKED_REASON_UNSPECIFIED" => SafetyBlockReason::Unspecified,
        "SAFETY" => SafetyBlockReason::Safety,
        "OTHER" => SafetyBlockReason::Other,
        "BLOCKLIST" => SafetyBlockReason::Blocklist,
        "PROHIBITED_CONTENT" => SafetyBlockReason::ProhibitedContent,
        "IMAGE_SAFETY" => SafetyBlockReason::ImageSafety,
        _ => {
            return Err(CodecError::Invalid(
                crate::protocol::failure::ShapeReason::InvalidValue,
            ));
        }
    })
}
fn output_reason(value: &str) -> Option<SafetyBlockReason> {
    Some(match value {
        "SAFETY" => SafetyBlockReason::Safety,
        "RECITATION" => SafetyBlockReason::Recitation,
        "BLOCKLIST" => SafetyBlockReason::Blocklist,
        "PROHIBITED_CONTENT" => SafetyBlockReason::ProhibitedContent,
        "SPII" => SafetyBlockReason::SensitivePersonalInformation,
        "IMAGE_SAFETY" => SafetyBlockReason::ImageSafety,
        "IMAGE_PROHIBITED_CONTENT" => SafetyBlockReason::ImageProhibitedContent,
        "IMAGE_RECITATION" => SafetyBlockReason::ImageRecitation,
        _ => return None,
    })
}
pub(super) fn report(
    o: &Map<String, Value>,
    subject: SafetySubject,
) -> Result<Option<SafetyReport>, CodecError> {
    let values = match o.get("safetyRatings") {
        None => &[][..],
        Some(v) => v
            .as_array()
            .ok_or(CodecError::Invalid(
                crate::protocol::failure::ShapeReason::InvalidValue,
            ))?
            .as_slice(),
    };
    if values.len() > MAX_ITEMS {
        return Err(CodecError::Limit);
    }
    let ratings = values
        .iter()
        .map(|v| {
            let rating = v.as_object().ok_or(CodecError::Invalid(
                crate::protocol::failure::ShapeReason::InvalidValue,
            ))?;
            Ok(SafetyRating {
                // Proto3 scalar defaults are source facts, not requested-policy inference.
                // https://github.com/googleapis/googleapis/blob/8cc2927c2ea64e6876568683050deabf2b52ce56/google/ai/generativelanguage/v1beta/safety.proto
                category: category(
                    string(rating, "category")?.unwrap_or("HARM_CATEGORY_UNSPECIFIED"),
                )?,
                probability: Some(probability(
                    string(rating, "probability")?.unwrap_or("HARM_PROBABILITY_UNSPECIFIED"),
                )?),
                blocked: Some(
                    rating
                        .get("blocked")
                        .map(|v| {
                            v.as_bool().ok_or(CodecError::Invalid(
                                crate::protocol::failure::ShapeReason::InvalidValue,
                            ))
                        })
                        .transpose()?
                        .unwrap_or(false),
                ),
            })
        })
        .collect::<Result<Vec<_>, CodecError>>()?;
    let (reason, message) = match subject {
        SafetySubject::Input => (
            string(o, "blockReason")?.map(input_reason).transpose()?,
            string(o, "blockReasonMessage")?
                .map(|v| {
                    Text::allowing_empty(v, "safety message", MAX_TEXT_BYTES)
                        .map_err(|_| CodecError::Limit)
                })
                .transpose()?,
        ),
        SafetySubject::Output => (string(o, "finishReason")?.and_then(output_reason), None),
    };
    // An empty native list reports no assessment, rather than an inferred safe result.
    if ratings.is_empty() && reason.is_none() && message.is_none() {
        return Ok(None);
    }
    let report = SafetyReport {
        subject,
        ratings,
        reason,
        message,
    };
    report.validate()?;
    Ok(Some(report))
}
