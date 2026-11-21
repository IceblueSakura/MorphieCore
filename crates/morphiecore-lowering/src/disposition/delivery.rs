//! Delivery facts and their named projection effects.

use super::{Disposition, DispositionKey, DispositionTable, Rule};
use crate::conversion::generation::{Change, Field};
use strum::{EnumCount, EnumIter, IntoEnumIterator};

#[derive(Clone, Copy, Debug, Eq, PartialEq, EnumCount, EnumIter)]
pub enum DeliveryFact {
    Reasoning,
    Phase,
    ExecutionHints,
    Settings,
    Fingerprint,
    Annotations,
    Probabilities,
    TerminalExplanation,
    Refusal,
    OpeningService,
    CallIdentity,
}

impl DispositionKey for DeliveryFact {
    fn index(self) -> usize {
        self as usize
    }
}

impl DeliveryFact {
    pub fn name(self) -> &'static str {
        match self {
            Self::Reasoning => "reasoning",
            Self::Phase => "message_phase",
            Self::ExecutionHints => "execution_hints",
            Self::Settings => "reported_settings",
            Self::Fingerprint => "system_fingerprint",
            Self::Annotations => "annotations",
            Self::Probabilities => "probabilities",
            Self::TerminalExplanation => "terminal_explanation",
            Self::Refusal => "refusal",
            Self::OpeningService => "opening_service_report",
            Self::CallIdentity => "call_identity",
        }
    }
}

pub type DeliveryDispositions = DispositionTable<DeliveryFact, { DeliveryFact::COUNT }>;

impl DeliveryDispositions {
    pub fn from_facts(mut disposition: impl FnMut(DeliveryFact) -> Disposition) -> Self {
        let mut table = Self::filled(Disposition::Native);
        for fact in DeliveryFact::iter() {
            table.set(fact, disposition(fact));
        }
        table
    }

    pub fn rules(&self) -> impl Iterator<Item = (DeliveryFact, Rule)> {
        DeliveryFact::iter().filter_map(|fact| self.get(fact).rule().map(|rule| (fact, rule)))
    }
}

impl Rule {
    /// Messages call IDs must match `^[a-zA-Z0-9_-]{1,64}$`; other IDs map to
    /// one stable digest in both request and delivery directions.
    pub const MESSAGES_PROJECT_CALL_IDS: Self = Self::new(
        "messages.project-call-ids.v1",
        Field::ToolCallIdentity,
        Change::Normalize,
    );
    pub const MESSAGES_REFUSAL_AS_TEXT: Self = Self::new(
        "messages.refusal-as-text.v1",
        Field::ContentRefusal,
        Change::Approximate,
    );
    pub const OMIT_CHAT_OPENING_SERVICE: Self = Self::new(
        "chat.omit-opening-service-report.v1",
        Field::OpeningReport,
        Change::Omit,
    );
    pub const OMIT_CHAT_READABLE_REASONING: Self = Self::new(
        "chat.omit-readable-reasoning.v1",
        Field::DeliveredProjection,
        Change::Omit,
    );
    pub const OMIT_DELIVERY_MESSAGE_PHASE: Self = Self::new(
        "generation.omit-delivery-message-phase.v1",
        Field::MessagePhase,
        Change::Omit,
    );
    pub const OMIT_MESSAGES_EXECUTION_HINTS: Self = Self::new(
        "messages.omit-reported-execution-hints.v1",
        Field::Representation,
        Change::Omit,
    );
    pub const OMIT_CHAT_STREAM_EXECUTION_HINTS: Self = Self::new(
        "chat.omit-stream-execution-hints.v1",
        Field::Representation,
        Change::Omit,
    );
    pub const OMIT_CHAT_UNSUPPORTED_EXECUTION_HINTS: Self = Self::new(
        "chat.omit-unsupported-execution-hints.v1",
        Field::Representation,
        Change::Omit,
    );
    pub const OMIT_MESSAGES_INCIDENTAL_SETTINGS: Self = Self::new(
        "messages.omit-incidental-settings.v1",
        Field::ReportedSettings,
        Change::Omit,
    );
    pub const OMIT_CHAT_DELIVERED_SETTINGS: Self = Self::new(
        "chat.omit-reported-settings.v1",
        Field::ReportedSettings,
        Change::Omit,
    );
    pub const OMIT_MESSAGES_SYSTEM_FINGERPRINT: Self = Self::new(
        "messages.omit-system-fingerprint.v1",
        Field::SystemFingerprint,
        Change::Omit,
    );
    pub const OMIT_CHAT_STREAM_ANNOTATIONS: Self = Self::new(
        "chat.omit-stream-annotations.v1",
        Field::Representation,
        Change::Omit,
    );
    pub const OMIT_MESSAGES_ANNOTATIONS: Self = Self::new(
        "messages.omit-annotations.v1",
        Field::Representation,
        Change::Omit,
    );
    pub const OMIT_MESSAGES_PROBABILITIES: Self = Self::new(
        "messages.omit-probability-reports.v1",
        Field::Representation,
        Change::Omit,
    );
    pub const OMIT_MESSAGES_TERMINAL_EXPLANATION: Self = Self::new(
        "messages.omit-terminal-explanation.v1",
        Field::Representation,
        Change::Omit,
    );
    pub const OMIT_RESPONSES_UNREPRESENTED_INCOMPLETE_REASON: Self = Self::new(
        "responses.omit-unrepresented-incomplete-reason.v1",
        Field::Incomplete,
        Change::Omit,
    );
}
