//! Body-free conversion evidence. No runtime, routing, publication or telemetry authority.
use crate::semantic::{task::generation::*, value::Presence};
use serde::{Serialize, Serializer, ser::SerializeStruct};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct RuleId(&'static str);
impl RuleId {
    pub const fn new(value: &'static str) -> Self {
        Self(value)
    }
    pub const fn as_str(self) -> &'static str {
        self.0
    }
}
impl From<&'static str> for RuleId {
    fn from(value: &'static str) -> Self {
        Self::new(value)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize)]
pub enum Field {
    #[serde(rename = "generation.service_class")]
    ServiceClass,
    #[serde(rename = "generation.identity.bucketing")]
    IdentityBucket,
    #[serde(rename = "generation.cache.retention")]
    CacheRetention,
    #[serde(rename = "generation.cache.options")]
    CacheOptions,
    #[serde(rename = "generation.cache.key")]
    CacheKey,
    #[serde(rename = "generation.parallel_tool_calls")]
    ParallelTools,
    #[serde(rename = "generation.tool_choice")]
    ToolChoice,
    #[serde(rename = "generation.document_context")]
    DocumentContext,
    #[serde(rename = "generation.cached_context")]
    CachedContext,
    #[serde(rename = "generation.safety")]
    Safety,
    #[serde(rename = "generation.delivered_projection")]
    DeliveredProjection,
    #[serde(rename = "generation.terminal.explanation")]
    TerminalExplanation,
    #[serde(rename = "generation.completion_cause")]
    CompletionCause,
    #[serde(rename = "generation.prediction")]
    Prediction,
    #[serde(rename = "generation.reasoning.allocation")]
    ReasoningAllocation,
    #[serde(rename = "generation.effort")]
    Effort,
    #[serde(rename = "generation.reasoning.summary")]
    ReasoningSummary,
    #[serde(rename = "generation.instructions.authority")]
    InstructionAuthority,
    #[serde(rename = "generation.sampling")]
    Sampling,
    #[serde(rename = "generation.text.verbosity")]
    Verbosity,
    #[serde(rename = "generation.message.phase")]
    MessagePhase,
    #[serde(rename = "generation.message_envelopes")]
    MessageEnvelopes,
    #[serde(rename = "generation.usage")]
    Usage,
    #[serde(rename = "generation.usage.reports")]
    UsageReports,
    #[serde(rename = "generation.usage.input_tokens")]
    InputTokens,
    #[serde(rename = "generation.usage.output_tokens")]
    OutputTokens,
    #[serde(rename = "generation.usage.total_tokens")]
    TotalTokens,
    #[serde(rename = "generation.usage.cached_input_tokens")]
    CachedTokens,
    #[serde(rename = "generation.usage.input_cache_write_tokens")]
    CacheWriteTokens,
    #[serde(rename = "generation.usage.reasoning_tokens")]
    ReasoningTokens,
    #[serde(rename = "generation.usage.input_image_tokens")]
    InputImageTokens,
    #[serde(rename = "generation.usage.input_text_tokens")]
    InputTextTokens,
    #[serde(rename = "generation.usage.input_audio_tokens")]
    InputAudioTokens,
    #[serde(rename = "generation.usage.output_text_tokens")]
    OutputTextTokens,
    #[serde(rename = "generation.usage.output_audio_tokens")]
    OutputAudioTokens,
    #[serde(rename = "generation.usage.accepted_prediction_tokens")]
    AcceptedPredictionTokens,
    #[serde(rename = "generation.usage.rejected_prediction_tokens")]
    RejectedPredictionTokens,
    #[serde(rename = "response.reports.opening")]
    OpeningReport,
    #[serde(rename = "response.reported.reasoning")]
    ReportedReasoning,
    #[serde(rename = "response.reported.effort")]
    ReportedEffort,
    #[serde(rename = "response.reported.settings")]
    ReportedSettings,
    #[serde(rename = "response.context.system_fingerprint")]
    SystemFingerprint,
    #[serde(rename = "response.terminal.completion")]
    Completion,
    #[serde(rename = "response.terminal.incomplete")]
    Incomplete,
    #[serde(rename = "response.terminal.refusal")]
    Refusal,
    #[serde(rename = "response.terminal.explanation")]
    Explanation,
    #[serde(rename = "generation.content.refusal")]
    ContentRefusal,
    #[serde(rename = "response.safety_reports")]
    SafetyReports,
    #[serde(rename = "response.input_transformations")]
    InputTransformations,
    #[serde(rename = "generation.content.annotations")]
    HistoryAnnotations,
    #[serde(rename = "response.delivery.usage")]
    DeliveryUsage,
    #[serde(rename = "response.delivery.settings")]
    DeliverySettings,
    #[serde(rename = "generation.tool_results.is_error")]
    ToolResultError,
    #[serde(rename = "generation.tool_results.execution")]
    ToolExecution,
    #[serde(rename = "generation.tool_calls.arguments")]
    ToolArguments,
    #[serde(rename = "generation.tool_calls.identity")]
    ToolCallIdentity,
    #[serde(rename = "generation.schema.name")]
    SchemaName,
    #[serde(rename = "generation.cache")]
    Cache,
    #[serde(rename = "generation.identity")]
    Identity,
    #[serde(rename = "generation.tools")]
    Tools,
    #[serde(rename = "generation.replay")]
    Replay,
    #[serde(rename = "protocol.representation")]
    Representation,
    #[serde(rename = "generation.max_output_tokens")]
    MaximumOutput,
}
impl Field {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ServiceClass => "generation.service_class",
            Self::IdentityBucket => "generation.identity.bucketing",
            Self::CacheRetention => "generation.cache.retention",
            Self::CacheOptions => "generation.cache.options",
            Self::CacheKey => "generation.cache.key",
            Self::ParallelTools => "generation.parallel_tool_calls",
            Self::ToolChoice => "generation.tool_choice",
            Self::DocumentContext => "generation.document_context",
            Self::CachedContext => "generation.cached_context",
            Self::Safety => "generation.safety",
            Self::DeliveredProjection => "generation.delivered_projection",
            Self::TerminalExplanation => "generation.terminal.explanation",
            Self::CompletionCause => "generation.completion_cause",
            Self::Prediction => "generation.prediction",
            Self::ReasoningAllocation => "generation.reasoning.allocation",
            Self::Effort => "generation.effort",
            Self::ReasoningSummary => "generation.reasoning.summary",
            Self::InstructionAuthority => "generation.instructions.authority",
            Self::Sampling => "generation.sampling",
            Self::Verbosity => "generation.text.verbosity",
            Self::MessagePhase => "generation.message.phase",
            Self::MessageEnvelopes => "generation.message_envelopes",
            Self::Usage => "generation.usage",
            Self::UsageReports => "generation.usage.reports",
            Self::InputTokens => "generation.usage.input_tokens",
            Self::OutputTokens => "generation.usage.output_tokens",
            Self::TotalTokens => "generation.usage.total_tokens",
            Self::CachedTokens => "generation.usage.cached_input_tokens",
            Self::CacheWriteTokens => "generation.usage.input_cache_write_tokens",
            Self::ReasoningTokens => "generation.usage.reasoning_tokens",
            Self::InputImageTokens => "generation.usage.input_image_tokens",
            Self::InputTextTokens => "generation.usage.input_text_tokens",
            Self::InputAudioTokens => "generation.usage.input_audio_tokens",
            Self::OutputTextTokens => "generation.usage.output_text_tokens",
            Self::OutputAudioTokens => "generation.usage.output_audio_tokens",
            Self::AcceptedPredictionTokens => "generation.usage.accepted_prediction_tokens",
            Self::RejectedPredictionTokens => "generation.usage.rejected_prediction_tokens",
            Self::OpeningReport => "response.reports.opening",
            Self::ReportedReasoning => "response.reported.reasoning",
            Self::ReportedEffort => "response.reported.effort",
            Self::ReportedSettings => "response.reported.settings",
            Self::SystemFingerprint => "response.context.system_fingerprint",
            Self::Completion => "response.terminal.completion",
            Self::Incomplete => "response.terminal.incomplete",
            Self::Refusal => "response.terminal.refusal",
            Self::Explanation => "response.terminal.explanation",
            Self::ContentRefusal => "generation.content.refusal",
            Self::SafetyReports => "response.safety_reports",
            Self::InputTransformations => "response.input_transformations",
            Self::HistoryAnnotations => "generation.content.annotations",
            Self::DeliveryUsage => "response.delivery.usage",
            Self::DeliverySettings => "response.delivery.settings",
            Self::ToolResultError => "generation.tool_results.is_error",
            Self::ToolExecution => "generation.tool_results.execution",
            Self::ToolArguments => "generation.tool_calls.arguments",
            Self::ToolCallIdentity => "generation.tool_calls.identity",
            Self::SchemaName => "generation.schema.name",
            Self::Cache => "generation.cache",
            Self::Identity => "generation.identity",
            Self::Tools => "generation.tools",
            Self::Replay => "generation.replay",
            Self::Representation => "protocol.representation",
            Self::MaximumOutput => "generation.max_output_tokens",
        }
    }
}
impl From<&'static str> for Field {
    fn from(value: &'static str) -> Self {
        match value {
            "generation.service_class" => Self::ServiceClass,
            "generation.identity.bucketing" => Self::IdentityBucket,
            "generation.cache.retention" => Self::CacheRetention,
            "generation.cache.options" => Self::CacheOptions,
            "generation.cache.key" => Self::CacheKey,
            "generation.parallel_tool_calls" => Self::ParallelTools,
            "generation.tool_choice" => Self::ToolChoice,
            "generation.document_context" => Self::DocumentContext,
            "generation.cached_context" => Self::CachedContext,
            "generation.safety" => Self::Safety,
            "generation.delivered_projection" => Self::DeliveredProjection,
            "generation.terminal.explanation" => Self::TerminalExplanation,
            "generation.completion_cause" => Self::CompletionCause,
            "generation.prediction" => Self::Prediction,
            "generation.reasoning.allocation" => Self::ReasoningAllocation,
            "generation.effort" => Self::Effort,
            "generation.reasoning.summary" => Self::ReasoningSummary,
            "generation.instructions.authority" => Self::InstructionAuthority,
            "generation.sampling" => Self::Sampling,
            "generation.text.verbosity" => Self::Verbosity,
            "generation.message.phase" => Self::MessagePhase,
            "generation.message_envelopes" => Self::MessageEnvelopes,
            "generation.usage" => Self::Usage,
            "generation.usage.reports" => Self::UsageReports,
            "generation.usage.input_tokens" => Self::InputTokens,
            "generation.usage.output_tokens" => Self::OutputTokens,
            "generation.usage.total_tokens" => Self::TotalTokens,
            "generation.usage.cached_input_tokens" => Self::CachedTokens,
            "generation.usage.input_cache_write_tokens" => Self::CacheWriteTokens,
            "generation.usage.reasoning_tokens" => Self::ReasoningTokens,
            "generation.usage.input_image_tokens" => Self::InputImageTokens,
            "generation.usage.input_text_tokens" => Self::InputTextTokens,
            "generation.usage.input_audio_tokens" => Self::InputAudioTokens,
            "generation.usage.output_text_tokens" => Self::OutputTextTokens,
            "generation.usage.output_audio_tokens" => Self::OutputAudioTokens,
            "generation.usage.accepted_prediction_tokens" => Self::AcceptedPredictionTokens,
            "generation.usage.rejected_prediction_tokens" => Self::RejectedPredictionTokens,
            "response.reports.opening" => Self::OpeningReport,
            "response.reported.reasoning" => Self::ReportedReasoning,
            "response.reported.effort" => Self::ReportedEffort,
            "response.reported.settings" => Self::ReportedSettings,
            "response.context.system_fingerprint" => Self::SystemFingerprint,
            "response.terminal.completion" => Self::Completion,
            "response.terminal.incomplete" => Self::Incomplete,
            "response.terminal.refusal" => Self::Refusal,
            "response.terminal.explanation" => Self::Explanation,
            "generation.content.refusal" => Self::ContentRefusal,
            "response.safety_reports" => Self::SafetyReports,
            "response.input_transformations" => Self::InputTransformations,
            "generation.content.annotations" => Self::HistoryAnnotations,
            "response.delivery.usage" => Self::DeliveryUsage,
            "response.delivery.settings" => Self::DeliverySettings,
            "generation.tool_results.is_error" => Self::ToolResultError,
            "generation.tool_results.execution" => Self::ToolExecution,
            "generation.tool_calls.arguments" => Self::ToolArguments,
            "generation.tool_calls.identity" => Self::ToolCallIdentity,
            "generation.schema.name" => Self::SchemaName,
            "generation.cache" => Self::Cache,
            "generation.identity" => Self::Identity,
            "generation.tools" => Self::Tools,
            "generation.replay" => Self::Replay,
            "protocol.representation" => Self::Representation,
            "generation.max_output_tokens" => Self::MaximumOutput,
            _ => panic!("undeclared static evidence field"),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize)]
pub enum ConversionBoundary {
    #[serde(rename = "request.decode")]
    RequestDecode,
    #[serde(rename = "request.project")]
    RequestProject,
    #[serde(rename = "request.encode")]
    RequestEncode,
    #[serde(rename = "response.decode")]
    ResponseDecode,
    #[serde(rename = "response.project")]
    ResponseProject,
    #[serde(rename = "response.encode")]
    ResponseEncode,
    #[serde(rename = "event.decode")]
    EventDecode,
    #[serde(rename = "event.project")]
    EventProject,
    #[serde(rename = "event.encode")]
    EventEncode,
}
impl ConversionBoundary {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RequestDecode => "request.decode",
            Self::RequestProject => "request.project",
            Self::RequestEncode => "request.encode",
            Self::ResponseDecode => "response.decode",
            Self::ResponseProject => "response.project",
            Self::ResponseEncode => "response.encode",
            Self::EventDecode => "event.decode",
            Self::EventProject => "event.project",
            Self::EventEncode => "event.encode",
        }
    }
}
impl From<&'static str> for ConversionBoundary {
    fn from(value: &'static str) -> Self {
        match value {
            "request.decode" => Self::RequestDecode,
            "request.project" => Self::RequestProject,
            "request.encode" => Self::RequestEncode,
            "response.decode" => Self::ResponseDecode,
            "response.project" => Self::ResponseProject,
            "response.encode" => Self::ResponseEncode,
            "event.decode" => Self::EventDecode,
            "event.project" => Self::EventProject,
            "event.encode" => Self::EventEncode,
            _ => panic!("undeclared static conversion boundary"),
        }
    }
}
/// The mandatory versioned rule names the method/premise; no free-form explanations.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DefaultBasis {
    SourceContract,
    SelectedPolicy,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Change {
    Normalize,
    Derive,
    Default(DefaultBasis),
    InferFromRequest,
    Omit,
    Approximate,
    Estimate,
}
impl Change {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Normalize => "normalize",
            Self::Derive => "derive",
            Self::Default(_) => "default",
            Self::InferFromRequest => "infer_from_request",
            Self::Omit => "omit",
            Self::Approximate => "approximate",
            Self::Estimate => "estimate",
        }
    }
    pub const fn is_loss(self) -> bool {
        matches!(self, Self::Omit | Self::Approximate | Self::Estimate)
    }
}
impl From<&'static str> for Change {
    fn from(value: &'static str) -> Self {
        match value {
            "omit" => Self::Omit,
            "normalize" => Self::Normalize,
            "derive" | "resolve" => Self::Derive,
            "insert" | "default" => Self::Default(DefaultBasis::SelectedPolicy),
            "infer" => Self::InferFromRequest,
            "estimate" => Self::Estimate,
            "approximate" | "regroup" | "wrap" => Self::Approximate,
            "project" | "adapt" => Self::Omit,
            _ => panic!("undeclared static conversion change"),
        }
    }
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", content = "local", rename_all = "snake_case")]
pub enum Owner {
    #[default]
    Root,
    Item {
        scope: u64,
        index: u64,
    },
    Part {
        scope: u64,
        index: u64,
    },
    Tool {
        ordinal: usize,
    },
}
impl Owner {
    pub fn item(id: ItemId) -> Self {
        Self::Item {
            scope: id.scope().get(),
            index: id.get(),
        }
    }
    pub fn part(id: PartId) -> Self {
        Self::Part {
            scope: id.scope().get(),
            index: id.get(),
        }
    }
}
/// Explicitly allowlisted controls only; no instructions, schemas or identities.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReasoningControls {
    pub configuration_efforts: Vec<EffortControl>,
    pub activation: Presence<ReasoningActivation>,
    pub effort: Presence<EffortControl>,
    pub budget_tokens: Option<u64>,
    pub max_output_tokens: Option<u64>,
    pub mode: Presence<ReasoningMode>,
    pub presentation: ReasoningPresentation,
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
            presentation: settings.reasoning.presentation.clone(),
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
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Effect {
    EffortMapping {
        rule: RuleId,
        owner: Owner,
        input: EffortControl,
        output: EffortControl,
        boundary: ConversionBoundary,
    },
    Field {
        rule: RuleId,
        field: Field,
        boundary: ConversionBoundary,
        change: Change,
        owner: Owner,
    },
    Verbosity {
        owner: Owner,
        rule: RuleId,
        input: Presence<Verbosity>,
        output: Presence<Verbosity>,
    },
    Reasoning {
        owner: Owner,
        rule: RuleId,
        input: ReasoningControls,
        output: ReasoningControls,
        preset_tokens: Option<u64>,
    },
    Summary {
        owner: Owner,
        rule: RuleId,
        input: Presence<ReasoningSummary>,
        output: ReasoningDisplay,
    },
    Sampling {
        owner: Owner,
        rule: RuleId,
        temperature: bool,
        top_p: bool,
        top_k: bool,
    },
    MessagesVerbosity {
        owner: Owner,
        input: Presence<Verbosity>,
        output: Presence<Verbosity>,
    },
}

impl Effect {
    pub fn new(
        rule: &'static str,
        field: Field,
        boundary: ConversionBoundary,
        change: Change,
    ) -> Self {
        Self::Field {
            rule: rule.into(),
            field,
            boundary,
            change,
            owner: Owner::Root,
        }
    }
    pub const fn rule(&self) -> &'static str {
        match self {
            Self::EffortMapping { rule, .. }
            | Self::Field { rule, .. }
            | Self::Verbosity { rule, .. }
            | Self::Reasoning { rule, .. }
            | Self::Summary { rule, .. }
            | Self::Sampling { rule, .. } => rule.as_str(),
            Self::MessagesVerbosity { .. } => "messages.omit-verbosity.v1",
        }
    }
    pub const fn semantic_field(&self) -> Field {
        match self {
            Self::Field { field, .. } => *field,
            Self::EffortMapping {
                boundary, owner, ..
            } => {
                if !matches!(owner, Owner::Root)
                    || matches!(boundary, ConversionBoundary::RequestDecode)
                {
                    Field::Effort
                } else {
                    Field::ReportedEffort
                }
            }
            Self::Reasoning { input, output, .. } => {
                if input.budget_tokens.is_some() || output.budget_tokens.is_some() {
                    Field::ReasoningAllocation
                } else {
                    Field::Effort
                }
            }
            Self::Summary { .. } => Field::ReasoningSummary,
            Self::Sampling { .. } => Field::Sampling,
            Self::Verbosity { .. } | Self::MessagesVerbosity { .. } => Field::Verbosity,
        }
    }
    pub const fn field(&self) -> &'static str {
        self.semantic_field().as_str()
    }
    pub const fn boundary(&self) -> ConversionBoundary {
        match self {
            Self::Field { boundary, .. } | Self::EffortMapping { boundary, .. } => *boundary,
            _ => ConversionBoundary::RequestProject,
        }
    }
    pub fn change(&self) -> Change {
        match self {
            Self::Field { change, .. } => *change,
            Self::EffortMapping { boundary, .. }
                if !matches!(
                    boundary,
                    ConversionBoundary::RequestProject | ConversionBoundary::RequestEncode
                ) =>
            {
                Change::Normalize
            }
            Self::Reasoning { input, output, .. }
                if !input.effort.is_absent()
                    && output.effort.is_absent()
                    && input.budget_tokens == output.budget_tokens =>
            {
                Change::Omit
            }
            Self::Verbosity { .. } | Self::MessagesVerbosity { .. } | Self::Sampling { .. } => {
                Change::Omit
            }
            _ => Change::Approximate,
        }
    }
    pub fn owner(&self) -> Owner {
        match self {
            Self::EffortMapping { owner, .. }
            | Self::Field { owner, .. }
            | Self::Reasoning { owner, .. }
            | Self::Verbosity { owner, .. }
            | Self::Summary { owner, .. }
            | Self::Sampling { owner, .. }
            | Self::MessagesVerbosity { owner, .. } => *owner,
        }
    }
    pub fn at(mut self, value: Owner) -> Self {
        match &mut self {
            Self::EffortMapping { owner, .. }
            | Self::Field { owner, .. }
            | Self::Reasoning { owner, .. }
            | Self::Verbosity { owner, .. }
            | Self::Summary { owner, .. }
            | Self::Sampling { owner, .. }
            | Self::MessagesVerbosity { owner, .. } => *owner = value,
        }
        self
    }
    pub fn with_boundary(mut self, value: ConversionBoundary) -> Self {
        if let Self::Field { boundary, .. } | Self::EffortMapping { boundary, .. } = &mut self {
            *boundary = value;
        }
        self
    }
    pub fn retained_bytes(&self) -> usize {
        std::mem::size_of_val(self)
            + match self {
                Self::Reasoning { input, output, .. } => {
                    (input.configuration_efforts.capacity()
                        + output.configuration_efforts.capacity())
                        * std::mem::size_of::<EffortControl>()
                }
                _ => 0,
            }
    }
}
impl Serialize for Effect {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut s = serializer.serialize_struct("Effect", 7)?;
        s.serialize_field("rule", self.rule())?;
        s.serialize_field("field", &self.semantic_field())?;
        s.serialize_field("boundary", &self.boundary())?;
        s.serialize_field("change", self.change().as_str())?;
        if let Change::Default(basis) = self.change() {
            s.serialize_field("default_basis", &basis)?;
        }
        s.serialize_field("owner", &self.owner())?;
        if self.change() != Change::Omit {
            s.serialize_field("method", self.rule())?;
        }
        if let Some((input, output)) = self.control_summary() {
            s.serialize_field("input", &input)?;
            s.serialize_field("output", &output)?;
        }
        s.end()
    }
}
/// Only this finite control vocabulary can create summaries; no arbitrary value input.
fn reasoning_summary(c: &ReasoningControls) -> serde_json::Value {
    serde_json::json!({"activation":format!("{:?}",c.activation),"effort":format!("{:?}",c.effort),
        "budget_tokens":c.budget_tokens,"max_output_tokens":c.max_output_tokens,"mode":format!("{:?}",c.mode),
        "summary":format!("{:?}",c.presentation.summary),"display":format!("{:?}",c.presentation.visibility),
        "configuration_efforts":format!("{:?}",c.configuration_efforts)})
}
impl Effect {
    pub fn control_summary(&self) -> Option<(serde_json::Value, serde_json::Value)> {
        use serde_json::json;
        Some(match self {
            Self::Field { .. } => return None,
            Self::EffortMapping { input, output, .. } => {
                (json!(format!("{input:?}")), json!(format!("{output:?}")))
            }
            Self::Reasoning {
                input,
                output,
                preset_tokens,
                ..
            } => {
                let mut o = reasoning_summary(output);
                o["preset_tokens"] = json!(preset_tokens);
                (reasoning_summary(input), o)
            }
            Self::Verbosity { input, output, .. }
            | Self::MessagesVerbosity { input, output, .. } => {
                (json!(format!("{input:?}")), json!(format!("{output:?}")))
            }
            Self::Summary { input, output, .. } => {
                (json!(format!("{input:?}")), json!(format!("{output:?}")))
            }
            Self::Sampling {
                temperature,
                top_p,
                top_k,
                ..
            } => (
                json!({"temperature_present":temperature,"top_p_present":top_p,"top_k_present":top_k}),
                json!({"temperature_present":false,"top_p_present":false,"top_k_present":false}),
            ),
        })
    }
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Coverage {
    Assessed,
    #[default]
    Partial,
    Unassessed,
}
pub const MAX_EFFECTS: usize = 64;
pub const MAX_EVIDENCE_BYTES: usize = 16 * 1024;
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub struct EvidenceDelta {
    effects: Vec<Effect>,
    pub coverage: Coverage,
    pub omitted_entries: u64,
    #[serde(skip)]
    bytes: usize,
}
impl EvidenceDelta {
    pub fn push(&mut self, effect: Effect) {
        if self.effects.contains(&effect) {
            return;
        }
        let bytes = effect.retained_bytes();
        if self.effects.len() >= MAX_EFFECTS
            || self.bytes.saturating_add(bytes) > MAX_EVIDENCE_BYTES
        {
            self.omitted_entries = self.omitted_entries.saturating_add(1);
            return;
        }
        self.bytes += bytes;
        self.effects.push(effect);
    }
    pub fn extend(&mut self, other: Self) {
        self.omitted_entries = self.omitted_entries.saturating_add(other.omitted_entries);
        if other.coverage != Coverage::Assessed {
            self.coverage = other.coverage;
        }
        for e in other.effects {
            self.push(e);
        }
    }
    pub fn partition_omissions(self) -> (Self, Self) {
        let mut completed = Self::default();
        let mut deferred = Self::default();
        completed.omitted_entries = self.omitted_entries;
        completed.coverage = self.coverage;
        for effect in self.effects {
            if effect.change() == Change::Omit {
                completed.push(effect)
            } else {
                deferred.push(effect)
            }
        }
        (completed, deferred)
    }
    pub fn contains_rule(&self, rule: &str) -> bool {
        self.effects.iter().any(|e| e.rule() == rule)
    }
    pub fn rule_names(&self) -> Vec<&'static str> {
        let mut names: Vec<_> = self.effects.iter().map(Effect::rule).collect();
        names.sort_unstable();
        names.dedup();
        names
    }
    pub fn prefer_summary(&mut self, effect: Effect) {
        if !matches!(effect, Effect::Field { .. }) {
            self.effects.retain(|e| {
                !(matches!(e, Effect::Field { .. })
                    && e.rule() == effect.rule()
                    && e.owner() == effect.owner())
            });
            self.bytes = self.effects.iter().map(Effect::retained_bytes).sum();
        }
        self.push(effect);
    }
    pub(crate) fn retain(&mut self, keep: impl Fn(&Effect) -> bool) {
        self.effects.retain(keep);
        self.bytes = self.effects.iter().map(Effect::retained_bytes).sum();
    }
    pub fn iter(&self) -> impl Iterator<Item = &Effect> {
        self.effects.iter()
    }
    pub fn len(&self) -> usize {
        self.effects.len()
    }
    pub fn is_empty(&self) -> bool {
        self.effects.is_empty() && self.omitted_entries == 0
    }
    pub fn retained_bytes(&self) -> usize {
        std::mem::size_of_val(self) + self.bytes
    }
    pub fn into_effects(self) -> impl Iterator<Item = Effect> {
        self.effects.into_iter()
    }
}
impl FromIterator<Effect> for EvidenceDelta {
    fn from_iter<I: IntoIterator<Item = Effect>>(iter: I) -> Self {
        let mut out = Self::default();
        for e in iter {
            out.push(e)
        }
        out
    }
}
pub struct Conversion<T> {
    pub value: T,
    pub evidence: EvidenceDelta,
}

impl<T> std::fmt::Debug for Conversion<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Conversion")
            .field("value", &"[redacted]")
            .field("evidence", &self.evidence)
            .finish()
    }
}

impl EvidenceDelta {
    pub(crate) fn record_cache_write_default(
        &mut self,
        _profile: &'static str,
        boundary: ConversionBoundary,
    ) {
        self.push(Effect::new(
            "openai.provider-cache-write-default.v1",
            Field::CacheWriteTokens,
            boundary,
            Change::Default(crate::protocol::conversion::generation::DefaultBasis::SelectedPolicy),
        ));
    }
    pub(crate) fn record_effort_mapping(
        &mut self,
        rule: &'static str,
        input: EffortControl,
        output: EffortControl,
        boundary: ConversionBoundary,
        owner: Owner,
    ) {
        self.push(Effect::EffortMapping {
            rule: rule.into(),
            owner,
            input,
            output,
            boundary,
        });
    }
    pub(crate) fn record_input_transformations(&mut self, boundary: ConversionBoundary) {
        self.push(Effect::new(
            "messages.omit-input-transformations.v1",
            Field::InputTransformations,
            boundary,
            Change::Omit,
        ));
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct SemanticLocus {
    pub field: Field,
    pub owner: Owner,
}

impl IntoIterator for EvidenceDelta {
    type Item = Effect;
    type IntoIter = std::vec::IntoIter<Effect>;
    fn into_iter(self) -> Self::IntoIter {
        self.effects.into_iter()
    }
}
impl std::ops::Deref for EvidenceDelta {
    type Target = [Effect];
    fn deref(&self) -> &[Effect] {
        &self.effects
    }
}

impl Field {
    pub(crate) fn is_usage(self) -> bool {
        matches!(
            self,
            Self::Usage
                | Self::UsageReports
                | Self::InputTokens
                | Self::OutputTokens
                | Self::TotalTokens
                | Self::CachedTokens
                | Self::CacheWriteTokens
                | Self::ReasoningTokens
                | Self::InputTextTokens
                | Self::InputAudioTokens
                | Self::InputImageTokens
        )
    }
}

impl Serialize for ReasoningControls {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        reasoning_summary(self).serialize(s)
    }
}
