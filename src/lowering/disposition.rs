//! Target-owned request dispositions. No routing, credentials or I/O.
use crate::{
    protocol::conversion::generation::{Change, ConversionBoundary, Effect, Field},
    semantic::task::generation::{
        Feature, GenerationRequest, GenerationResponse, GenerationResponseRequirements,
    },
};
use strum::{EnumCount, IntoEnumIterator};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Rule {
    pub id: &'static str,
    pub field: Field,
    pub change: Change,
}
impl Rule {
    pub const fn new(id: &'static str, field: Field, change: Change) -> Self {
        Self { id, field, change }
    }
    pub fn effect(self, boundary: ConversionBoundary) -> Effect {
        Effect::new(self.id, self.field, boundary, self.change)
    }
}
impl Rule {
    pub const fn name(self) -> &'static str {
        self.id
    }
    pub const OMIT_PREDICTION: Self = Self::new(
        "generation.omit-prediction.v1",
        Field::Prediction,
        Change::Omit,
    );
    pub const OMIT_OPENING_REPORT: Self = Self::new(
        "responses.terminal-report-snapshot.v1",
        Field::OpeningReport,
        Change::Omit,
    );
    pub const USAGE_REPORT_VIEW: Self = Self::new(
        "generation.usage-report-view.v1",
        Field::UsageReports,
        Change::Omit,
    );
    pub const OMIT_REPORTED_REASONING_CONTROLS: Self = Self::new(
        "responses.omit-incidental-reasoning-controls.v1",
        Field::ReportedReasoning,
        Change::Omit,
    );
    pub const OMIT_CHAT_REPORTED_SETTINGS: Self = Self::new(
        "chat.omit-incidental-settings.v1",
        Field::ReportedSettings,
        Change::Omit,
    );
    pub const OMIT_CHAT_TEXT_ITEM_STATUS: Self = Self::new(
        "chat.omit-text-item-status.v1",
        Field::MessageStatus,
        Change::Omit,
    );
    pub const ECHO_REQUEST_METADATA: Self = Self::new(
        "responses.echo-request-metadata.v1",
        Field::ResponseMetadata,
        Change::InferFromRequest,
    );
    pub const OMIT_RESPONSES_SYSTEM_FINGERPRINT: Self = Self::new(
        "responses.omit-system-fingerprint.v1",
        Field::SystemFingerprint,
        Change::Omit,
    );
    pub const OMIT_RESPONSES_INPUT_IMAGE_TOKENS: Self = Self::new(
        "responses.omit-input-image-tokens.v1",
        Field::InputImageTokens,
        Change::Omit,
    );
    pub const OMIT_RESPONSES_INPUT_TEXT_TOKENS: Self = Self::new(
        "responses.omit-input-text-tokens.v1",
        Field::InputTextTokens,
        Change::Omit,
    );
    pub const OMIT_RESPONSES_OUTPUT_TEXT_TOKENS: Self = Self::new(
        "responses.omit-output-text-tokens.v1",
        Field::OutputTextTokens,
        Change::Omit,
    );
    pub const OMIT_RESPONSES_INPUT_AUDIO_TOKENS: Self = Self::new(
        "responses.omit-input-audio-tokens.v1",
        Field::InputAudioTokens,
        Change::Omit,
    );
    pub const OMIT_RESPONSES_OUTPUT_AUDIO_TOKENS: Self = Self::new(
        "responses.omit-output-audio-tokens.v1",
        Field::OutputAudioTokens,
        Change::Omit,
    );
    pub const OMIT_RESPONSES_ACCEPTED_PREDICTION_TOKENS: Self = Self::new(
        "responses.omit-accepted-prediction-tokens.v1",
        Field::AcceptedPredictionTokens,
        Change::Omit,
    );
    pub const OMIT_RESPONSES_REJECTED_PREDICTION_TOKENS: Self = Self::new(
        "responses.omit-rejected-prediction-tokens.v1",
        Field::RejectedPredictionTokens,
        Change::Omit,
    );
    pub const OMIT_RESPONSES_MESSAGE_ENVELOPES: Self = Self::new(
        "responses.omit-message-envelopes.v1",
        Field::MessageEnvelopes,
        Change::Omit,
    );
    pub const OMIT_CHAT_MESSAGE_PHASE: Self = Self::new(
        "chat.omit-message-phase.v1",
        Field::MessagePhase,
        Change::Omit,
    );
    pub const REGROUP_CHAT_MESSAGE_ITEMS: Self = Self::new(
        "chat.regroup-message-items.v1",
        Field::MessageEnvelopes,
        Change::Approximate,
    );
    pub const COARSEN_CHAT_COMPLETION_CAUSE: Self = Self::new(
        "chat.coarsen-completion-cause.v1",
        Field::Completion,
        Change::Approximate,
    );
    pub const OMIT_RESPONSES_COMPLETION_CAUSE: Self = Self::new(
        "responses.omit-completion-cause.v1",
        Field::Completion,
        Change::Omit,
    );
    pub const RECITATION_AS_CONTENT_FILTER: Self = Self::new(
        "openai.recitation-as-content-filter.v1",
        Field::Incomplete,
        Change::Approximate,
    );
    pub const CONTEXT_WINDOW_AS_LENGTH: Self = Self::new(
        "chat.context-window-as-length.v1",
        Field::Incomplete,
        Change::Approximate,
    );
    pub const OMIT_RESPONSES_CONTEXT_WINDOW_REASON: Self = Self::new(
        "responses.omit-context-window-reason.v1",
        Field::Incomplete,
        Change::Omit,
    );
    pub const OMIT_EXPLANATORY_REFUSAL_DETAILS: Self = Self::new(
        "openai.omit-explanatory-refusal-details.v1",
        Field::Refusal,
        Change::Omit,
    );
    pub const OMIT_TERMINAL_EXPLANATION: Self = Self::new(
        "openai.omit-terminal-explanation.v1",
        Field::Explanation,
        Change::Omit,
    );
    pub const OMIT_UNREPRESENTABLE_SERVICE_CLASS: Self = Self::new(
        "openai.omit-unrepresentable-service-class.v1",
        Field::ServiceClass,
        Change::Omit,
    );
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Disposition {
    Native,
    Equivalent,
    Approximate(Rule),
    Omit(Rule),
    Delegate,
    Reject(&'static str),
}
impl Disposition {
    pub fn accepts(self) -> bool {
        !matches!(self, Self::Reject(_))
    }
    pub fn rule(self) -> Option<Rule> {
        match self {
            Self::Approximate(r) | Self::Omit(r) => Some(r),
            _ => None,
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Dispositions {
    entries: [Disposition; Feature::COUNT],
}
impl Dispositions {
    pub fn full() -> Self {
        let mut table = Self {
            entries: [Disposition::Native; Feature::COUNT],
        };
        table.set(
            Feature::ProviderObservation,
            Disposition::Reject("hosted execution is not scheduled"),
        );
        table
    }
    pub fn empty() -> Self {
        let mut table = Self::full();
        for feature in Feature::iter() {
            table.set_native(feature, false);
        }
        table
    }
    pub fn text_images() -> Self {
        let mut table = Self::empty();
        for feature in [
            Feature::Instructions,
            Feature::OrderedInstructions,
            Feature::Temperature,
            Feature::MaxOutputTokens,
            Feature::Tools,
            Feature::ToolChoice,
            Feature::SerialTools,
            Feature::TextMetadata,
            Feature::TopP,
            Feature::Logprobs,
            Feature::Verbosity,
            Feature::Truncation,
            Feature::StructuredOutput,
            Feature::Reasoning,
            Feature::ImageInput,
            Feature::ParallelTools,
            Feature::ConditionalToolStrictness,
        ] {
            table.set_native(feature, true);
        }
        table
    }
    pub fn get(&self, feature: Feature) -> Disposition {
        self.entries[feature as usize]
    }
    pub fn set(&mut self, feature: Feature, disposition: Disposition) {
        self.entries[feature as usize] = disposition;
    }
    pub fn native(&self, feature: Feature) -> bool {
        self.get(feature) == Disposition::Native
    }
    pub fn accepts(&self, feature: Feature) -> bool {
        self.get(feature).accepts()
    }
    pub fn set_native(&mut self, feature: Feature, native: bool) {
        self.set(
            feature,
            if native {
                Disposition::Native
            } else {
                unsupported(feature)
            },
        );
    }
    pub fn with_native(mut self, feature: Feature, native: bool) -> Self {
        self.set_native(feature, native);
        self
    }
    pub fn supports(&self, promise: &Self) -> bool {
        Feature::iter().all(|feature| !promise.native(feature) || self.accepts(feature))
    }
    /// Trusted declarations cannot invent a native carrier merely because an
    /// input can instead be approximated or omitted.
    pub(crate) fn supports_native(&self, declaration: &Self) -> bool {
        Feature::iter().all(|feature| !declaration.native(feature) || self.native(feature))
    }
    /// Actual output facts have their own requirements, not a fabricated request.
    pub fn check_response(
        &self,
        response: &GenerationResponse,
    ) -> Result<GenerationResponseRequirements, Feature> {
        use Feature::*;
        let q = GenerationResponseRequirements::derive(response);
        if q.provider_observations {
            return Err(ProviderObservation);
        }
        for (needed, supported, feature) in [
            (q.safety_reports, self.native(Feature::Safety), Safety),
            (q.compaction, self.native(Feature::Compaction), Compaction),
            (
                q.audio_output,
                self.native(Feature::AudioOutput),
                AudioOutput,
            ),
            (
                q.instructions,
                self.native(Feature::Instructions),
                Instructions,
            ),
            (q.tools, self.native(Feature::Tools), Tools),
            (q.custom_tools, self.native(Feature::CustomTools), Tools),
            (
                q.structured_arguments,
                self.native(Feature::StructuredArguments),
                StructuredArguments,
            ),
            (
                q.structured_tool_results,
                self.native(Feature::StructuredToolResult),
                StructuredToolResult,
            ),
            (
                q.tool_execution_reports,
                self.native(Feature::ToolExecutionReport),
                ToolExecutionReport,
            ),
            (
                q.tool_result_errors,
                self.native(Feature::ToolResultError),
                ToolResultError,
            ),
            (
                q.tool_result_media,
                self.native(Feature::ToolResultMedia),
                ToolResultMedia,
            ),
            (q.reasoning, self.native(Feature::Reasoning), Reasoning),
            (
                q.response_effort,
                self.native(Feature::ResponseEffort),
                ResponseEffort,
            ),
            (
                q.text_metadata,
                self.native(Feature::TextMetadata),
                TextMetadata,
            ),
            (q.logprobs, self.native(Feature::Logprobs), Controls),
        ] {
            if needed && !supported {
                return Err(feature);
            }
        }
        Ok(q)
    }

    /// Source admission and final projection use only table-owned L5 decisions.
    pub fn check(
        &self,
        request: &GenerationRequest,
    ) -> Result<std::collections::BTreeMap<Feature, usize>, Feature> {
        let facts = request.features();
        for &feature in facts.keys() {
            if !self.accepts(feature) {
                return Err(feature);
            }
        }
        Ok(facts)
    }
}

pub(crate) const OMIT_MESSAGES_REASONING: Rule = Rule::new(
    "messages.omit-uncarried-reasoning.v1",
    Field::ReasoningContent,
    Change::Omit,
);
pub(crate) const OMIT_IDENTITY_HINTS: Rule = Rule::new(
    "generation.omit-identity-hints.v1",
    Field::Identity,
    Change::Omit,
);
pub(crate) const OMIT_USER_BUCKETING: Rule = Rule::new(
    "cache.omit-legacy-user-bucketing.v1",
    Field::IdentityBucket,
    Change::Omit,
);
pub(crate) const OMIT_EAGER_INPUT: Rule = Rule::new(
    "generation.omit-eager-tool-input.v1",
    Field::EagerToolInput,
    Change::Omit,
);
pub(crate) const TRANSFER_CHAT_HISTORY: Rule = Rule::new(
    "chat.transfer-visible-history.v1",
    Field::Replay,
    Change::Omit,
);
pub(crate) const TRANSFER_RESPONSES_HISTORY: Rule = Rule::new(
    "responses.transfer-visible-history.v1",
    Field::Replay,
    Change::Omit,
);
pub(crate) const TRANSFER_MESSAGES_HISTORY: Rule = Rule::new(
    "messages.transfer-visible-history.v1",
    Field::Replay,
    Change::Omit,
);
pub(crate) const MESSAGES_REASONING_AS_TEXT: Rule = Rule::new(
    "messages.reasoning-as-text.v1",
    Field::ReportedReasoning,
    Change::Approximate,
);
pub(crate) const MESSAGES_SUMMARY: Rule = Rule::new(
    "messages.summarize-reasoning.v1",
    Field::ReasoningSummary,
    Change::Approximate,
);
pub(crate) const GENERATE_CONTENT_SUMMARY: Rule = Rule::new(
    "gemini.summarize-reasoning.v1",
    Field::ReasoningSummary,
    Change::Approximate,
);
pub(crate) const INTERACTIONS_SUMMARY: Rule = Rule::new(
    "interactions.summarize-reasoning.v1",
    Field::ReasoningSummary,
    Change::Approximate,
);
pub(crate) const GENERATE_CONTENT_REASONING_AS_TEXT: Rule = Rule::new(
    "gemini.reasoning-as-text.v1",
    Field::ReportedReasoning,
    Change::Approximate,
);
pub(crate) const INTERACTIONS_REASONING_AS_TEXT: Rule = Rule::new(
    "interactions.reasoning-as-text.v1",
    Field::ReportedReasoning,
    Change::Approximate,
);
pub(crate) const TRANSFER_GENERATE_CONTENT_HISTORY: Rule = Rule::new(
    "gemini.transfer-visible-history.v1",
    Field::Replay,
    Change::Omit,
);
pub(crate) const TRANSFER_INTERACTIONS_HISTORY: Rule = Rule::new(
    "interactions.transfer-visible-history.v1",
    Field::Replay,
    Change::Omit,
);
pub(crate) const OMIT_GENERATE_CONTENT_SESSION: Rule =
    Rule::new("gemini.omit-cache-key.v1", Field::Cache, Change::Omit);
pub(crate) const OMIT_INTERACTIONS_SESSION: Rule =
    Rule::new("interactions.omit-cache-key.v1", Field::Cache, Change::Omit);
pub(crate) const OMIT_REPLAY_MISMATCH: Rule = Rule::new(
    "messages.omit-block-binding.v1",
    Field::Replay,
    Change::Approximate,
);
pub(crate) const OMIT_PHASE: Rule = Rule::new(
    "generation.omit-message-phase.v1",
    Field::MessagePhase,
    Change::Omit,
);
pub(crate) const CONDITIONAL_STRICTNESS: Rule = Rule::new(
    "openai.omit-conditional-tool-strictness.v1",
    Field::Tools,
    Change::Approximate,
);
pub(crate) const EFFORT: Rule = Rule::new(
    "generation.project-effort.v1",
    Field::Effort,
    Change::Approximate,
);
pub(crate) const BUDGET_TO_EFFORT: Rule = Rule::new(
    "generation.budget-to-effort.v1",
    Field::ReasoningAllocation,
    Change::Approximate,
);
pub(crate) const BUDGET_WITH_EFFORT: Rule = Rule::new(
    "generation.adaptive-preserve-explicit-effort.v1",
    Field::ReasoningAllocation,
    Change::Omit,
);
pub(crate) const CLAMP_OUTPUT: Rule = Rule::new(
    "generation.clamp-output-to-model-maximum.v1",
    Field::MaximumOutput,
    Change::Approximate,
);
pub(crate) const OMIT_SERVICE: Rule = Rule::new(
    "generation.omit-service-policy.v1",
    Field::ServiceClass,
    Change::Omit,
);

pub(crate) const MESSAGES_SCHEMA_NAME: Rule = Rule::new(
    "messages.omit-schema-name.v1",
    Field::SchemaName,
    Change::Omit,
);
pub(crate) const GENERATE_CONTENT_SCHEMA_NAME: Rule = Rule::new(
    "gemini.omit-schema-name.v1",
    Field::SchemaName,
    Change::Omit,
);
pub(crate) const INTERACTIONS_SCHEMA_NAME: Rule = Rule::new(
    "interactions.omit-schema-name.v1",
    Field::SchemaName,
    Change::Omit,
);
pub(crate) const MESSAGE_EFFORT: Rule = Rule::new(
    "messages.flatten-message-effort.v1",
    Field::Effort,
    Change::Omit,
);
pub(crate) const TOOL_ERROR: Rule = Rule::new(
    "openai.wrap-tool-result-errors.v1",
    Field::ToolResultError,
    Change::Approximate,
);
pub(crate) const TOOL_CHANGES: Rule = Rule::new(
    "messages.collapse-tool-changes.v1",
    Field::Tools,
    Change::Approximate,
);

pub(crate) const OMIT_IMAGE_DETAIL: Rule = Rule::new(
    "generation.omit-image-detail.v1",
    Field::Representation,
    Change::Omit,
);
pub(crate) const OMIT_REASONING_SUMMARY: Rule = Rule::new(
    "generation.omit-reasoning-summary.v1",
    Field::ReasoningSummary,
    Change::Omit,
);
pub(crate) const OMIT_REASONING_INCLUDE: Rule = Rule::new(
    "generation.omit-encrypted-reasoning-include.v1",
    Field::Replay,
    Change::Omit,
);

pub(crate) fn unsupported(feature: Feature) -> Disposition {
    match feature {
        Feature::ToolResultError => Disposition::Approximate(TOOL_ERROR),
        Feature::ToolChoice | Feature::SerialTools => Disposition::Delegate,
        Feature::ImageDetail => Disposition::Omit(OMIT_IMAGE_DETAIL),
        Feature::MessageCount
        | Feature::TextPartCount
        | Feature::ResourceCount
        | Feature::ReasoningHistory => Disposition::Native,
        Feature::Temperature => Disposition::Omit(Rule::new(
            "generation.omit-temperature.v1",
            Field::Sampling,
            Change::Omit,
        )),
        Feature::TopP => Disposition::Omit(Rule::new(
            "generation.omit-top-p.v1",
            Field::Sampling,
            Change::Omit,
        )),
        Feature::TopK => Disposition::Omit(Rule::new(
            "generation.omit-top-k.v1",
            Field::Sampling,
            Change::Omit,
        )),
        Feature::Seed => Disposition::Omit(Rule::new(
            "generation.omit-seed.v1",
            Field::Sampling,
            Change::Omit,
        )),
        Feature::PresencePenalty => Disposition::Omit(Rule::new(
            "generation.omit-presence-penalty.v1",
            Field::Sampling,
            Change::Omit,
        )),
        Feature::FrequencyPenalty => Disposition::Omit(Rule::new(
            "generation.omit-frequency-penalty.v1",
            Field::Sampling,
            Change::Omit,
        )),
        Feature::MaxOutputTokens => Disposition::Omit(Rule::new(
            "generation.omit-max-output-tokens.v1",
            Field::MaximumOutput,
            Change::Omit,
        )),
        Feature::ResponseEffort => Disposition::Omit(Rule::new(
            "generation.omit-response-effort.v1",
            Field::Effort,
            Change::Omit,
        )),
        Feature::ExplicitCache => Disposition::Omit(Rule::new(
            "generation.omit-explicit-cache.v1",
            Field::Cache,
            Change::Omit,
        )),
        Feature::AutomaticPrefixCache => Disposition::Omit(Rule::new(
            "generation.omit-automatic-prefix-cache.v1",
            Field::Cache,
            Change::Omit,
        )),
        Feature::RequestLifetimeCache => Disposition::Omit(Rule::new(
            "generation.omit-request-lifetime-cache.v1",
            Field::Cache,
            Change::Omit,
        )),
        Feature::ReasoningBudget => Disposition::Omit(Rule::new(
            "generation.omit-reasoning-budget.v1",
            Field::ReasoningAllocation,
            Change::Omit,
        )),
        Feature::SchemaName => Disposition::Omit(Rule::new(
            "generation.omit-schema-name.v1",
            Field::SchemaName,
            Change::Omit,
        )),
        Feature::ReasoningActivation => Disposition::Omit(Rule::new(
            "generation.omit-reasoning-activation.v1",
            Field::ReasoningAllocation,
            Change::Omit,
        )),
        Feature::ReasoningMode => Disposition::Omit(Rule::new(
            "generation.omit-reasoning-mode.v1",
            Field::ReasoningAllocation,
            Change::Omit,
        )),
        Feature::ReasoningDisplay => Disposition::Omit(Rule::new(
            "generation.omit-reasoning-display.v1",
            Field::ReasoningSummary,
            Change::Omit,
        )),
        Feature::ReasoningSummary => Disposition::Omit(Rule::new(
            "generation.omit-reasoning-summary.v1",
            Field::ReasoningSummary,
            Change::Omit,
        )),
        Feature::EncryptedReasoning => Disposition::Omit(Rule::new(
            "generation.omit-encrypted-reasoning.v1",
            Field::Replay,
            Change::Omit,
        )),
        Feature::Truncation => Disposition::Omit(Rule::new(
            "generation.omit-truncation.v1",
            Field::DeliveredProjection,
            Change::Omit,
        )),
        Feature::Logprobs => Disposition::Omit(Rule::new(
            "generation.omit-logprobs.v1",
            Field::Sampling,
            Change::Omit,
        )),
        Feature::ConditionalToolStrictness => Disposition::Omit(Rule::new(
            "generation.omit-conditional-tool-strictness.v1",
            Field::Tools,
            Change::Omit,
        )),
        Feature::Metadata => Disposition::Omit(Rule::new(
            "generation.omit-metadata.v1",
            Field::ResponseMetadata,
            Change::Omit,
        )),
        Feature::ServicePolicy => Disposition::Omit(Rule::new(
            "generation.omit-service-policy.v1",
            Field::ServiceClass,
            Change::Omit,
        )),
        Feature::CacheKey => Disposition::Omit(Rule::new(
            "generation.omit-cache-key.v1",
            Field::CacheKey,
            Change::Omit,
        )),
        Feature::CacheRetention => Disposition::Omit(Rule::new(
            "generation.omit-cache-retention.v1",
            Field::CacheRetention,
            Change::Omit,
        )),
        Feature::CacheOptions => Disposition::Omit(Rule::new(
            "generation.omit-cache-options.v1",
            Field::CacheOptions,
            Change::Omit,
        )),
        Feature::Identity => Disposition::Omit(Rule::new(
            "generation.omit-identity.v1",
            Field::Identity,
            Change::Omit,
        )),
        Feature::Prediction => Disposition::Omit(Rule::OMIT_PREDICTION),
        Feature::MessagePhase => Disposition::Omit(Rule::new(
            "generation.omit-message-phase.v1",
            Field::MessagePhase,
            Change::Omit,
        )),
        Feature::Verbosity => Disposition::Omit(Rule::new(
            "generation.omit-verbosity.v1",
            Field::Verbosity,
            Change::Omit,
        )),
        Feature::ParallelTools => Disposition::Omit(Rule::new(
            "generation.omit-parallel-tools.v1",
            Field::ParallelTools,
            Change::Omit,
        )),
        Feature::Effort => Disposition::Omit(Rule::new(
            "generation.omit-effort.v1",
            Field::Effort,
            Change::Omit,
        )),
        _ => Disposition::Reject("the requested content or explicit guarantee has no carrier"),
    }
}
