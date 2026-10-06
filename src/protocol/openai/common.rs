use super::{CodecError, Profile};
use crate::{
    protocol::fidelity::FidelityRecords,
    semantic::{task::generation::*, value::Text},
};
use serde_json::{Map, Value};
use std::io::{self, Write};

// Count serialization without allocating another full wire buffer.
pub(super) fn bounded(v: &Value) -> Result<(), CodecError> {
    struct Counter(usize);
    impl Write for Counter {
        fn write(&mut self, b: &[u8]) -> io::Result<usize> {
            self.0 = self.0.saturating_add(b.len());
            if self.0 > MAX_TOTAL_BYTES {
                return Err(io::Error::other("wire limit"));
            }
            Ok(b.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    serde_json::to_writer(Counter(0), v).map_err(|_| CodecError::Limit)
}
pub(super) fn read_presence<T>(
    o: &Map<String, Value>,
    key: &'static str,
    parse: impl FnOnce(&Value) -> Result<T, CodecError>,
) -> Result<crate::semantic::value::Presence<T>, CodecError> {
    use crate::semantic::value::Presence;
    match o.get(key) {
        None => Ok(Presence::Absent),
        Some(Value::Null) => Ok(Presence::Null),
        Some(v) => parse(v).map(Presence::Value),
    }
}
pub(super) fn put_presence<T>(
    o: &mut Map<String, Value>,
    key: &str,
    value: &crate::semantic::value::Presence<T>,
    encode: impl FnOnce(&T) -> Value,
) {
    use crate::semantic::value::Presence;
    match value {
        Presence::Absent => {}
        Presence::Null => {
            o.insert(key.into(), Value::Null);
        }
        Presence::Value(v) => {
            o.insert(key.into(), encode(v));
        }
    }
}
pub(super) fn object(v: &Value) -> Result<&Map<String, Value>, CodecError> {
    v.as_object().ok_or(CodecError::Invalid("object"))
}
pub(super) fn fields(o: &Map<String, Value>, allowed: &[&str]) -> Result<(), CodecError> {
    if let Some(key) = o.keys().find(|key| !allowed.contains(&key.as_str())) {
        return Err(CodecError::Unsupported(key.clone()));
    }
    Ok(())
}
/// A replayed SDK derived view (`parsed`, `parsed_arguments`) must equal the naive JSON
/// parse of its authoritative raw body, mirroring the pinned SDK's derivation over this
/// opaque string; null means no derived value. The view is validated then dropped: it
/// never becomes IR state, is never emitted, and an unparseable body cannot carry a
/// non-null view.
pub(super) fn admit_parsed(
    key: &'static str,
    parsed: Option<&Value>,
    raw: Option<&str>,
) -> Result<(), CodecError> {
    match (parsed, raw) {
        (None | Some(Value::Null), _) => Ok(()),
        (Some(_), None) => Err(CodecError::Invalid(key)),
        (Some(v), Some(raw)) => {
            let expected: Value = crate::semantic::value::parse_json_view(raw.as_bytes())
                .map_err(|_| CodecError::Invalid(key))?;
            if *v != expected {
                return Err(CodecError::Invalid(key));
            }
            Ok(())
        }
    }
}
pub(super) fn string<'a>(
    o: &'a Map<String, Value>,
    key: &'static str,
) -> Result<&'a str, CodecError> {
    o.get(key)
        .and_then(Value::as_str)
        .ok_or(CodecError::Invalid(key))
}
pub(super) fn text(value: &str, kind: &'static str, max: usize) -> Result<Text, CodecError> {
    Text::new(value, kind, max).map_err(|_| CodecError::Invalid(kind))
}
pub(super) fn nullable_bool(
    o: &Map<String, Value>,
    key: &'static str,
) -> Result<Option<bool>, CodecError> {
    o.get(key)
        .filter(|v| !v.is_null())
        .map(|v| v.as_bool().ok_or(CodecError::Invalid(key)))
        .transpose()
}
pub(super) fn controls(
    o: &Map<String, Value>,
    key: &'static str,
) -> Result<GenerationControls, CodecError> {
    let mut c = GenerationControls::default();
    c.max_output_tokens = o
        .get(key)
        .filter(|v| !v.is_null())
        .map(|v| {
            v.as_u64()
                .filter(|n| *n > 0)
                .ok_or(CodecError::Invalid(key))
        })
        .transpose()?;
    if let Some(t) = o.get("temperature").filter(|v| !v.is_null()) {
        c = c.with_temperature(t.as_f64().ok_or(CodecError::Invalid("temperature"))?)?;
    }
    Ok(c)
}
pub(super) fn write_controls(r: &GenerationRequest, o: &mut Map<String, Value>, key: &str) {
    write_control_values(r.controls(), o, key);
}
pub(super) fn write_control_values(c: &GenerationControls, o: &mut Map<String, Value>, key: &str) {
    if let Some(t) = c.temperature() {
        o.insert("temperature".into(), Value::from(t));
    }
    if let Some(n) = c.max_output_tokens {
        o.insert(key.into(), Value::from(n));
    }
}
#[derive(Default)]
pub(super) struct Items {
    pub items: Vec<(ItemId, Item)>,
    pub fidelity: FidelityRecords,
    next_item: u64,
    next_part: u64,
}
impl Items {
    pub fn id(&mut self) -> Result<ItemId, CodecError> {
        if self.next_item as usize >= MAX_ITEMS {
            return Err(CodecError::Limit);
        }
        self.next_item += 1;
        Ok(ItemId::new(self.next_item))
    }
    pub fn part_id(&mut self) -> Result<PartId, CodecError> {
        self.next_part += 1;
        if self.next_part as usize > MAX_ITEMS {
            return Err(CodecError::Limit);
        }
        Ok(PartId::new(self.next_part))
    }
    pub fn refusal(&mut self, text: Text) -> Result<Part, CodecError> {
        self.next_part += 1;
        if self.next_part as usize > MAX_ITEMS {
            return Err(CodecError::Limit);
        }
        Ok(Part {
            id: PartId::new(self.next_part),
            content: ContentPart::Refusal(text.into()),
        })
    }
    pub fn part(&mut self, s: &str) -> Result<Part, CodecError> {
        self.next_part += 1;
        if self.next_part as usize > MAX_ITEMS {
            return Err(CodecError::Limit);
        }
        Ok(Part {
            id: PartId::new(self.next_part),
            content: ContentPart::Text(
                Text::allowing_empty(s, "text", MAX_TEXT_BYTES)
                    .map_err(|_| CodecError::Limit)?
                    .into(),
            ),
        })
    }
    pub fn record_id(&mut self, id: ItemId, o: &Map<String, Value>) -> Result<(), CodecError> {
        if let Some(v) = o.get("id") {
            self.fidelity
                .record_response_item_id(id, v.as_str().ok_or(CodecError::Invalid("item id"))?)?;
        }
        Ok(())
    }
}
pub(super) fn raw_string(o: &Map<String, Value>, key: &'static str) -> Result<String, CodecError> {
    let s = string(o, key)?;
    if s.len() > MAX_TEXT_BYTES {
        return Err(CodecError::Limit);
    }
    Ok(s.to_owned())
}
pub(super) fn strict_default(profile: Profile) -> StrictDefault {
    match profile {
        Profile::Chat => StrictDefault::NonStrict,
        Profile::Responses => StrictDefault::NormalizeSchema,
    }
}
pub(super) fn tool_call(
    o: &Map<String, Value>,
    profile: Profile,
    message: Option<ItemId>,
    item_status: Option<&str>,
    replay: bool,
) -> Result<ToolCall, CodecError> {
    let (f, id) = match profile {
        Profile::Chat => {
            fields(o, &["id", "type", "function", "index"])?;
            if string(o, "type")? != "function" {
                return Err(CodecError::Unsupported("tool kind".into()));
            }
            let f = object(o.get("function").ok_or(CodecError::Invalid("function"))?)?;
            fields(
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
            fields(
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
    admit_parsed(
        "parsed_arguments",
        f.get("parsed_arguments"),
        Some(string(f, "arguments")?),
    )?;
    // Function-call status is optional/nullable, unlike complete message status.
    // https://github.com/openai/openai-python/blob/be9d66628ad7377bd36fe5a76ae6d735843f0e76/src/openai/types/responses/response_function_tool_call.py
    let status = if o.get("status").is_some_and(Value::is_null) {
        ItemLifecycle::Completed
    } else {
        super::responses::status(o, ItemLifecycle::Completed)?
    };
    Ok(ToolCall {
        call_id: text(id, "call_id", 256)?,
        name: text(string(f, "name")?, "function name", 128)?,
        arguments: raw_string(f, "arguments")?.into(),
        message,
        status,
        context: if profile == Profile::Responses {
            super::responses::call_context(o)?
        } else {
            CallContext::default()
        },
    })
}
pub(super) fn check_response_carriers(response: &GenerationResponse) -> Result<(), CodecError> {
    if response.usage_reports().len() > 1
        || response.usage_reports().iter().any(|usage| {
            usage.scope != UsageScope::Operation
                || usage.basis != UsageBasis::Final
                || usage.output_relation != OutputTokenRelation::IncludesReasoning
                || usage.total_relation != TotalTokenRelation::InputAndOutput
                || usage.input_tokens.is_none()
                || usage.output_tokens.is_none()
                || usage.total_tokens.is_none()
        })
    {
        return Err(CodecError::Unsupported("usage projection".into()));
    }
    if response.progress() != InteractionProgress::Unreported {
        return Err(CodecError::Unsupported("interaction progress".into()));
    }
    Ok(())
}

pub(super) fn check_item_carriers(items: &[(ItemId, Item)]) -> Result<(), CodecError> {
    if items.iter().any(|(_, item)| matches!(item, Item::Reasoning(r) if r.replay.as_ref().is_some_and(|value| value.format() != ReplayFormat::ResponsesEncrypted))) {
        return Err(CodecError::Unsupported("replay format".into()));
    }
    if items.iter().any(|(_, item)| matches!(item, Item::ToolCall(call) if !matches!(call.arguments, ToolArguments::Raw(_)))) {
        return Err(CodecError::Unsupported("structured arguments".into()));
    }
    Ok(())
}
pub(super) fn accept_status(o: &Map<String, Value>, expected: &str) -> Result<(), CodecError> {
    let actual = o.get("status").and_then(Value::as_str);
    let allowed = if expected == "incomplete" {
        actual.is_none() || matches!(actual, Some("completed" | "incomplete" | "in_progress"))
    } else {
        actual.is_none() || actual == Some(expected)
    };
    if !allowed {
        return Err(CodecError::Unsupported("non-completed item".into()));
    }
    Ok(())
}
