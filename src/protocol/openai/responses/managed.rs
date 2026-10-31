//! Synchronous Responses storage envelopes; no store, transport or upstream state.
use crate::{
    protocol::{
        CodecError, DecodedRequest, DecodedResponse, GenerationRequestEnvelope as Request,
        ResponseStorage,
        failure::CarrierReason,
        openai::{GenerationApi, GenerationCodec, shape::*},
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
        store: read_presence(o, "store", |v| v.as_bool().ok_or(CodecError::invalid()))?,
        previous_response_id: read_presence(o, "previous_response_id", |v| {
            v.as_str().map(str::to_owned).ok_or(CodecError::invalid())
        })?,
    };
    storage.validate()?;
    Ok(Some(storage))
}

/// Compose typed stored history with the current input before combined validation.
pub fn decode(
    adapter: &GenerationCodec,
    bytes: &[u8],
    history: Option<(&DecodedRequest, &DecodedResponse)>,
) -> Result<(Request, Options), CodecError> {
    if adapter.protocol != GenerationApi::Responses {
        return Err(CodecError::ProfileMismatch);
    }
    let mut value = crate::protocol::json::decode(bytes)?;
    let options = Options::read(&value)?;
    let o = value.as_object_mut().ok_or(CodecError::invalid())?;
    o.remove("previous_response_id");
    o.insert("store".into(), json!(false));
    if history.is_none() && options.previous.is_some() {
        return Err(CodecError::invalid());
    }
    bounded(&value)?;
    let request = adapter.decode_client_history(
        &serde_json::to_vec(&value).map_err(|_| CodecError::invalid())?,
        history,
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
    let input_bytes = input.semantic.retained_bytes()?;
    let output_bytes = output.semantic.retained_bytes()?;
    let owners = input.semantic.items().iter().chain(output.semantic.items());
    let mut structural = 0usize;
    for (_, item) in owners {
        let provider_parts = match item {
            crate::semantic::task::generation::Item::ProviderTool(value) => match &value.output {
                Some(crate::semantic::task::generation::ToolOutput::Parts(parts)) => parts.len(),
                _ => 0,
            },
            _ => 0,
        };
        // Payload validation does not charge empty content-part allocations.
        // Part is the largest retained part variant; allow vector growth as well.
        structural = structural.saturating_add(
            item.part_ids()
                .count()
                .saturating_add(provider_parts)
                .saturating_mul(std::mem::size_of::<crate::semantic::task::generation::Part>())
                .saturating_mul(2),
        );
    }
    // Include hidden native attachments and sidecars without encoding public wire.
    Ok(input_bytes
        .saturating_add(output_bytes)
        .saturating_add(structural)
        .saturating_add(input.fidelity.retained_bytes())
        .saturating_add(output.fidelity.retained_bytes())
        .saturating_add(output.metadata.context.reports.validate()?)
        .saturating_add(
            output
                .metadata
                .delivery
                .as_ref()
                .map_or(0, |d| d.request_metadata_bytes()),
        )
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
            _ => {
                return Err(CodecError::Unsupported(CarrierReason::Unavailable));
            }
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
            return Err(CodecError::invalid());
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
                    .ok_or(CodecError::invalid())?;
            }
            "order" => {
                ascending = match value.as_str() {
                    "asc" => true,
                    "desc" => false,
                    _ => {
                        return Err(CodecError::invalid());
                    }
                }
            }
            "after" if !value.is_empty() && value.len() <= 256 => after = Some(value),
            "include[]" if value == "reasoning.encrypted_content" => {}
            _ => {
                return Err(CodecError::Unsupported(CarrierReason::Unavailable));
            }
        }
    }
    let mut items = super::items::encode_items(
        request.semantic.items(),
        &request.fidelity,
        false,
        request.semantic.resources(),
    )?;
    for (index, (value, (_, item))) in items.iter_mut().zip(request.semantic.items()).enumerate() {
        if value.get("id").is_none() {
            value["id"] = json!(format!("{response_id}_input_{index}"));
        }
        if value.get("type").is_none() {
            value["type"] = json!("message");
        }
        if let Some(status) = item.lifecycle() {
            value["status"] = json!(super::items::status_label(status));
        }
        // Request shorthand is not a returned resource. Accepted history is
        // complete unless it explicitly retains another lifecycle state.
        use crate::semantic::task::generation::Item;
        if matches!(item, Item::Instruction(_))
            && let Some(text) = value["content"].as_str()
        {
            value["content"] = json!([{"type":"input_text","text":text}]);
        }
        if matches!(
            item,
            Item::Instruction(_)
                | Item::ToolResult(_)
                | Item::CustomCall(_)
                | Item::CustomResult(_)
        ) && value.get("status").is_none()
        {
            value["status"] = json!("completed");
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
                .ok_or(CodecError::invalid())?
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
    use crate::{
        protocol::openai::{Dialect, GenerationApi, GenerationCodec, TargetContract},
        semantic::{task::generation::*, value::Presence},
    };
    use serde_json::json;

    #[test]
    fn continuation_decodes_ordered_history_with_current_settings_and_no_upstream_state() {
        let adapter = GenerationCodec::new(GenerationApi::Responses, Dialect::Standard, None);
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
        assert_eq!(
            adapter
                .encode_request(&request, "selected", &TargetContract::full())
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
    fn typed_continuation_preserves_native_signed_owners_and_grouping() {
        use crate::protocol::{google::generate_content, native::ReplayTarget};
        let adapter = GenerationCodec::new(GenerationApi::Responses, Dialect::Standard, None);
        let old = adapter
            .decode_client_request(br#"{"model":"m","input":"first"}"#)
            .unwrap();
        let target = ReplayTarget::new("synthetic", "native").unwrap();
        let mut decoder = generate_content::GenerationSseDecoder::new(
            200,
            "text/event-stream",
            Default::default(),
            LocalScope::ROOT,
            target.clone(),
        )
        .unwrap();
        let body = json!({"responseId":"native-r","modelVersion":"native","candidates":[{
            "index":0,"content":{"role":"model","parts":[
                {"text":"OK","thoughtSignature":"text-a"},
                {"text":"OK","thoughtSignature":"text-b"},
                {"functionCall":{"id":"c","name":"lookup","args":{"n":9007199254740993u64}},
                 "thoughtSignature":"call-signature"}
            ]},"finishReason":"STOP"
        }]});
        let wire = format!("data: {body}\n\n");
        let (consumed, _) = decoder.consume(wire.as_bytes()).unwrap();
        assert_eq!(consumed, wire.len());
        let mut output = decoder.finish().unwrap();
        crate::execution::replay::seal_for_client(
            &mut output,
            &crate::protocol::replay_envelope::CLIENT_ORIGIN,
        )
        .unwrap();
        let before = output.clone();
        let (mut request, _) = decode(
            &adapter,
            br#"{"model":"m","previous_response_id":"r","input":[{"type":"function_call_output","call_id":"c","output":"found"}]}"#,
            Some((&old.task, &output)),
        ).unwrap();
        assert_eq!(
            request.task.semantic.items()[0],
            old.task.semantic.items()[0]
        );
        assert_eq!(request.task.semantic.message_envelopes().len(), 1);
        assert_eq!(
            request.task.semantic.message_envelopes()[0].members().len(),
            3
        );
        let signatures: Vec<_> = request
            .task
            .semantic
            .items()
            .iter()
            .flat_map(|(_, item)| match item {
                Item::Message(m) => m
                    .parts
                    .iter()
                    .filter_map(|p| p.replay.as_ref())
                    .collect::<Vec<_>>(),
                Item::ToolCall(c) => c.replay.iter().collect(),
                _ => vec![],
            })
            .collect();
        assert_eq!(
            signatures.len(),
            3,
            "no attachment may disappear through public-wire composition"
        );
        assert!(retained_size(&request.task, &output, 2).unwrap() > 3 * 2048);
        let empty = adapter.decode_response(br#"{"id":"empty","object":"response","created_at":1,"model":"m","status":"completed","output":[]}"#).unwrap();
        let mut stale_prefix = request.task.clone();
        let mut stale_items = stale_prefix.semantic.items().to_vec();
        let Item::Message(message) = &mut stale_items[1].1 else {
            panic!("signed prefix text")
        };
        message.parts[0].content = ContentPart::Text(
            crate::semantic::value::Text::new("edited", "test", 32)
                .unwrap()
                .into(),
        );
        stale_prefix.semantic = stale_prefix.semantic.with_items(stale_items).unwrap();
        assert!(matches!(
            decode(
                &adapter,
                br#"{"model":"m","input":"third"}"#,
                Some((&stale_prefix, &empty)),
            ),
            Err(CodecError::Invalid(
                crate::protocol::failure::ShapeReason::InvalidValue
            ))
        ));
        let (third, _) = decode(
            &adapter,
            br#"{"model":"m","input":"third"}"#,
            Some((&request.task, &empty)),
        )
        .unwrap();
        assert_eq!(
            &third.task.semantic.items()[..request.task.semantic.items().len()],
            request.task.semantic.items(),
        );
        crate::execution::replay::open_from_client(&mut request).unwrap();
        let (native, _) = generate_content::encode_generation_request(
            &request.task.semantic,
            &request.task.fidelity,
            &target,
            None,
        )
        .unwrap();
        assert_eq!(
            native["contents"],
            json!([
                {"role":"user","parts":[{"text":"first"}]},
                {"role":"model","parts":[
                    {"text":"OK","thoughtSignature":"text-a"},
                    {"text":"OK","thoughtSignature":"text-b"},
                    {"functionCall":{"id":"c","name":"lookup","args":{"n":9007199254740993u64}},
                     "thoughtSignature":"call-signature"}
                ]},
                {"role":"user","parts":[{"functionResponse":{
                    "id":"c","name":"lookup","response":{"output":"found"}
                }}]}
            ])
        );
        assert_eq!(
            output, before,
            "composition cannot mutate the stored source"
        );
        let mut edited = output.clone();
        let mut items = edited.semantic.items().to_vec();
        let Item::Message(message) = &mut items[0].1 else {
            panic!("signed text")
        };
        message.parts[0].content = ContentPart::Text(
            crate::semantic::value::Text::new("edited", "test", 32)
                .unwrap()
                .into(),
        );
        edited.semantic = edited.semantic.with_items(items).unwrap();
        assert!(matches!(
            decode(
                &adapter,
                br#"{"model":"m","input":"again"}"#,
                Some((&old.task, &edited)),
            ),
            Err(CodecError::Invalid(
                crate::protocol::failure::ShapeReason::InvalidValue
            ))
        ));
    }

    #[test]
    fn sealed_compaction_history_does_not_resurrect_output_only_creator_reports() {
        let adapter = GenerationCodec::new(
            GenerationApi::Responses,
            Dialect::Standard,
            Some(crate::semantic::value::ReplayOrigin::new("synthetic-issuer").unwrap()),
        );
        let old = adapter
            .decode_client_request(br#"{"model":"m","input":"first"}"#)
            .unwrap();
        let mut output = adapter.decode_response(br#"{"id":"r","object":"response","created_at":1,"model":"m",
            "status":"completed","output":[{"type":"compaction","id":"cmp","encrypted_content":"native-context","created_by":"actor"}]}"#).unwrap();
        crate::execution::replay::seal_for_client(
            &mut output,
            &crate::protocol::replay_envelope::CLIENT_ORIGIN,
        )
        .unwrap();
        let (mut next, _) = decode(
            &adapter,
            br#"{"model":"m","input":"next"}"#,
            Some((&old.task, &output)),
        )
        .unwrap();
        let (id, item) = &next.task.semantic.items()[1];
        assert!(
            next.task
                .fidelity
                .created_by(*id, item, next.task.semantic.resources())
                .is_none()
        );
        crate::execution::replay::open_from_client(&mut next).unwrap();
        assert!(
            matches!(&next.task.semantic.items()[1].1, Item::Compaction(value) if value.as_str() == "native-context")
        );
    }

    #[test]
    fn typed_history_keeps_resource_references_and_empty_envelope_boundaries() {
        let adapter = GenerationCodec::new(GenerationApi::Responses, Dialect::Standard, None);
        let mut old = adapter
            .decode_client_request(
                br#"{"model":"m","input":[{"role":"user","content":[
            {"type":"input_image","image_url":"https://example.invalid/old"}]}]}"#,
            )
            .unwrap()
            .task;
        old.semantic = old
            .semantic
            .with_message_envelopes(vec![MessageEnvelope::empty(
                GroupId::new(LocalScope::ROOT, 99),
                MessageEnvelopeRole::Assistant,
                None,
            )])
            .unwrap();
        let mut output = adapter.decode_response(br#"{"id":"r","object":"response","model":"m","created_at":1,
            "status":"completed","output":[{"id":"text","type":"message","role":"assistant","status":"completed","content":[
                {"type":"output_text","text":"OK","annotations":[{"type":"url_citation","start_index":0,"end_index":2,
                    "url":"https://example.invalid/source","title":"source"}]}
            ]}]}"#).unwrap();
        output.semantic = output
            .semantic
            .clone()
            .with_message_envelopes(vec![
                MessageEnvelope::empty(
                    GroupId::new(LocalScope::ROOT, 1),
                    MessageEnvelopeRole::Assistant,
                    Some(output.semantic.items()[0].0),
                ),
                MessageEnvelope::empty(
                    GroupId::new(LocalScope::ROOT, 2),
                    MessageEnvelopeRole::Assistant,
                    None,
                ),
            ])
            .unwrap();
        let (next, _) = decode(
            &adapter,
            br#"{"model":"m","input":[{"role":"user","content":[
            {"type":"input_image","image_url":"https://example.invalid/new"}]}]}"#,
            Some((&old, &output)),
        )
        .unwrap();
        let items = next.task.semantic.items();
        let groups = next.task.semantic.message_envelopes();
        assert_eq!(groups.len(), 3);
        assert_eq!(groups[0].before(), Some(items[1].0));
        assert_eq!(groups[1].before(), Some(items[1].0));
        assert_eq!(groups[2].before(), Some(items[2].0));
        let resources = next.task.semantic.resources();
        assert_eq!(resources.len(), 3);
        let Item::Message(message) = &items[1].1 else {
            panic!("assistant")
        };
        let ContentPart::Text(text) = &message.parts[0].content else {
            panic!("text")
        };
        let citation = &text.annotations()[0];
        citation.check_source(resources).unwrap();
        assert_eq!(
            resources.get(citation.value().source),
            Some(&ResourceLocation::Url(
                crate::semantic::value::Text::new("https://example.invalid/source", "source", 256)
                    .unwrap(),
            ))
        );
        for (index, expected) in [
            (0, "https://example.invalid/old"),
            (2, "https://example.invalid/new"),
        ] {
            let Item::Message(message) = &items[index].1 else {
                panic!("user")
            };
            let ContentPart::Resource(resource) = &message.parts[0].content else {
                panic!("image")
            };
            assert_eq!(
                resource.resolve(resources).unwrap(),
                &ResourceLocation::Url(
                    crate::semantic::value::Text::new(expected, "image", 256).unwrap(),
                )
            );
        }
    }

    #[test]
    fn retained_size_charges_empty_history_part_allocations() {
        let adapter = GenerationCodec::new(GenerationApi::Responses, Dialect::Standard, None);
        let output = adapter.decode_response(br#"{"id":"r","object":"response","created_at":1,"model":"m","status":"completed","output":[]}"#).unwrap();
        let weight = |count| {
            let parts = vec![json!({"type":"output_text","text":"","annotations":[]}); count];
            let request = adapter
                .decode_client_request(
                    json!({"model":"m","input":[{"id":"msg_1","type":"message","status":"completed","role":"assistant","content":parts}]})
                        .to_string()
                        .as_bytes(),
                )
                .unwrap();
            retained_size(&request.task, &output, 2).unwrap()
        };
        assert!(weight(1024) - weight(1) >= 1023 * std::mem::size_of::<Part>());
    }

    #[test]
    fn input_items_have_stable_cursors_and_strict_pagination() {
        let adapter = GenerationCodec::new(GenerationApi::Responses, Dialect::Standard, None);
        let resources = adapter
            .decode_client_request(
                br#"{"model":"synthetic","input":[
                {"role":"system","content":"rules"},
                {"role":"developer","content":[{"type":"input_text","text":"more"}],"status":"incomplete"},
                {"type":"function_call","call_id":"f","name":"lookup","arguments":"{}"},
                {"type":"function_call_output","call_id":"f","output":"found"},
                {"type":"custom_tool_call","call_id":"c","name":"sql","input":"SELECT 1"},
                {"type":"custom_tool_call_output","call_id":"c","output":"1","status":"incomplete"}
            ]}"#,
            )
            .unwrap();
        let listed = input_items(&resources.task, "resp_resources", Some("order=asc")).unwrap();
        assert_eq!(
            listed["data"][0]["content"],
            json!([{"type":"input_text","text":"rules"}])
        );
        assert_eq!(listed["data"][0]["role"], "developer");
        assert_eq!(listed["data"][1]["role"], "developer");
        assert_eq!(listed["data"][0]["status"], "completed");
        assert_eq!(
            listed["data"][1]["content"],
            json!([{"type":"input_text","text":"more"}])
        );
        assert_eq!(listed["data"][1]["status"], "incomplete");
        assert_eq!(listed["data"][3]["status"], "completed");
        assert_eq!(listed["data"][4]["status"], "completed");
        assert_eq!(listed["data"][5]["status"], "incomplete");
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

    #[test]
    fn reasoning_phase_and_tool_history_match_an_independent_stateless_oracle() {
        for dialect in [Dialect::OpenRouter, Dialect::Grok, Dialect::Siwc] {
            let origin = crate::semantic::value::ReplayOrigin::new("synthetic-history").unwrap();
            let adapter =
                GenerationCodec::new(GenerationApi::Responses, dialect, Some(origin.clone()));
            let mut contract = TargetContract::full();
            contract.replay_origin = Some(origin);
            let mut old_wire = json!({"model":"m","instructions":"old","input":"first"});
            if dialect == Dialect::OpenRouter {
                old_wire["metadata"] = json!({"old":"value"});
            }
            let old = adapter
                .decode_client_request(old_wire.to_string().as_bytes())
                .unwrap();
            let mut output_wire = json!({"id":"r1","object":"response","created_at":1,"model":"m",
            "status":"completed","output":[
                {"id":"reason1","type":"reasoning","summary":[{"type":"summary_text","text":"summary"}],
                    "content":[{"type":"reasoning_text","text":"readable"}],"encrypted_content":"opaque-final","status":"completed"},
                {"id":"comment","type":"message","role":"assistant","phase":"commentary","status":"completed",
                    "content":[{"type":"output_text","text":"checking","annotations":[]}]},
                {"id":"call-item","type":"function_call","call_id":"c","name":"lookup",
                    "arguments":"{\"n\":9007199254740993}","status":"completed"}],
            "usage":{"input_tokens":3,"output_tokens":2,"total_tokens":5,
                "input_tokens_details":{"cached_tokens":1},"output_tokens_details":{"reasoning_tokens":1}}});
            if dialect == Dialect::Siwc {
                output_wire["output"][2]["namespace"] = json!("ops");
            }
            let output = adapter
                .decode_response(output_wire.to_string().as_bytes())
                .unwrap();
            let mut expected_input = json!([
                {"type":"message","role":"user","content":[{"type":"input_text","text":"first"}]},
                {"id":"reason1","type":"reasoning","summary":[{"type":"summary_text","text":"summary"}],
                    "content":[{"type":"reasoning_text","text":"readable"}],"encrypted_content":"opaque-final"},
                {"id":"comment","type":"message","role":"assistant","status":"completed","phase":"commentary",
                    "content":[{"type":"output_text","text":"checking","annotations":[]}]},
                {"id":"call-item","type":"function_call","call_id":"c","name":"lookup","arguments":"{\"n\":9007199254740993}"},
                {"type":"function_call_output","call_id":"c","output":"done"}
            ]);
            if dialect == Dialect::Siwc {
                expected_input[3]["namespace"] = json!("ops");
            }
            let mut current = json!({"model":"m","previous_response_id":"public-r1","instructions":"new",
            "input":[{"type":"function_call_output","call_id":"c","output":"done"}],
            "include":["reasoning.encrypted_content"],"truncation":"disabled","max_tool_calls":3,
            "metadata":{"new":"value"},"max_output_tokens":1024});
            if dialect != Dialect::OpenRouter {
                for key in ["metadata", "max_tool_calls"] {
                    current.as_object_mut().unwrap().remove(key);
                }
            }
            if dialect == Dialect::Siwc {
                for key in ["truncation", "max_output_tokens"] {
                    current.as_object_mut().unwrap().remove(key);
                }
            }
            let (stored, _) = decode(
                &adapter,
                current.to_string().as_bytes(),
                Some((&old.task, &output)),
            )
            .unwrap();
            let mut explicit = current.clone();
            explicit
                .as_object_mut()
                .unwrap()
                .remove("previous_response_id");
            explicit["input"] = expected_input.clone();
            let (stateless, _) = decode(&adapter, explicit.to_string().as_bytes(), None).unwrap();
            let mut expected = explicit;
            expected["store"] = json!(false);
            if dialect == Dialect::OpenRouter {
                expected["provider"] = json!({"require_parameters":true});
            }
            if dialect == Dialect::Siwc {
                expected["stream"] = json!(true);
            }
            for request in [&stored, &stateless] {
                assert_eq!(
                    adapter.encode_request(request, "m", &contract).unwrap(),
                    expected
                );
            }
            let final_output=adapter.decode_response(json!({"id":"r2","object":"response","created_at":2,"model":"m",
            "status":"completed","output":[{"id":"final","type":"message","role":"assistant","phase":"final_answer",
                "status":"completed","content":[{"type":"output_text","text":"answer","annotations":[]}]}]})
            .to_string().as_bytes()).unwrap();
            let mut twice = expected_input.as_array().unwrap().clone();
            twice.extend([
            json!({"id":"final","type":"message","role":"assistant","status":"completed","phase":"final_answer",
                "content":[{"type":"output_text","text":"answer","annotations":[]}]}),
            json!({"type":"message","role":"user","content":[{"type":"input_text","text":"again"}]})
        ]);
            let (next, _) = decode(
                &adapter,
                br#"{"model":"m","previous_response_id":"public-r2","input":"again"}"#,
                Some((&stored.task, &final_output)),
            )
            .unwrap();
            let mut expected_next = json!({"model":"m","store":false,"input":twice});
            if dialect == Dialect::OpenRouter {
                expected_next["provider"] = json!({"require_parameters":true});
            }
            if dialect == Dialect::Siwc {
                expected_next["stream"] = json!(true);
            }
            assert_eq!(
                adapter.encode_request(&next, "m", &contract).unwrap(),
                expected_next
            );
            let listing = input_items(&next.task, "public-r3", Some("limit=2&order=asc")).unwrap();
            assert_eq!(listing["data"].as_array().unwrap().len(), 2);
            assert_eq!(listing["has_more"], true);
        }
    }
}
