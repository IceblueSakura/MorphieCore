//! Function-call content carriers shared by Chat and Responses.
use crate::protocol::openai::{CodecError, Profile, shape::*};
use crate::semantic::task::generation::*;
use serde_json::{Map, Value};

pub(in crate::protocol::openai) fn strict_default(profile: Profile) -> StrictDefault {
    match profile {
        Profile::Chat => StrictDefault::NonStrict,
        Profile::Responses => StrictDefault::NormalizeSchema,
    }
}
pub(in crate::protocol::openai) fn reject_chat_orchestration(
    o: &Map<String, Value>,
) -> Result<(), CodecError> {
    if let Some(key) = [
        "async",
        "defer_loading",
        "allowed_callers",
        "caller",
        "namespace",
    ]
    .into_iter()
    .find(|key| o.get(*key).is_some_and(|v| !v.is_null()))
    {
        return Err(CodecError::Unsupported(key.into()));
    }
    Ok(())
}
pub(in crate::protocol::openai) fn tool_call(
    o: &Map<String, Value>,
    profile: Profile,
    item_status: Option<&str>,
    replay: bool,
    direction: DecodeDirection,
) -> Result<ToolCall, CodecError> {
    let (f, id) = match profile {
        Profile::Chat => {
            reject_chat_orchestration(o)?;
            direction.fields(o, &["id", "type", "function", "index"])?;
            if string(o, "type")? != "function" {
                return Err(CodecError::Unsupported("tool kind".into()));
            }
            let f = object(o.get("function").ok_or(CodecError::Invalid("function"))?)?;
            reject_chat_orchestration(f)?;
            direction.fields(
                f,
                if replay {
                    &["name", "arguments", "parsed_arguments"]
                } else {
                    &["name", "arguments"]
                },
            )?;
            (f, string(o, "id")?)
        }
        Profile::Responses => {
            direction.fields(
                o,
                if replay {
                    &[
                        "id",
                        "type",
                        "call_id",
                        "name",
                        "arguments",
                        "parsed_arguments",
                        "status",
                        "caller",
                        "namespace",
                        "async",
                    ]
                } else {
                    &[
                        "id",
                        "type",
                        "call_id",
                        "name",
                        "arguments",
                        "status",
                        "caller",
                        "namespace",
                        "async",
                    ]
                },
            )?;
            if let Some(status) = item_status {
                accept_status(o, status)?;
            }
            (o, string(o, "call_id")?)
        }
    };
    // The pinned SDK derives this view from the authoritative raw arguments; wire
    // positions never carry it and history replay drops it after validation.
    if replay {
        admit_parsed(
            "parsed_arguments",
            f.get("parsed_arguments"),
            Some(string(f, "arguments")?),
        )?;
    }
    // Function-call status is optional/nullable, unlike complete message status.
    // https://github.com/openai/openai-python/blob/be9d66628ad7377bd36fe5a76ae6d735843f0e76/src/openai/types/responses/response_function_tool_call.py
    let status = if profile == Profile::Chat || o.get("status").is_some_and(Value::is_null) {
        ItemLifecycle::Completed
    } else {
        crate::protocol::openai::responses::items::status(o, ItemLifecycle::Completed)?
    };
    Ok(ToolCall {
        replay: None,
        call_id: text(id, "call_id", 256)?,
        cache: None,
        name: text(string(f, "name")?, "function name", 128)?,
        arguments: raw_string(f, "arguments")?.into(),
        status,
        context: if profile == Profile::Responses {
            crate::protocol::openai::responses::items::call_context(o, direction)?
        } else {
            CallContext::default()
        },
    })
}
