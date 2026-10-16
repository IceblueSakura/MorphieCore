//! Shared generation representation failures and reported-fact policy.
use crate::semantic::task::generation::*;
use crate::semantic::value::Presence;

/// Explicitly allowlisted controls only; no instructions, schemas or identities.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReasoningControls {
    pub configuration_efforts: Vec<EffortControl>,
    pub activation: Presence<ReasoningActivation>,
    pub effort: Presence<EffortControl>,
    pub budget_tokens: Option<u64>,
    pub max_output_tokens: Option<u64>,
    pub mode: Presence<ReasoningMode>,
    pub summary: Presence<ReasoningSummary>,
    pub display: Presence<ReasoningDisplay>,
}
impl From<&GenerationSettings> for ReasoningControls {
    fn from(settings: &GenerationSettings) -> Self {
        Self {
            configuration_efforts: vec![],
            activation: settings.reasoning.activation.clone(),
            effort: settings.effort.clone(),
            budget_tokens: settings.reasoning.budget_tokens,
            max_output_tokens: settings.controls.max_output_tokens,
            mode: settings.reasoning.mode.clone(),
            summary: settings.reasoning.summary.clone(),
            display: settings.reasoning.display.clone(),
        }
    }
}
impl ReasoningControls {
    pub fn request(request: &GenerationRequest) -> Self {
        let mut controls: Self = request.settings().into();
        for (_, item) in request.items() {
            if let Item::ConfigurationUpdate(update) = item
                && let Some(effort) = update.effort
                && !controls.configuration_efforts.contains(&effort)
            {
                controls.configuration_efforts.push(effort);
            }
        }
        controls
    }
}
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct LossEvidence {
    pub rule: &'static str,
    pub field: &'static str,
    pub direction: &'static str,
    pub action: &'static str,
}
/// Request-side control evidence contains finite enums and presence, never prompt data.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RequestControlLoss {
    Projection(LossEvidence),
    Verbosity {
        rule: &'static str,
        input: Presence<Verbosity>,
        output: Presence<Verbosity>,
    },
    Reasoning {
        rule: &'static str,
        input: ReasoningControls,
        output: ReasoningControls,
        preset_tokens: Option<u64>,
    },
    Summary {
        rule: &'static str,
        input: Presence<ReasoningSummary>,
        output: ReasoningDisplay,
    },
    Role {
        rule: &'static str,
        input: InstructionAuthority,
        output: InstructionAuthority,
    },
    Sampling {
        rule: &'static str,
        temperature: bool,
        top_p: bool,
    },
    MessagesVerbosity {
        input: Presence<Verbosity>,
        output: Presence<Verbosity>,
    },
}
impl RequestControlLoss {
    pub const fn rule(&self) -> &'static str {
        match self {
            Self::Projection(evidence) => evidence.rule,
            Self::Reasoning { rule, .. }
            | Self::Summary { rule, .. }
            | Self::Role { rule, .. }
            | Self::Sampling { rule, .. }
            | Self::Verbosity { rule, .. } => rule,
            Self::MessagesVerbosity { .. } => "messages.omit-verbosity.v1",
        }
    }
    pub const fn field(&self) -> &'static str {
        match self {
            Self::Projection(evidence) => evidence.field,
            Self::Reasoning { input, output, .. } => {
                if input.budget_tokens.is_some() || output.budget_tokens.is_some() {
                    "generation.reasoning.allocation"
                } else {
                    "generation.effort"
                }
            }
            Self::Summary { .. } => "generation.reasoning.summary",
            Self::Role { .. } => "generation.instructions.authority",
            Self::Sampling { .. } => "generation.sampling",
            Self::MessagesVerbosity { .. } | Self::Verbosity { .. } => "generation.text.verbosity",
        }
    }
    pub const fn direction(&self) -> &'static str {
        match self {
            Self::Projection(evidence) => evidence.direction,
            _ => "request.encode",
        }
    }
    pub fn action(&self) -> &'static str {
        match self {
            Self::Projection(evidence) => evidence.action,
            Self::Reasoning { input, output, .. }
                if !input.effort.is_absent()
                    && output.effort.is_absent()
                    && input.budget_tokens == output.budget_tokens =>
            {
                "omit"
            }
            Self::MessagesVerbosity { .. } | Self::Verbosity { .. } | Self::Sampling { .. } => {
                "omit"
            }
            _ => "approximate",
        }
    }
}

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
