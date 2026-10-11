//! Shared generation representation failures and reported-fact policy.
use crate::semantic::task::generation::*;

/// Delivery policy for reported response facts (ADR 0008). Absence is a
/// reported fact; a target that demands the SDK-strict complete form fails
/// lowering instead of omitting the fact.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReportedFactPolicy {
    /// Absent facts stay absent and are never filled or synthesized.
    Faithful,
    /// The target requires complete reported facts; absence is a
    /// representability failure, not an omission.
    StrictComplete,
}
#[derive(Clone, Debug, Eq, thiserror::Error, PartialEq)]
pub enum RepresentationError {
    #[error("unknown projection rule")]
    UnknownProjectionRule,
    #[error("projection stage budget exceeded")]
    ProjectionLimit,
    #[error(transparent)]
    Admission(#[from] GenerationFeature),
    #[error(transparent)]
    Semantic(#[from] GenerationError),
    #[error(transparent)]
    Event(#[from] EventError),
    #[error("target cannot represent reported usage details")]
    UsageDetails,
    #[error("target requires complete reported facts")]
    ReportedFacts,
    #[error("target cannot represent text metadata")]
    TextMetadata,
    #[error("target has no reported interaction-progress carrier")]
    InteractionProgress,
    #[error("target cannot represent generation controls")]
    Controls,
    #[error("target cannot represent instructions")]
    Instructions,
    #[error("target cannot represent temperature")]
    Temperature,
    #[error("target cannot represent maximum output tokens")]
    MaxOutputTokens,
    #[error("target cannot represent tools")]
    Tools,
    #[error("target cannot represent parallel tools")]
    ParallelTools,
    #[error("target cannot represent strict tools")]
    StrictTools,
    #[error("omitted strict has a different default in the target profile")]
    StrictDefault,
    #[error("target cannot represent structured output")]
    StructuredOutput,
    #[error("target cannot represent reasoning")]
    Reasoning,
    #[error("target cannot represent this replay-value phase at item start")]
    ReplayPhase,
    #[error("target has no carrier for this replay format")]
    ReplayFormat,
    #[error("opaque replay requires a final token and a matching trusted origin")]
    ReplayOrigin,
    #[error("target cannot represent image input")]
    ImageInput,
    #[error("target cannot represent audio input")]
    AudioInput,
    #[error("target cannot represent file input")]
    FileInput,
    #[error("semantic domain has no migrated codec yet")]
    UnmigratedSemantic,
    #[error("message grouping cannot be preserved in this target")]
    MessageGrouping,
    #[error("target cannot represent the queued response lifecycle")]
    Lifecycle,
    #[error("terminal cannot be represented by the target profile")]
    Terminal,
    #[error("usage conversion is not part of this migration slice")]
    UsageProjection,
    #[error("invalid or conflicting response representation metadata")]
    Metadata,
    #[error("target has no system fingerprint carrier")]
    SystemFingerprint,
}
