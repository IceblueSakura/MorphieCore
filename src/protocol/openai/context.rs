//! Reported context and the executed-context wire codec shared by both OpenAI profiles.
//! A context hint is never a task instruction, and each profile owns its field set.
use crate::protocol::failure::{CarrierReason, ShapeReason};
use crate::protocol::openai::{CodecError, Profile, ResponseMetadata, shape::*};
use crate::semantic::{
    context::{CacheHints, ExecutionHints},
    task::generation::*,
};
use serde_json::{Map, Number, Value};

pub(super) fn metadata(
    o: &Map<String, Value>,
    profile: Profile,
    adaptation: &crate::protocol::openai::dialect::adaptation::Adaptation,
    evidence: &mut crate::lowering::generation::EvidenceDelta,
    boundary: crate::lowering::generation::ConversionBoundary,
) -> Result<ResponseMetadata, CodecError> {
    let created_key = if profile == Profile::Chat {
        "created"
    } else {
        "created_at"
    };
    let id = text(string(o, "id")?, "response id", 256)?
        .as_str()
        .to_owned();
    let model = text(string(o, "model")?, "response model", 256)?
        .as_str()
        .to_owned();
    let created = timestamp(
        o.get(created_key)
            .ok_or(CodecError::Invalid(ShapeReason::Metadata))?,
    )?;
    let (mut context, instruction_fidelity) = if profile == Profile::Responses {
        crate::semantic::context::ResponseContext::read(o, DecodeDirection::Upstream)?
    } else {
        (
            crate::semantic::context::ResponseContext {
                reports: crate::semantic::context::ResponseReports::single(
                    crate::semantic::context::ResponseReport {
                        service_class: crate::protocol::openai::service::read_class(
                            o,
                            Profile::Chat,
                        )?,
                        ..Default::default()
                    },
                ),
                execution: crate::semantic::context::ExecutionHints::read(
                    o,
                    Profile::Chat,
                    DecodeDirection::Upstream,
                )?,
                system_fingerprint: read_presence(o, "system_fingerprint", |v| {
                    v.as_str().map(str::to_owned).ok_or(CodecError::invalid())
                })?,
                ..Default::default()
            },
            Default::default(),
        )
    };
    context.validate()?;
    if let Some(mapping) = adaptation.effort_mapping {
        mapping.validate()?;
        if let Some(settings) = context
            .reports
            .latest_mut()
            .and_then(|r| r.settings.as_mut())
            && let Some(control) = settings.effort.value().copied()
        {
            if control.scope != mapping.scope {
                return Err(CodecError::Unsupported(CarrierReason::Reasoning));
            }
            let mapped = mapping.decode(control.level)?;
            if mapped != control.level {
                settings.effort = crate::semantic::value::Presence::Value(EffortControl {
                    scope: control.scope,
                    level: mapped,
                });
                settings.validate()?;
                evidence.record_effort_mapping(
                    mapping.rule,
                    control,
                    EffortControl {
                        scope: control.scope,
                        level: mapped,
                    },
                    boundary,
                    crate::lowering::generation::Owner::Root,
                );
            }
        }
    }
    if profile == Profile::Chat {
        crate::protocol::openai::chat::envelope::validate_context(&context.execution)?;
    }
    Ok(ResponseMetadata {
        delivery: None,
        storage: if profile == Profile::Responses {
            crate::protocol::openai::responses::managed::read_storage(o)?
        } else {
            None
        },
        id: Some(id),
        model,
        created: Some(created),
        context,
        instruction_fidelity,
    })
}

pub(in crate::protocol::openai) const CACHE_FIELDS: &[&str] = &[
    "prompt_cache_key",
    "prompt_cache_retention",
    "prompt_cache_options",
    "safety_identifier",
    "user",
];
impl CacheHints {
    pub(in crate::protocol::openai) fn write(
        &self,
        o: &mut Map<String, Value>,
    ) -> Result<(), CodecError> {
        self.validate()?;
        let Value::Object(v) = serde_json::to_value(self).map_err(|_| CodecError::invalid())?
        else {
            unreachable!()
        };
        o.extend(v);
        Ok(())
    }
}

pub(in crate::protocol::openai) const EXEC_FIELDS: &[&str] = &[
    "metadata",
    "service_tier",
    "max_tool_calls",
    "store",
    "background",
    "previous_response_id",
    "conversation",
    "prompt",
    "moderation",
    "context_management",
];
pub(in crate::protocol::openai) fn check_access_programs(
    o: &Map<String, Value>,
) -> Result<(), CodecError> {
    if o.get("access_programs").is_some_and(|v| !v.is_null()) {
        return Err(CodecError::Unsupported(CarrierReason::Unavailable));
    }
    Ok(())
}
impl ExecutionHints {
    pub(in crate::protocol::openai) fn read(
        o: &Map<String, Value>,
        profile: Profile,
        direction: DecodeDirection<'_>,
    ) -> Result<Self, CodecError> {
        if profile == Profile::Responses {
            check_access_programs(o)?;
        }
        let mut fields: Map<_, _> = o
            .iter()
            .filter(|(k, _)| {
                k.as_str() != "service_tier"
                    && (CACHE_FIELDS.contains(&k.as_str())
                        // Chat `store` persists completions; only false/null is admitted.
                        || if profile == Profile::Chat {
                            matches!(k.as_str(), "metadata" | "store")
                        } else {
                            EXEC_FIELDS.contains(&k.as_str())
                        })
            })
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        // Reports may echo null, but request cache-options containers are non-nullable.
        if matches!(
            direction,
            DecodeDirection::Request | DecodeDirection::Client(_)
        ) && fields
            .get("prompt_cache_options")
            .is_some_and(Value::is_null)
        {
            return Err(CodecError::invalid());
        }
        if let Some(options) = fields
            .get_mut("prompt_cache_options")
            .filter(|v| !v.is_null())
        {
            let options = options.as_object_mut().ok_or(CodecError::invalid())?;
            let allowed: &[&str] = if profile == Profile::Chat {
                if direction.client()
                    && ["comparison_response_id", "prewarm"]
                        .iter()
                        .any(|key| options.get(*key).is_some_and(|v| !v.is_null()))
                {
                    return Err(CodecError::Unsupported(CarrierReason::Unavailable));
                }
                &["mode", "ttl"]
            } else {
                &["mode", "ttl", "comparison_response_id", "prewarm"]
            };
            direction.fields(options, allowed)?;
            options.retain(|key, _| allowed.contains(&key.as_str()));
        }
        let hints: Self =
            serde_json::from_value(Value::Object(fields)).map_err(|_| CodecError::invalid())?;
        hints.validate()?;
        validate_identity(&hints.identity)?;
        Ok(hints)
    }
    pub(in crate::protocol::openai) fn write(
        &self,
        o: &mut Map<String, Value>,
    ) -> Result<(), CodecError> {
        self.validate()?;
        validate_identity(&self.identity)?;
        let Value::Object(v) = serde_json::to_value(self).map_err(|_| CodecError::invalid())?
        else {
            unreachable!()
        };
        o.extend(v);
        Ok(())
    }
}

pub(in crate::protocol::openai) fn timestamp(v: &Value) -> Result<Number, CodecError> {
    match v {
        Value::Number(n) if crate::semantic::value::valid_timestamp(n) => Ok(n.clone()),
        _ => Err(CodecError::Invalid(ShapeReason::Metadata)),
    }
}

pub(crate) fn validate_identity(
    identity: &crate::semantic::context::ClientIdentityHints,
) -> Result<(), CodecError> {
    identity.validate()?;
    for (value, maximum) in [(&identity.user, 256), (&identity.safety_identifier, 128)] {
        if value
            .value()
            .is_some_and(|value| value.chars().count() > maximum || value.len() > maximum * 4)
        {
            return Err(CodecError::Limit);
        }
    }
    Ok(())
}
