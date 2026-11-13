//! Hosted MiniCPM activation is a template Boolean, not a thinking-budget carrier.
use crate::{
    protocol::openai::{CodecError, GenerationApi, reasoning, shape::object},
    semantic::task::generation::{
        EffortScope, GenerationSettings, ReasoningActivation, ReasoningPresence,
    },
};
use serde_json::{Map, Value, json};

pub(in crate::protocol::openai) fn read(
    template: &Value,
    settings: &mut GenerationSettings,
) -> Result<(), CodecError> {
    let Some(enabled) = object(template)?.get("enable_thinking") else {
        return Ok(());
    };
    let activation = if enabled.as_bool().ok_or(CodecError::invalid())? {
        ReasoningActivation::Enabled
    } else {
        ReasoningActivation::Disabled
    };
    if settings
        .reasoning
        .activation
        .is_some_and(|value| value != activation)
    {
        return Err(CodecError::invalid());
    }
    settings.reasoning.presence = ReasoningPresence::Present;
    settings.reasoning.activation = Some(activation);
    settings.validate().map_err(CodecError::from)?;
    Ok(())
}

pub(crate) fn standard_reasoning(settings: &GenerationSettings) -> GenerationSettings {
    let mut standard = settings.clone();
    standard.reasoning.activation = None;
    standard
}

pub(in crate::protocol::openai) fn write(
    settings: &GenerationSettings,
    output: &mut Map<String, Value>,
) -> Result<(), CodecError> {
    let activation = settings.reasoning.activation.or_else(|| {
        settings
            .effort
            .filter(|effort| effort.scope == EffortScope::Reasoning)
            .map(|_| ReasoningActivation::Enabled)
    });
    if let Some(activation) = activation {
        output.insert(
            "chat_template_kwargs".into(),
            json!({"enable_thinking":activation == ReasoningActivation::Enabled}),
        );
    }
    reasoning::write_request(&standard_reasoning(settings), output, GenerationApi::Chat)
}
