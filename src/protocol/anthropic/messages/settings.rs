//! Shared wire↔IR setting conversions used by native intake and client admission.
use crate::semantic::{task::generation::*, value::Presence};

/// Messages has one summarized display; the IR keeps the client's distinction.
pub(in crate::protocol::anthropic::messages) fn display(
    value: &Presence<crate::protocol::anthropic::messages::Display>,
) -> Presence<ReasoningDisplay> {
    match value {
        Presence::Absent => Presence::Absent,
        Presence::Null => Presence::Null,
        Presence::Value(crate::protocol::anthropic::messages::Display::Summarized) => {
            Presence::Value(ReasoningDisplay::Summary)
        }
        Presence::Value(crate::protocol::anthropic::messages::Display::Omitted) => {
            Presence::Value(ReasoningDisplay::Omitted)
        }
    }
}

/// Effort and format of a validated Messages `output_config`.
/// Unresolved null-member semantics reject before optional owners are projected.
pub(in crate::protocol::anthropic::messages) fn output_settings(
    config: &crate::protocol::anthropic::messages::OutputConfig,
    settings: &mut GenerationSettings,
) {
    if let Presence::Value(effort) = config.effort {
        settings.effort = Some(EffortControl {
            scope: EffortScope::Response,
            level: effort_level(effort),
        });
    }
    if let Presence::Value(schema) = &config.format {
        // Messages output always conforms to the schema, as OpenAI strict output does.
        settings.text.presence = Presence::Value(());
        settings.text.format = Presence::Value(OutputConstraint::JsonSchema {
            name: None,
            description: None,
            schema: schema.clone(),
            strict: Some(true),
        });
    }
}
pub(in crate::protocol::anthropic::messages) fn replay_mismatch(
    behavior: crate::protocol::anthropic::messages::PrefixMismatch,
) -> ReplayMismatch {
    match behavior {
        super::PrefixMismatch::Error => ReplayMismatch::Reject,
        super::PrefixMismatch::DropBlock => ReplayMismatch::Drop,
    }
}
pub(in crate::protocol::anthropic::messages) fn effort_level(
    effort: crate::protocol::anthropic::messages::Effort,
) -> EffortLevel {
    match effort {
        super::Effort::Low => EffortLevel::Low,
        super::Effort::Medium => EffortLevel::Medium,
        super::Effort::High => EffortLevel::High,
        super::Effort::XHigh => EffortLevel::XHigh,
        super::Effort::Max => EffortLevel::Max,
    }
}
