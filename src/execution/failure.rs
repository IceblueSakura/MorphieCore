//! Stable body-free failure codes and pure recovery classification.
//! Codes are explicit contracts, independent of Debug, Display and public wire spelling.
use super::{
    AttemptError,
    attempt::{DeadlineReason, DeliveryReason, ProtocolReason},
    lifecycle::LifecycleError,
};
use crate::{
    protocol::{
        CodecError, ConversionCause, ConversionFailure,
        conversion::generation::RepresentationError,
        failure::{CarrierReason, Resource, ShapeReason, SourceFailureKind},
        stream::SseError,
    },
    provider::RecoveryClass,
    semantic::{
        context::{ContextError, ContextReason},
        task::generation::{EventError, GenerationError, GenerationFeature, StreamTerminal},
    },
    transport::sse::SseDecodeError,
};

/// Selected recovery classification, not unconditional retry permission.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RecoveryDecision {
    pub class: Option<RecoveryClass>,
    pub affects_health: bool,
}
impl RecoveryDecision {
    pub fn source(status: u16, kind: SourceFailureKind) -> Self {
        let class = if kind == SourceFailureKind::Quota {
            Some(RecoveryClass::Quota)
        } else {
            match crate::provider::classify_status(status) {
                crate::provider::StatusClass::Success => None,
                crate::provider::StatusClass::Failure(class) => Some(class),
            }
        };
        Self {
            class,
            affects_health: true,
        }
    }
    pub const fn transport(class: RecoveryClass) -> Self {
        Self {
            class: Some(class),
            affects_health: true,
        }
    }
}

impl AttemptError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Codec(e) | Self::Sse(SseError::Codec(e)) => codec_code(e),
            Self::Conversion(e) => conversion_code(e),
            Self::Representation(e) => representation_code(e),
            Self::Sse(e) => sse_code(e),
            Self::Protocol(e) => protocol_code(e),
            Self::Delivery(e) => delivery_code(e),
            Self::Transport(e) => e.code(),
            Self::Status { class, .. } => recovery_code(class),
            Self::Lifecycle(e) => lifecycle_code(e),
            Self::Limit => "execution.limit",
            Self::Resource(e) => resource_code(e),
            Self::Deadline(e) => deadline_code(e),
        }
    }
}
pub(crate) fn conversion_code(e: &ConversionFailure) -> &'static str {
    match &e.cause {
        ConversionCause::Codec(e) => codec_code(e),
        ConversionCause::Representation(e) => representation_code(e),
    }
}
fn codec_code(e: &CodecError) -> &'static str {
    match e {
        CodecError::Representation { cause, .. } => representation_code(cause),
        CodecError::Invalid(e) => shape_code(e),
        CodecError::Unsupported(e) => carrier_code(e),
        CodecError::Semantic(e) => semantic_code(e),
        CodecError::Event(e) => event_code(e),
        CodecError::Context(e) => match e {
            ContextError::Invalid(e) => context_reason_code(e),
            ContextError::Limit => "context.limit",
            ContextError::Stateful => "context.stateful_unsupported",
            ContextError::Generation(e) => semantic_code(e),
        },
        CodecError::StreamFailure(e) => source_code(&e.kind),
        CodecError::ProfileMismatch => "codec.profile_mismatch",
        CodecError::Limit => "codec.value_limit",
        CodecError::Resource(e) => resource_code(e),
    }
}
fn sse_code(e: &SseError) -> &'static str {
    match e {
        SseError::Http => "sse.invalid_http",
        SseError::Limit => "sse.limit",
        SseError::EventType => "sse.event_type_mismatch",
        SseError::Closed => "sse.closed",
        SseError::Framing(e) => match e {
            SseDecodeError::EventTooLarge => "sse.event_too_large",
            SseDecodeError::InvalidUtf8 => "sse.invalid_utf8",
            SseDecodeError::UnexpectedEof => "sse.unexpected_eof",
            SseDecodeError::Closed => "sse.closed",
        },
        SseError::Codec(e) => codec_code(e),
    }
}
fn event_code(e: &EventError) -> &'static str {
    match e {
        EventError::Identity => "event.identity",
        EventError::Lifecycle => "event.lifecycle",
        EventError::Limit => "event.limit",
        EventError::EofBeforeTerminal => "event.eof_before_terminal",
        EventError::Semantic(e) => semantic_code(e),
        EventError::TerminalFailure(terminal) => match terminal {
            StreamTerminal::Completed => "event.terminal.completed",
            StreamTerminal::Incomplete => "event.terminal.incomplete",
            StreamTerminal::Failed => "event.terminal.failed",
            StreamTerminal::Cancelled => "event.terminal.cancelled",
            StreamTerminal::Error => "event.terminal.error",
        },
    }
}

fn shape_code(e: &ShapeReason) -> &'static str {
    match e {
        ShapeReason::Json => "codec.invalid.json",
        ShapeReason::Object => "codec.invalid.object",
        ShapeReason::Metadata => "codec.invalid.metadata",
        ShapeReason::MetadataIdChanged => "codec.invalid.metadata_id_changed",
        ShapeReason::MetadataModelChanged => "codec.invalid.metadata_model_changed",
        ShapeReason::MetadataCreatedChanged => "codec.invalid.metadata_created_changed",
        ShapeReason::Choices => "codec.invalid.choices",
        ShapeReason::CallIdentity => "codec.invalid.call_identity",
        ShapeReason::Delta => "codec.invalid.delta",
        ShapeReason::UsageTail => "codec.invalid.usage_tail",
        ShapeReason::EarlyUsage => "codec.invalid.early_usage",
        ShapeReason::AfterFinish => "codec.invalid.after_finish",
        ShapeReason::Sequence => "codec.invalid.sequence",
        ShapeReason::SettingsConflict => "codec.invalid.settings_conflict",
        ShapeReason::ConfigurationConflict => "codec.invalid.configuration_conflict",
        ShapeReason::ItemSnapshot => "codec.invalid.item_snapshot",
        ShapeReason::TerminalSnapshot => "codec.invalid.terminal_snapshot",
        ShapeReason::SnapshotShape => "codec.invalid.snapshot_shape",
        ShapeReason::SnapshotIdentity => "codec.invalid.snapshot_identity",
        ShapeReason::SnapshotLifecycle => "codec.invalid.snapshot_lifecycle",
        ShapeReason::SnapshotPhase => "codec.invalid.snapshot_phase",
        ShapeReason::SnapshotReplay => "codec.invalid.snapshot_replay",
        ShapeReason::SnapshotReplayAdded => "codec.invalid.snapshot_replay_added",
        ShapeReason::SnapshotReplayRemoved => "codec.invalid.snapshot_replay_removed",
        ShapeReason::SnapshotReplayChanged => "codec.invalid.snapshot_replay_changed",
        ShapeReason::SnapshotSummary => "codec.invalid.snapshot_summary",
        ShapeReason::SnapshotText => "codec.invalid.snapshot_text",
        ShapeReason::SnapshotAnnotations => "codec.invalid.snapshot_annotations",
        ShapeReason::SnapshotProbabilityPresence => "codec.invalid.snapshot_probability_presence",
        ShapeReason::SnapshotProbabilities => "codec.invalid.snapshot_probabilities",
        ShapeReason::ValueSnapshot => "codec.invalid.value_snapshot",
        ShapeReason::Reasoning => "codec.invalid.reasoning",
        ShapeReason::Probabilities => "codec.invalid.probabilities",
        ShapeReason::Item => "codec.invalid.item",
        ShapeReason::Identity => "codec.invalid.identity",
        ShapeReason::Usage => "codec.invalid.usage",
        ShapeReason::InvalidValue => "codec.invalid.invalid_value",
        ShapeReason::Status => "codec.invalid.status",
        ShapeReason::TerminalStatus => "codec.invalid.terminal_status",
        ShapeReason::TerminalExplanation => "codec.invalid.terminal_explanation",
        ShapeReason::Signature => "codec.invalid.signature",
        ShapeReason::SignaturePayload => "codec.invalid.signature_payload",
    }
}

fn carrier_code(e: &CarrierReason) -> &'static str {
    match e {
        CarrierReason::Schema => "codec.unsupported.schema",
        CarrierReason::CitationMetadata => "codec.unsupported.citation_metadata",
        CarrierReason::UrlContextMetadata => "codec.unsupported.url_context_metadata",
        CarrierReason::MeanLogprob => "codec.unsupported.mean_logprob",
        CarrierReason::SumLogprob => "codec.unsupported.sum_logprob",
        CarrierReason::TokenId => "codec.unsupported.token_id",
        CarrierReason::StepError => "codec.unsupported.step_error",
        CarrierReason::Annotations => "codec.unsupported.annotations",
        CarrierReason::MultipartContent => "codec.unsupported.multipart_content",
        CarrierReason::UnknownField => "codec.unsupported.unknown_field",
        CarrierReason::Unavailable => "codec.unsupported.unavailable",
        CarrierReason::Terminal => "codec.unsupported.terminal",
        CarrierReason::Usage => "codec.unsupported.usage",
        CarrierReason::Replay => "codec.unsupported.replay",
        CarrierReason::Reasoning => "codec.unsupported.reasoning",
        CarrierReason::Tools => "codec.unsupported.tools",
        CarrierReason::Safety => "codec.unsupported.safety",
        CarrierReason::Metadata => "codec.unsupported.metadata",
        CarrierReason::Field(_) => "codec.unsupported.field",
    }
}

fn resource_code(e: &Resource) -> &'static str {
    match e {
        Resource::Bytes => "resource.bytes",
        Resource::Depth => "resource.depth",
        Resource::Nodes => "resource.nodes",
        Resource::Events => "resource.events",
        Resource::StreamState => "resource.stream_state",
        Resource::Projection => "resource.projection",
    }
}

pub(crate) fn source_code(e: &SourceFailureKind) -> &'static str {
    match e {
        SourceFailureKind::ContextOverflow => "source.context_overflow",
        SourceFailureKind::Quota => "source.quota",
        SourceFailureKind::RateLimit => "source.rate_limit",
        SourceFailureKind::Overloaded => "source.overloaded",
        SourceFailureKind::Other => "source.other",
    }
}

fn semantic_code(e: &GenerationError) -> &'static str {
    match e {
        GenerationError::EmptyInput => "semantic.empty_input",
        GenerationError::DuplicateItemId => "semantic.duplicate_item_id",
        GenerationError::EmptyMessage => "semantic.empty_message",
        GenerationError::DuplicatePartId => "semantic.duplicate_part_id",
        GenerationError::NonFiniteTemperature => "semantic.non_finite_temperature",
        GenerationError::InvalidControl => "semantic.invalid_control",
        GenerationError::InvalidDependency => "semantic.invalid_dependency",
        GenerationError::InvalidContextTransform => "semantic.invalid_context_transform",
        GenerationError::DuplicateCall => "semantic.duplicate_call",
        GenerationError::InvalidToolResult => "semantic.invalid_tool_result",
        GenerationError::InvalidReplay => "semantic.invalid_replay",
        GenerationError::InvalidProviderObservation => "semantic.invalid_provider_observation",
        GenerationError::InvalidMessageGroup => "semantic.invalid_message_group",
        GenerationError::InvalidToolDefinition => "semantic.invalid_tool_definition",
        GenerationError::InvalidSchema => "semantic.invalid_schema",
        GenerationError::InvalidToolChoice => "semantic.invalid_tool_choice",
        GenerationError::InvalidJsonValue => "semantic.invalid_json_value",
        GenerationError::InvalidArguments => "semantic.invalid_arguments",
        GenerationError::Limit => "semantic.limit",
        GenerationError::InvalidResponse => "semantic.invalid_response",
        GenerationError::InvalidResource => "semantic.invalid_resource",
        GenerationError::InvalidCitation => "semantic.invalid_citation",
        GenerationError::PhaseInUserMessage => "semantic.phase_in_user_message",
        GenerationError::RefusalInUserMessage => "semantic.refusal_in_user_message",
        GenerationError::TextMetadataInUserMessage => "semantic.text_metadata_in_user_message",
        GenerationError::InvalidProgramOutput => "semantic.invalid_program_output",
    }
}

pub(crate) fn feature_code(e: &GenerationFeature) -> &'static str {
    match e {
        GenerationFeature::Instructions => "admission.instructions",
        GenerationFeature::OrderedInstructions => "admission.ordered_instructions",
        GenerationFeature::Temperature => "admission.temperature",
        GenerationFeature::MaxOutputTokens => "admission.max_output_tokens",
        GenerationFeature::Tools => "admission.tools",
        GenerationFeature::ProviderObservation => "admission.provider_observation",
        GenerationFeature::TextMetadata => "admission.text_metadata",
        GenerationFeature::Controls => "admission.controls",
        GenerationFeature::ParallelTools => "admission.parallel_tools",
        GenerationFeature::StrictTools => "admission.strict_tools",
        GenerationFeature::ToolExamples => "admission.tool_examples",
        GenerationFeature::ToolChanges => "admission.tool_changes",
        GenerationFeature::ToolSchemaNormalization => "admission.tool_schema_normalization",
        GenerationFeature::ExplicitCache => "admission.explicit_cache",
        GenerationFeature::CachedContext => "admission.cached_context",
        GenerationFeature::Safety => "admission.safety",
        GenerationFeature::AutomaticPrefixCache => "admission.automatic_prefix_cache",
        GenerationFeature::StructuredArguments => "admission.structured_arguments",
        GenerationFeature::StructuredToolResult => "admission.structured_tool_result",
        GenerationFeature::ToolExecutionReport => "admission.tool_execution_report",
        GenerationFeature::ToolResultError => "admission.tool_result_error",
        GenerationFeature::ToolResultMedia => "admission.tool_result_media",
        GenerationFeature::StructuredOutput => "admission.structured_output",
        GenerationFeature::Reasoning => "admission.reasoning",
        GenerationFeature::ResponseEffort => "admission.response_effort",
        GenerationFeature::Compaction => "admission.compaction",
        GenerationFeature::ImageInput => "admission.image_input",
        GenerationFeature::AudioInput => "admission.audio_input",
        GenerationFeature::VideoInput => "admission.video_input",
        GenerationFeature::AudioOutput => "admission.audio_output",
        GenerationFeature::AudioHistory => "admission.audio_history",
        GenerationFeature::FileInput => "admission.file_input",
        GenerationFeature::DocumentContext => "admission.document_context",
    }
}

fn context_reason_code(e: &ContextReason) -> &'static str {
    match e {
        ContextReason::Conversation => "context.invalid.conversation",
        ContextReason::CacheOptions => "context.invalid.cache_options",
        ContextReason::MaxToolCalls => "context.invalid.max_tool_calls",
        ContextReason::SystemFingerprint => "context.invalid.system_fingerprint",
        ContextReason::InstructionEchoes => "context.invalid.instruction_echoes",
        ContextReason::CompletionTimestamp => "context.invalid.completion_timestamp",
        ContextReason::StreamOptions => "context.invalid.stream_options",
        ContextReason::StreamOptionsWithoutStreaming => {
            "context.invalid.stream_options_without_streaming"
        }
    }
}

fn protocol_code(e: &ProtocolReason) -> &'static str {
    match e {
        ProtocolReason::AttemptAlreadyBegan => "execution.attempt_already_began",
        ProtocolReason::AttemptIsClosed => "execution.attempt_is_closed",
        ProtocolReason::AttemptNotFinished => "execution.attempt_not_finished",
        ProtocolReason::BodySerialization => "execution.body_serialization",
        ProtocolReason::DeliveryClosed => "execution.delivery_closed",
        ProtocolReason::EmbeddingAdmission => "execution.embedding_admission",
        ProtocolReason::EmbeddingBindingMismatch => "execution.embedding_binding_mismatch",
        ProtocolReason::EmbeddingSerialization => "execution.embedding_serialization",
        ProtocolReason::Entropy => "execution.entropy",
        ProtocolReason::GatewayClock => "execution.gateway_clock",
        ProtocolReason::ImageBindingMismatch => "execution.image_binding_mismatch",
        ProtocolReason::ImageSerialization => "execution.image_serialization",
        ProtocolReason::InvalidEndpoint => "execution.invalid_endpoint",
        ProtocolReason::MissingResponseFidelity => "execution.missing_response_fidelity",
        ProtocolReason::MissingResponseMetadata => "execution.missing_response_metadata",
        ProtocolReason::ProviderEndpointMismatch => "execution.provider_endpoint_mismatch",
        ProtocolReason::PublicModelMismatch => "execution.public_model_mismatch",
        ProtocolReason::RejectedAttempt => "execution.rejected_attempt",
        ProtocolReason::SpeechAdmission => "execution.speech_admission",
        ProtocolReason::SpeechBindingMismatch => "execution.speech_binding_mismatch",
        ProtocolReason::SpeechSerialization => "execution.speech_serialization",
        ProtocolReason::TranscriptionBinding => "execution.transcription_binding",
        ProtocolReason::TranscriptionSerialization => "execution.transcription_serialization",
        ProtocolReason::UndeclaredOperation => "execution.undeclared_operation",
        ProtocolReason::UnexpectedResponseMediaType => "execution.unexpected_response_media_type",
        ProtocolReason::ResponseLength => "execution.response_length",
    }
}

fn delivery_code(e: &DeliveryReason) -> &'static str {
    match e {
        DeliveryReason::BodyConsumerClosed => "delivery.body_consumer_closed",
        DeliveryReason::PendingContentNeedsDrain => "delivery.pending_content_needs_drain",
        DeliveryReason::AttemptChanged => "delivery.attempt_changed",
        DeliveryReason::DeliveryNotComplete => "delivery.delivery_not_complete",
        DeliveryReason::EndpointCannotStream => "delivery.endpoint_cannot_stream",
        DeliveryReason::MissingDeliveryIdentity => "delivery.missing_delivery_identity",
        DeliveryReason::MissingStreamEncoder => "delivery.missing_stream_encoder",
        DeliveryReason::NoDeliverableOutput => "delivery.no_deliverable_output",
        DeliveryReason::PaddingConsumed => "delivery.padding_consumed",
        DeliveryReason::StreamAlreadyStarted => "delivery.stream_already_started",
        DeliveryReason::TerminalProjectionSourceChanged => {
            "delivery.terminal_projection_source_changed"
        }
        DeliveryReason::TerminalRequiresValidatedUpstreamEOF => {
            "delivery.terminal_requires_validated_upstream_eof"
        }
        DeliveryReason::UpstreamWasNotAStream => "delivery.upstream_was_not_a_stream",
    }
}

fn deadline_code(e: &DeadlineReason) -> &'static str {
    match e {
        DeadlineReason::RequestBody => "deadline.request_body",
        DeadlineReason::Attempt => "deadline.attempt",
        DeadlineReason::Exchange => "deadline.exchange",
    }
}

fn lifecycle_code(e: &LifecycleError) -> &'static str {
    match e {
        LifecycleError::OutputAfterTerminal => "lifecycle.output_after_terminal",
        LifecycleError::AlreadyTerminal => "lifecycle.already_terminal",
    }
}

fn recovery_code(e: &RecoveryClass) -> &'static str {
    match e {
        RecoveryClass::CredentialUnavailable => "recovery.credential_unavailable",
        RecoveryClass::Authentication => "recovery.authentication",
        RecoveryClass::PermissionDenied => "recovery.permission_denied",
        RecoveryClass::InvalidRequest => "recovery.invalid_request",
        RecoveryClass::NotFound => "recovery.not_found",
        RecoveryClass::Redirect => "recovery.redirect",
        RecoveryClass::RateLimit => "recovery.rate_limit",
        RecoveryClass::Quota => "recovery.quota",
        RecoveryClass::Server => "recovery.server",
        RecoveryClass::Timeout => "recovery.timeout",
        RecoveryClass::Upstream => "recovery.upstream",
    }
}

fn representation_code(e: &RepresentationError) -> &'static str {
    match e {
        RepresentationError::ProjectionLimit => "representation.projection_limit",
        RepresentationError::Admission(e) => feature_code(e),
        RepresentationError::Semantic(e) => semantic_code(e),
        RepresentationError::Event(e) => event_code(e),
        RepresentationError::UsageDetails => "representation.usage_details",
        RepresentationError::ReportedFacts => "representation.reported_facts",
        RepresentationError::TextMetadata => "representation.text_metadata",
        RepresentationError::InteractionProgress => "representation.interaction_progress",
        RepresentationError::Controls => "representation.controls",
        RepresentationError::Instructions => "representation.instructions",
        RepresentationError::Temperature => "representation.temperature",
        RepresentationError::MaxOutputTokens => "representation.max_output_tokens",
        RepresentationError::Tools => "representation.tools",
        RepresentationError::ParallelTools => "representation.parallel_tools",
        RepresentationError::StrictTools => "representation.strict_tools",
        RepresentationError::StrictDefault => "representation.strict_default",
        RepresentationError::StructuredOutput => "representation.structured_output",
        RepresentationError::Reasoning => "representation.reasoning",
        RepresentationError::ReplayPhase => "representation.replay_phase",
        RepresentationError::ReplayFormat => "representation.replay_format",
        RepresentationError::ReplayOrigin => "representation.replay_origin",
        RepresentationError::ImageInput => "representation.image_input",
        RepresentationError::AudioInput => "representation.audio_input",
        RepresentationError::FileInput => "representation.file_input",
        RepresentationError::UnmigratedSemantic => "representation.unmigrated_semantic",
        RepresentationError::MessageGrouping => "representation.message_grouping",
        RepresentationError::Lifecycle => "representation.lifecycle",
        RepresentationError::Terminal => "representation.terminal",
        RepresentationError::Safety => "representation.safety",
        RepresentationError::UsageProjection => "representation.usage_projection",
        RepresentationError::Metadata => "representation.metadata",
    }
}
