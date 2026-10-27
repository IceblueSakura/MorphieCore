//! Pure source-error meaning and closed conversion rejection vocabulary.
use crate::protocol::conversion::generation::Field;
use serde::Serialize;

/// Closed vocabulary of structural wire framing and snapshot shape validation failures.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ShapeReason {
    /// JSON syntax or deserialization failure.
    Json,
    /// Expected a JSON object container.
    Object,
    /// Invalid or conflicting top-level response metadata.
    Metadata,
    /// Response ID mutated across stream frames or snapshots.
    MetadataIdChanged,
    /// Model identifier mutated across stream frames or snapshots.
    MetadataModelChanged,
    /// Creation timestamp mutated across stream frames or snapshots.
    MetadataCreatedChanged,
    /// Invalid choices container or cardinality.
    Choices,
    /// Tool call identifier is malformed or missing.
    CallIdentity,
    /// Content delta framing violation.
    Delta,
    /// Usage reported after terminal EOF.
    UsageTail,
    /// Usage reported prematurely before initial content.
    EarlyUsage,
    /// Event received after stream has already completed or finished.
    AfterFinish,
    /// Event sequence ordering violation.
    Sequence,
    /// Conflicting request or response generation settings.
    SettingsConflict,
    /// Configuration update conflicts with admitted request state.
    ConfigurationConflict,
    /// Invalid item snapshot framing.
    ItemSnapshot,
    /// Invalid terminal response snapshot framing.
    TerminalSnapshot,
    /// Snapshot item container shape violation.
    SnapshotShape,
    /// Snapshot item identifier disagreement.
    SnapshotIdentity,
    /// Snapshot lifecycle state conflict.
    SnapshotLifecycle,
    /// Snapshot message phase conflict.
    SnapshotPhase,
    /// Snapshot replay attachment violation.
    SnapshotReplay,
    /// Replay attachment unexpectedly added to existing item.
    SnapshotReplayAdded,
    /// Replay attachment unexpectedly removed from existing item.
    SnapshotReplayRemoved,
    /// Replay attachment mutated from previously published snapshot.
    SnapshotReplayChanged,
    /// Snapshot reasoning summary conflict.
    SnapshotSummary,
    /// Snapshot text content conflict.
    SnapshotText,
    /// Snapshot history annotations conflict.
    SnapshotAnnotations,
    /// Snapshot probability presence conflict.
    SnapshotProbabilityPresence,
    /// Snapshot probabilities values conflict.
    SnapshotProbabilities,
    /// Snapshot value shape violation.
    ValueSnapshot,
    /// Reasoning control shape violation.
    Reasoning,
    /// Log probabilities shape violation.
    Probabilities,
    /// General item shape violation.
    Item,
    /// Client identity hint shape violation.
    Identity,
    /// Usage breakdown shape violation.
    Usage,
    /// Generic invalid field value.
    InvalidValue,
    /// Invalid response item status.
    Status,
    /// Invalid terminal outcome status.
    TerminalStatus,
    /// Invalid terminal explanation.
    TerminalExplanation,
    /// Invalid cryptographic signature framing.
    Signature,
    /// Signature payload corrupted or unparseable.
    SignaturePayload,
}
impl std::fmt::Display for ShapeReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if matches!(self, Self::Json) {
            f.write_str("JSON")
        } else {
            write!(f, "{self:?}")
        }
    }
}
/// Closed vocabulary of semantic carrier absence or incompatibility rejections.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CarrierReason {
    /// Target has no schema carrier for structured output.
    Schema,
    /// Target cannot represent citation coordinate metadata.
    CitationMetadata,
    /// Target cannot represent URL context metadata.
    UrlContextMetadata,
    /// Target has no carrier for mean log probabilities.
    MeanLogprob,
    /// Target has no carrier for summed log probabilities.
    SumLogprob,
    /// Target cannot carry explicit token IDs.
    TokenId,
    /// Target cannot represent step-level error status.
    StepError,
    /// Target cannot represent rich history annotations.
    Annotations,
    /// Target cannot carry multipart message contents.
    MultipartContent,
    /// Target has an unrepresented unknown field without extension carrier.
    UnknownField,
    /// Target feature is unavailable or disabled.
    Unavailable,
    /// Target cannot represent the requested terminal outcome.
    Terminal,
    /// Target has no carrier for detailed usage breakdowns.
    Usage,
    /// Target lacks a carrier for the requested replay format.
    Replay,
    /// Target cannot represent the requested reasoning controls or tokens.
    Reasoning,
    /// Target lacks a tool or function calling carrier.
    Tools,
    /// Target has no carrier for safety ratings or assessments.
    Safety,
    /// Target cannot represent request or response metadata.
    Metadata,
    /// Target cannot represent the specified domain field.
    Field(Field),
}
impl std::fmt::Display for CarrierReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Resource {
    Bytes,
    Depth,
    Nodes,
    Events,
    StreamState,
    Projection,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Observation {
    Pending,
    Complete,
    Interrupted,
    OverLimit,
    IntentionallyUnread,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RecognitionBasis {
    Status,
    EnvelopeCode,
    EnvelopeType,
    EnvelopeFields,
    QuotaPattern,
    ContextPattern,
    OverloadPattern,
    Unrecognized,
}
/// A protocol-generated policy hint, never an observed HTTP response head.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct ProtocolStatusHint(u16);
impl ProtocolStatusHint {
    pub const fn get(self) -> u16 {
        self.0
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct SourceFailureEvidence {
    pub kind: SourceFailureKind,
    pub basis: RecognitionBasis,
    pub status_hint: Option<ProtocolStatusHint>,
    pub observation: Observation,
}
impl std::fmt::Display for SourceFailureEvidence {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.kind)
    }
}
impl SourceFailureEvidence {
    pub fn http(status: u16, body: &[u8], observation: Observation) -> Self {
        let (kind, basis) = recognize_failure(status, body);
        Self {
            kind,
            basis,
            status_hint: None,
            observation,
        }
    }
    pub fn envelope(
        status: u16,
        hint_basis: RecognitionBasis,
        code: Option<&str>,
        kind: Option<&str>,
        message: Option<&str>,
    ) -> Self {
        // Only the resulting closed classification survives this temporary body.
        let body =
            serde_json::to_vec(&serde_json::json!({"code":code,"type":kind,"message":message}))
                .expect("error envelope");
        let mut evidence = Self::stream(status, &body);
        if evidence.basis == RecognitionBasis::Status {
            evidence.basis = hint_basis;
        }
        evidence
    }
    pub fn stream(status: u16, body: &[u8]) -> Self {
        let status = if (400..600).contains(&status) {
            status
        } else {
            500
        };
        let mut result = Self::http(status, body, Observation::Complete);
        result.status_hint = Some(ProtocolStatusHint(status));
        result
    }
}
/// What an agent loop does next with a failed turn: compact, stop, wait or retry.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceFailureKind {
    ContextOverflow,
    Quota,
    RateLimit,
    Overloaded,
    Other,
}

// Lowercase fragments of bound Providers' error codes, types and messages; the
// same fragments pi (`utils/overflow.ts`, `utils/retry.ts`) and opencode match.
const OVERFLOW: &[&str] = &[
    "context_length_exceeded",
    "context length exceeded",
    "exceeds the context window",
    "maximum context length",
    "context window exceeds limit",
    "model_context_window_exceeded",
    "prompt is too long",
    "prompt too long",
    "request_too_large",
    "input is too long",
    "maximum prompt length",
    "exceeded model token limit",
    "range of input length should be",
    // Gemini: "The input token count (N) exceeds the maximum number of tokens allowed (M)."
    "exceeds the maximum number of tokens allowed",
];
const QUOTA: &[&str] = &[
    "insufficient_quota",
    "quota exceeded",
    "billing",
    "credit balance",
    "insufficient balance",
    // OpenCode Go and Zen subscription limits (`GoUsageLimitError`).
    "usagelimiterror",
];

/// Classify a failed upstream reply from its status and the `code`, `type` and
/// `message` of an OpenAI-compatible or Anthropic error body. Overflow wording
/// counts only on request errors, so throttling text never reads as overflow.
pub fn classify_failure(status: u16, body: &[u8]) -> SourceFailureKind {
    recognize_failure(status, body).0
}
fn recognize_failure(status: u16, body: &[u8]) -> (SourceFailureKind, RecognitionBasis) {
    let value: serde_json::Value = serde_json::from_slice(body).unwrap_or_default();
    let error = match value.get("error") {
        Some(serde_json::Value::String(message)) => serde_json::json!({ "message": message }),
        Some(error) => error.clone(),
        // vLLM-style servers report the fields at the top level.
        None => value,
    };
    let text = ["code", "type", "message"]
        .iter()
        .filter_map(|key| error.get(key).and_then(serde_json::Value::as_str))
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();
    let has = |fragments: &[&str]| fragments.iter().any(|f| text.contains(f));
    let matched_basis = |fragments: &[&str], message_basis: RecognitionBasis| {
        if error
            .get("code")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|s| fragments.iter().any(|f| s.to_lowercase().contains(f)))
        {
            RecognitionBasis::EnvelopeCode
        } else if error
            .get("type")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|s| fragments.iter().any(|f| s.to_lowercase().contains(f)))
        {
            RecognitionBasis::EnvelopeType
        } else if error
            .get("message")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|s| fragments.iter().any(|f| s.to_lowercase().contains(f)))
        {
            message_basis
        } else {
            RecognitionBasis::EnvelopeFields
        }
    };
    match status {
        401 | 403 => (SourceFailureKind::Other, RecognitionBasis::Status),
        402 => (SourceFailureKind::Quota, RecognitionBasis::Status),
        400..=499 if has(QUOTA) => (
            SourceFailureKind::Quota,
            matched_basis(QUOTA, RecognitionBasis::QuotaPattern),
        ),
        429 => (SourceFailureKind::RateLimit, RecognitionBasis::Status),
        400 | 413 | 422 if has(OVERFLOW) => (
            SourceFailureKind::ContextOverflow,
            matched_basis(OVERFLOW, RecognitionBasis::ContextPattern),
        ),
        503 | 529 => (SourceFailureKind::Overloaded, RecognitionBasis::Status),
        500..=599 if has(&["overloaded"]) => (
            SourceFailureKind::Overloaded,
            matched_basis(&["overloaded"], RecognitionBasis::OverloadPattern),
        ),
        _ => (SourceFailureKind::Other, RecognitionBasis::Unrecognized),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn winning_classification_and_basis_are_one_fact() {
        let quota = SourceFailureEvidence::http(
            429,
            br#"{"error":{"code":"insufficient_quota"}}"#,
            Observation::Complete,
        );
        assert_eq!(
            (quota.kind, quota.basis),
            (SourceFailureKind::Quota, RecognitionBasis::EnvelopeCode)
        );
        assert!(quota.status_hint.is_none());
        let message = SourceFailureEvidence::envelope(
            429,
            RecognitionBasis::EnvelopeType,
            None,
            Some("rate_limit_error"),
            Some("credit balance"),
        );
        assert_eq!(
            (message.kind, message.basis),
            (SourceFailureKind::Quota, RecognitionBasis::QuotaPattern)
        );
        assert_eq!(message.status_hint.unwrap().get(), 429);
        let throttle = SourceFailureEvidence::envelope(
            429,
            RecognitionBasis::EnvelopeType,
            None,
            Some("rate_limit_error"),
            Some("synthetic"),
        );
        assert_eq!(
            (throttle.kind, throttle.basis),
            (SourceFailureKind::RateLimit, RecognitionBasis::EnvelopeType)
        );
        let auth = SourceFailureEvidence::http(
            401,
            br#"{"error":{"code":"insufficient_quota"}}"#,
            Observation::IntentionallyUnread,
        );
        assert_eq!(
            (auth.kind, auth.basis),
            (SourceFailureKind::Other, RecognitionBasis::Status)
        );
    }
}
