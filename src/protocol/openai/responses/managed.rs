//! Synchronous Responses storage envelopes; no store, transport or upstream state.
use crate::{
    adapter::Request,
    protocol::{
        CodecError, DecodedRequest, DecodedResponse, ResponseStorage,
        openai::{Adapter, Profile, common::*},
    },
    semantic::value::Presence,
};
use serde_json::{Map, Value, json};

#[derive(Clone, Debug)]
pub struct Options {
    pub store: bool,
    pub previous: Option<String>,
}
impl Options {
    pub fn read(value: &Value) -> Result<Self, CodecError> {
        let storage = read_storage(object(value)?)?.unwrap_or_default();
        Ok(Self {
            store: storage.store != Presence::Value(false),
            previous: storage.previous_response_id.value().cloned(),
        })
    }
    pub fn metadata(&self) -> ResponseStorage {
        ResponseStorage {
            store: Presence::Value(self.store),
            previous_response_id: self
                .previous
                .clone()
                .map_or(Presence::Null, Presence::Value),
        }
    }
}
pub(crate) fn read_storage(o: &Map<String, Value>) -> Result<Option<ResponseStorage>, CodecError> {
    if !o.contains_key("store") && !o.contains_key("previous_response_id") {
        return Ok(None);
    }
    let storage = ResponseStorage {
        store: read_presence(o, "store", |v| {
            v.as_bool().ok_or(CodecError::Invalid("store"))
        })?,
        previous_response_id: read_presence(o, "previous_response_id", |v| {
            v.as_str()
                .map(str::to_owned)
                .ok_or(CodecError::Invalid("previous_response_id"))
        })?,
    };
    storage.validate()?;
    Ok(Some(storage))
}

/// Expand only typed stored history, then use ordinary client intake and validation.
/// The temporary wire belongs to this codec, not to storage or upstream execution.
pub fn decode(
    adapter: &Adapter,
    bytes: &[u8],
    history: Option<(&DecodedRequest, &DecodedResponse)>,
) -> Result<(Request, Options), CodecError> {
    if adapter.protocol != Profile::Responses {
        return Err(CodecError::ProfileMismatch);
    }
    let mut value = crate::protocol::json::decode(bytes)?;
    let options = Options::read(&value)?;
    let o = value
        .as_object_mut()
        .ok_or(CodecError::Invalid("request"))?;
    o.remove("previous_response_id");
    o.insert("store".into(), json!(false));
    if let Some((input, output)) = history {
        if !matches!(
            output.semantic.outcome(),
            crate::semantic::task::generation::Outcome::Completed
                | crate::semantic::task::generation::Outcome::Incomplete
        ) {
            return Err(CodecError::Invalid("non-replayable response"));
        }
        let mut items = super::encode_items(
            input.semantic.items(),
            &input.fidelity,
            false,
            input.semantic.resources(),
        );
        items.extend(super::encode_items(
            output.semantic.items(),
            &output.fidelity,
            false,
            output.semantic.resources(),
        ));
        match o.remove("input") {
            Some(Value::Array(values)) => items.extend(values),
            Some(Value::String(text)) => items.push(json!({"role":"user","content":text})),
            None => {}
            _ => return Err(CodecError::Invalid("input")),
        }
        o.insert("input".into(), Value::Array(items));
    } else if options.previous.is_some() {
        return Err(CodecError::Invalid("unresolved previous_response_id"));
    }
    bounded(&value)?;
    let request = adapter.decode_client_request(
        &serde_json::to_vec(&value).map_err(|_| CodecError::Invalid("request"))?,
    )?;
    request
        .task
        .fidelity
        .check_wire_item_ids(request.task.semantic.items(), false)?;
    Ok((request, options))
}

pub(crate) fn retained_size(
    input: &DecodedRequest,
    output: &DecodedResponse,
    body_bytes: usize,
) -> Result<usize, CodecError> {
    let mut value = json!({"input":super::encode_items(
        input.semantic.items(), &input.fidelity, false, input.semantic.resources(),
    )});
    crate::protocol::openai::settings::write(
        input.semantic.settings(),
        value.as_object_mut().expect("object"),
    )?;
    let input_bytes = crate::semantic::value::json_size(
        &value,
        crate::semantic::task::generation::MAX_TOTAL_BYTES,
    )
    .map_err(|_| CodecError::Limit)?;
    // Account for the typed payload, retrieval bytes and fixed per-owner storage.
    Ok(input_bytes
        .saturating_add(body_bytes.saturating_mul(2))
        .saturating_add(
            (input.semantic.items().len() + output.semantic.items().len())
                * std::mem::size_of::<(
                    crate::semantic::task::generation::ItemId,
                    crate::semantic::task::generation::Item,
                )>(),
        )
        .saturating_add(1024))
}
/// Only completed snapshots are retrievable; no stream replay or background polling.
pub fn validate_retrieve_query(query: Option<&str>) -> Result<(), CodecError> {
    for (name, value) in query_pairs(query)? {
        match (name.as_str(), value.as_str()) {
            ("stream", "false") | ("include[]", "reasoning.encrypted_content") => {}
            _ => return Err(CodecError::Unsupported("response retrieval query".into())),
        }
    }
    Ok(())
}
fn query_pairs(query: Option<&str>) -> Result<Vec<(String, String)>, CodecError> {
    let query = query.unwrap_or_default();
    if query.len() > 2048 {
        return Err(CodecError::Limit);
    }
    let mut pairs = vec![];
    for (key, value) in url::form_urlencoded::parse(query.as_bytes()) {
        if pairs.iter().any(|(old, _)| old == &key) {
            return Err(CodecError::Invalid("duplicate query parameter"));
        }
        pairs.push((key.into_owned(), value.into_owned()));
    }
    Ok(pairs)
}
pub fn input_items(
    request: &DecodedRequest,
    response_id: &str,
    query: Option<&str>,
) -> Result<Value, CodecError> {
    let mut limit = 20usize;
    let mut ascending = false;
    let mut after = None;
    for (key, value) in query_pairs(query)? {
        match key.as_str() {
            "limit" => {
                limit = value
                    .parse()
                    .ok()
                    .filter(|n| (1..=100).contains(n))
                    .ok_or(CodecError::Invalid("limit"))?;
            }
            "order" => {
                ascending = match value.as_str() {
                    "asc" => true,
                    "desc" => false,
                    _ => return Err(CodecError::Invalid("order")),
                }
            }
            "after" if !value.is_empty() && value.len() <= 256 => after = Some(value),
            "include[]" if value == "reasoning.encrypted_content" => {}
            _ => return Err(CodecError::Unsupported("input items query".into())),
        }
    }
    let mut items = super::encode_items(
        request.semantic.items(),
        &request.fidelity,
        false,
        request.semantic.resources(),
    );
    for (index, (value, (_, item))) in items.iter_mut().zip(request.semantic.items()).enumerate() {
        if value.get("id").is_none() {
            value["id"] = json!(format!("{response_id}_input_{index}"));
        }
        if value.get("type").is_none() {
            value["type"] = json!("message");
        }
        if let Some(status) = item.lifecycle() {
            value["status"] = json!(super::status_label(status));
        }
    }
    if !ascending {
        items.reverse();
    }
    let start = match after {
        Some(id) => {
            items
                .iter()
                .position(|value| value["id"].as_str() == Some(&id))
                .ok_or(CodecError::Invalid("after"))?
                + 1
        }
        None => 0,
    };
    let has_more = items.len().saturating_sub(start) > limit;
    let data: Vec<_> = items.into_iter().skip(start).take(limit).collect();
    Ok(json!({
        "object":"list",
        "first_id":data.first().and_then(|v| v.get("id")),
        "last_id":data.last().and_then(|v| v.get("id")),
        "has_more":has_more,
        "data":data
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::openai::{Adapter, Dialect, Profile, TargetCaps};
    use crate::semantic::{task::generation::*, value::Presence};
    use serde_json::json;

    #[test]
    fn continuation_decodes_ordered_history_with_current_settings_and_no_upstream_state() {
        let adapter = Adapter::new(Profile::Responses, Dialect::Standard, None);
        let old = adapter
            .decode_client_request(br#"{"model":"synthetic","instructions":"old","input":"first"}"#)
            .unwrap();
        let output = adapter.decode_response(
            br#"{"id":"upstream","object":"response","model":"synthetic","created_at":1,"status":"completed","output":[{"type":"function_call","id":"call-item","call_id":"c","name":"lookup","arguments":"{}","status":"completed"}]}"#,
        ).unwrap();
        let (request, options) = decode(
            &adapter,
            br#"{"model":"synthetic","previous_response_id":"resp_parent","instructions":"new","input":[{"type":"function_call_output","call_id":"c","output":"found"}]}"#,
            Some((&old.task, &output)),
        ).unwrap();
        assert!(options.store);
        assert_eq!(options.previous.as_deref(), Some("resp_parent"));
        assert_eq!(request.task.semantic.items().len(), 3);
        assert!(
            matches!(&request.task.semantic.items()[1].1, Item::ToolCall(c) if c.call_id.as_str() == "c")
        );
        assert!(
            matches!(&request.task.semantic.items()[2].1, Item::ToolResult(r) if r.call_id.as_ref().unwrap().as_str() == "c")
        );
        assert_eq!(request.context.store, Presence::Value(false));
        assert!(request.context.previous_response_id.is_absent());
        assert_eq!(
            adapter
                .encode_request(&request, "selected", &TargetCaps::full())
                .unwrap(),
            json!({"model":"selected","store":false,"instructions":"new","input":[
                {"type":"message","role":"user","content":[{"type":"input_text","text":"first"}]},
                {"type":"function_call","id":"call-item","call_id":"c","name":"lookup","arguments":"{}"},
                {"type":"function_call_output","call_id":"c","output":"found"}
            ]})
        );
        assert!(decode(&adapter,
            br#"{"model":"synthetic","input":[{"type":"function_call_output","call_id":"other","output":"bad"}]}"#,
            Some((&old.task, &output))).is_err());
        assert!(Options::read(&json!({"store":"true"})).is_err());
        assert!(Options::read(&json!({})).unwrap().store);
        assert!(Options::read(&json!({"store":null})).unwrap().store);
        assert!(!Options::read(&json!({"store":false})).unwrap().store);
    }

    #[test]
    fn input_items_have_stable_cursors_and_strict_pagination() {
        let adapter = Adapter::new(Profile::Responses, Dialect::Standard, None);
        let request = adapter.decode_client_request(
            br#"{"model":"synthetic","input":[{"role":"user","content":"one"},{"role":"assistant","content":"two"}]}"#,
        ).unwrap();
        let first = input_items(&request.task, "resp_local", Some("limit=1&order=asc")).unwrap();
        assert_eq!(first["data"][0]["content"][0]["text"], "one");
        assert_eq!(first["has_more"], true);
        let cursor = first["last_id"].as_str().unwrap();
        let last = input_items(
            &request.task,
            "resp_local",
            Some(&format!("after={cursor}&order=asc")),
        )
        .unwrap();
        assert_eq!(last["data"][0]["content"][0]["text"], "two");
        assert_eq!(last["has_more"], false);
        assert!(input_items(&request.task, "resp_local", Some("limit=0")).is_err());
        assert!(input_items(&request.task, "resp_local", Some("after=unknown")).is_err());
    }
}
