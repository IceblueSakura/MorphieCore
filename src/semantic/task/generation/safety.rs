//! Requested probability filtering and independent reported assessments.
use crate::semantic::task::generation::{
    GenerationError, MAX_ITEMS, MAX_TEXT_BYTES, MAX_TOTAL_BYTES,
};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum SafetyCategory {
    Unspecified,
    Harassment,
    HateSpeech,
    SexuallyExplicit,
    DangerousContent,
    CivicIntegrity,
    Jailbreak,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SafetyThreshold {
    TargetDefault,
    Disabled,
    BlockNone,
    LowAndAbove,
    MediumAndAbove,
    HighOnly,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SafetySetting {
    pub category: SafetyCategory,
    pub threshold: SafetyThreshold,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SafetySubject {
    Input,
    Output,
}
/// Ordinal probability reports, never severity levels or calibrated numeric scores.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SafetyProbability {
    Unspecified,
    Negligible,
    Low,
    Medium,
    High,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SafetyRating {
    pub category: SafetyCategory,
    pub probability: Option<SafetyProbability>,
    pub blocked: Option<bool>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SafetyBlockReason {
    Unspecified,
    Safety,
    Other,
    Blocklist,
    ProhibitedContent,
    ImageSafety,
    Recitation,
    SensitivePersonalInformation,
    ImageProhibitedContent,
    ImageRecitation,
}
/// One observed assessment. Its exact content span is not inferred.
#[derive(Clone, Eq, PartialEq)]
pub struct SafetyReport {
    pub subject: SafetySubject,
    pub ratings: Vec<SafetyRating>,
    pub reason: Option<SafetyBlockReason>,
    pub message: Option<crate::semantic::value::Text>,
}
impl std::fmt::Debug for SafetyReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SafetyReport([redacted])")
    }
}
impl SafetyReport {
    pub fn validate(&self) -> Result<usize, GenerationError> {
        if self.ratings.len() > MAX_ITEMS {
            return Err(GenerationError::Limit);
        }
        let mut categories = std::collections::BTreeSet::new();
        if self
            .ratings
            .iter()
            .any(|rating| !categories.insert(rating.category))
        {
            return Err(GenerationError::InvalidResponse);
        }
        let message = self
            .message
            .as_ref()
            .map_or(0, |message| message.as_str().len());
        if message > MAX_TEXT_BYTES {
            return Err(GenerationError::Limit);
        }
        Ok(std::mem::size_of::<Self>() + std::mem::size_of_val(self.ratings.as_slice()) + message)
    }
    /// No block reason or message, and every rating explicitly reports no block.
    /// An unreported block flag is not evidence of an unblocked rating.
    pub fn is_assessment_only(&self) -> bool {
        self.reason.is_none()
            && self.message.is_none()
            && self
                .ratings
                .iter()
                .all(|rating| rating.blocked == Some(false))
    }
}
pub(super) fn settings_bytes(settings: &[SafetySetting]) -> Result<usize, GenerationError> {
    if settings.len() > MAX_ITEMS {
        return Err(GenerationError::Limit);
    }
    let mut categories = std::collections::BTreeSet::new();
    if settings.iter().any(|setting| {
        setting.category == SafetyCategory::Unspecified || !categories.insert(setting.category)
    }) {
        return Err(GenerationError::InvalidControl);
    }
    Ok(std::mem::size_of_val(settings))
}
pub(super) fn reports_bytes(reports: &[SafetyReport]) -> Result<usize, GenerationError> {
    if reports.len() > MAX_ITEMS {
        return Err(GenerationError::Limit);
    }
    reports.iter().try_fold(0usize, |bytes, report| {
        bytes
            .checked_add(report.validate()?)
            .filter(|bytes| *bytes <= MAX_TOTAL_BYTES)
            .ok_or(GenerationError::Limit)
    })
}
