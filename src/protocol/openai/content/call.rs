//! Function-call content carriers shared by Chat and Responses.
use crate::{
    protocol::{
        failure::CarrierReason,
        openai::{CodecError, GenerationApi, shape::*},
    },
    semantic::task::generation::*,
};
use serde_json::{Map, Value};

pub(in crate::protocol::openai) fn strict_default(profile: GenerationApi) -> StrictDefault {
    match profile {
        GenerationApi::Chat => StrictDefault::NonStrict,
        GenerationApi::Responses => StrictDefault::NormalizeSchema,
    }
}
pub(in crate::protocol::openai) fn reject_chat_orchestration(
    o: &Map<String, Value>,
) -> Result<(), CodecError> {
    if let Some(_key) = [
        "async",
        "defer_loading",
        "allowed_callers",
        "caller",
        "namespace",
    ]
    .into_iter()
    .find(|key| o.get(*key).is_some_and(|v| !v.is_null()))
    {
        return Err(CodecError::Unsupported(CarrierReason::UnknownField));
    }
    Ok(())
}
pub(in crate::protocol::openai) fn tool_call(
    o: &Map<String, Value>,
    profile: GenerationApi,
    item_status: Option<&str>,
    replay: bool,
    direction: DecodeDirection,
) -> Result<ToolCall, CodecError> {
    let (f, id) = match profile {
        GenerationApi::Chat => {
            if !direction.client() {
                reject_chat_orchestration(o)?;
            }
            direction.fields(o, &["id", "type", "function"])?;
            if string(o, "type")? != "function" {
                return Err(CodecError::Unsupported(CarrierReason::Tools));
            }
            let f = object(o.get("function").ok_or(CodecError::invalid())?)?;
            if !direction.client() {
                reject_chat_orchestration(f)?;
            }
            direction.fields(f, &["name", "arguments"])?;
            (f, string(o, "id")?)
        }
        GenerationApi::Responses => {
            direction.fields(
                o,
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
                ],
            )?;
            if let Some(status) = item_status {
                accept_status(o, status)?;
            }
            (o, string(o, "call_id")?)
        }
    };
    if replay && profile == GenerationApi::Responses {
        for key in ["id", "status", "namespace", "async"] {
            if o.get(key).is_some_and(Value::is_null) {
                return Err(CodecError::invalid());
            }
        }
    }
    // Request status is optional but non-nullable; upstream reports also admit null.
    // https://github.com/openai/openai-python/blob/e8a0fd28bbdc1699c97ff0f602b1af3dd378382d/src/openai/types/responses/response_function_tool_call_param.py
    let status = if profile == GenerationApi::Chat || o.get("status").is_some_and(Value::is_null) {
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
        context: if profile == GenerationApi::Responses {
            crate::protocol::openai::responses::items::call_context(o, direction)?
        } else {
            CallContext::default()
        },
    })
}
