//! Response metadata and reported execution context shared by both OpenAI profiles.
use crate::protocol::openai::{CodecError, Profile, ResponseMetadata, common::*};
use crate::semantic::task::generation::*;
use serde_json::{Map, Value};

pub(super) fn metadata(
    o: &Map<String, Value>,
    profile: Profile,
    adaptation: &crate::protocol::openai::dialect::adaptation::Adaptation,
    fidelity: &mut crate::protocol::fidelity::FidelityRecords,
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
    let created = crate::protocol::openai::responses::envelope::timestamp(
        o.get(created_key)
            .ok_or(CodecError::Invalid("created time"))?,
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
                    v.as_str()
                        .map(str::to_owned)
                        .ok_or(CodecError::Invalid("system fingerprint"))
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
                return Err(CodecError::Unsupported("reasoning mapping scope".into()));
            }
            let mapped = mapping.decode(control.level)?;
            if mapped != control.level {
                settings.effort = crate::semantic::value::Presence::Value(EffortControl {
                    scope: control.scope,
                    level: mapped,
                });
                settings.validate()?;
                fidelity.record_effort_mapping(mapping.rule, control.level.into(), mapped.into());
            }
        }
    }
    if profile == Profile::Chat {
        crate::protocol::openai::chat::envelope::validate_context(&context.execution)?;
    }
    Ok(ResponseMetadata {
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
