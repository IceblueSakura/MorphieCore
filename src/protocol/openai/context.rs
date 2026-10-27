//! Reported context and the executed-context wire codec shared by both OpenAI profiles.
//! A context hint is never a task instruction, and each profile owns its field set.
pub(in crate::protocol::openai) mod wire;
use crate::{
    protocol::{
        failure::{CarrierReason, ShapeReason},
        openai::{CodecError, GenerationApi, ResponseMetadata, shape::*},
    },
    semantic::{
        context::{CacheHints, ExecutionHints},
        task::generation::*,
    },
};
use serde_json::{Map, Number, Value};

pub(super) fn metadata(
    o: &Map<String, Value>,
    profile: GenerationApi,
    adaptation: &crate::protocol::openai::dialect::adaptation::Adaptation,
    evidence: &mut crate::protocol::conversion::generation::EvidenceDelta,
    boundary: crate::protocol::conversion::generation::ConversionBoundary,
) -> Result<ResponseMetadata, CodecError> {
    let created_key = if profile == GenerationApi::Chat {
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
    let (mut context, instruction_fidelity) = if profile == GenerationApi::Responses {
        crate::semantic::context::ResponseContext::read(o, DecodeDirection::Upstream)?
    } else {
        (
            crate::semantic::context::ResponseContext {
                reports: crate::semantic::context::ResponseReports::single(
                    crate::semantic::context::ResponseReport {
                        service_class: crate::protocol::openai::service::read_class(
                            o,
                            GenerationApi::Chat,
                        )?,
                        ..Default::default()
                    },
                ),
                execution: crate::semantic::context::ExecutionHints::read(
                    o,
                    GenerationApi::Chat,
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
                    crate::protocol::conversion::generation::Owner::Root,
                );
            }
        }
    }
    if profile == GenerationApi::Chat {
        crate::protocol::openai::chat::envelope::validate_context(&context.execution)?;
    }
    Ok(ResponseMetadata {
        delivery: None,
        storage: if profile == GenerationApi::Responses {
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
        let Value::Object(v) = serde_json::to_value(wire::CacheHints::from(self))
            .map_err(|_| CodecError::invalid())?
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
        profile: GenerationApi,
        direction: DecodeDirection<'_>,
    ) -> Result<Self, CodecError> {
        if profile == GenerationApi::Responses {
            if matches!(
                direction,
                DecodeDirection::Request | DecodeDirection::Client(_)
            ) && o.get("access_programs").is_some_and(Value::is_null)
            {
                return Err(CodecError::invalid());
            }
            check_access_programs(o)?;
        }
        let mut fields: Map<_, _> = o
            .iter()
            .filter(|(k, _)| {
                k.as_str() != "service_tier"
                    && (CACHE_FIELDS.contains(&k.as_str())
                        // Chat `store` persists completions; only false/null is admitted.
                        || if profile == GenerationApi::Chat {
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
            let allowed: &[&str] = if profile == GenerationApi::Chat {
                &["mode", "ttl"]
            } else {
                &["mode", "ttl", "comparison_response_id", "prewarm"]
            };
            direction.fields(options, allowed)?;
            options.retain(|key, _| allowed.contains(&key.as_str()));
        }
        let hints: wire::ExecutionHints =
            serde_json::from_value(Value::Object(fields)).map_err(|_| CodecError::invalid())?;
        let hints = Self::from(&hints);
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
        let Value::Object(v) = serde_json::to_value(wire::ExecutionHints::from(self))
            .map_err(|_| CodecError::invalid())?
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
    // Identity bounds are protocol-specific; the semantic owner has its own ceiling.
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::semantic::{context::*, value::Presence};
    use serde_json::json;

    #[test]
    fn context_wire_intake_preserves_explicit_presence_and_enum_meaning() {
        let value = json!({
            "prompt_cache_key":"",
            "prompt_cache_retention":"24h",
            "prompt_cache_options":{"mode":"explicit","ttl":"30m","prewarm":false,"comparison_response_id":null},
            "safety_identifier":null,"user":"","metadata":{},"store":false
        });
        let hints = ExecutionHints::read(
            value.as_object().unwrap(),
            GenerationApi::Responses,
            DecodeDirection::Request,
        )
        .unwrap();
        assert_eq!(hints.cache.prompt_cache_key, Presence::Value(String::new()));
        assert_eq!(
            hints.cache.prompt_cache_retention,
            Presence::Value(CacheRetention::Day)
        );
        let options = hints.cache.prompt_cache_options.value().unwrap();
        assert_eq!(options.mode, Presence::Value(CacheMode::Explicit));
        assert_eq!(options.ttl, Presence::Value(CacheTtl::ThirtyMinutes));
        assert_eq!(options.prewarm, Presence::Value(false));
        assert_eq!(options.comparison_response_id, Presence::Null);
        assert_eq!(hints.identity.safety_identifier, Presence::Null);
        assert!(hints.background.is_absent());
        for invalid in [
            json!({"prompt_cache_options":null}),
            json!({"prompt_cache_retention":"Day"}),
            json!({"prompt_cache_options":{"ttl":"ThirtyMinutes"}}),
        ] {
            assert!(
                ExecutionHints::read(
                    invalid.as_object().unwrap(),
                    GenerationApi::Responses,
                    DecodeDirection::Request
                )
                .is_err()
            );
        }
    }

    #[test]
    fn context_wire_output_is_explicit_and_not_semantic_serde() {
        let hints = ExecutionHints {
            cache: CacheHints {
                prompt_cache_retention: Presence::Value(CacheRetention::InMemory),
                prompt_cache_options: Presence::Value(CacheOptions {
                    mode: Presence::Value(CacheMode::Implicit),
                    ttl: Presence::Value(CacheTtl::ThirtyMinutes),
                    ..Default::default()
                }),
                ..Default::default()
            },
            identity: ClientIdentityHints {
                user: Presence::Null,
                ..Default::default()
            },
            background: Presence::Value(false),
            ..Default::default()
        };
        let mut output = Map::new();
        hints.write(&mut output).unwrap();
        assert_eq!(
            Value::Object(output),
            json!({
                "prompt_cache_retention":"in_memory",
                "prompt_cache_options":{"mode":"implicit","ttl":"30m"},
                "user":null,"background":false
            })
        );
        let report = CacheDiagnostics::CacheMiss {
            reason: CacheMissReason::ToolsChanged,
            cache_missed_tokens: 0,
            comparison_reusable_tokens: Presence::Null,
        };
        assert_eq!(
            serde_json::to_value(wire::CacheDiagnostics::from(&report)).unwrap(),
            json!({
                "type":"cache_miss","reason":"tools_changed","cache_missed_tokens":0,"comparison_reusable_tokens":null
            })
        );
    }
}
