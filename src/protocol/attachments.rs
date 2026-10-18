//! Gateway-only transport of part/call ownership and message phases through client carriers.
//! The manifest stores boundaries and dependencies, never replacement business content.
use crate::{
    protocol::{CodecError, DecodedResponse, fidelity::FidelityRecords, replay_envelope},
    semantic::{
        task::generation::*,
        value::{JsonLimits, Text, parse_json},
    },
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

const PREFIX: &str = "mca1.";
fn invalid() -> CodecError {
    CodecError::Invalid("replay attachment manifest")
}
fn unsupported() -> CodecError {
    CodecError::Unsupported("replay attachment representation".into())
}
pub(crate) fn check_chat_support(
    profile: crate::protocol::openai::Profile,
    structured: bool,
) -> Result<(), CodecError> {
    if profile == crate::protocol::openai::Profile::Chat && !structured {
        return Err(CodecError::Unsupported("Chat attachment carrier".into()));
    }
    Ok(())
}
fn digest(parts: &[&str]) -> String {
    let mut hash = Sha256::new();
    for part in parts {
        hash.update((part.len() as u64).to_le_bytes());
        hash.update(part.as_bytes());
    }
    hash.finalize().iter().map(|b| format!("{b:02x}")).collect()
}
fn arguments(call: &ToolCall) -> Result<String, CodecError> {
    let value = match &call.arguments {
        ToolArguments::Structured(value) => value.clone(),
        ToolArguments::Raw(raw) => StructuredValue::from_bytes(raw.as_bytes())?,
        ToolArguments::StructuredPartial(_) => return Err(unsupported()),
    };
    if !value.value().is_object() {
        return Err(unsupported());
    }
    serde_json::to_string(value.value()).map_err(|_| invalid())
}
fn call_hash(call: &ToolCall) -> Result<String, CodecError> {
    if call.context != CallContext::default() {
        return Err(unsupported());
    }
    Ok(digest(&[
        call.call_id.as_str(),
        call.name.as_str(),
        &arguments(call)?,
    ]))
}
fn reasoning_text(reasoning: &ReasoningItem) -> String {
    reasoning
        .parts
        .iter()
        .map(|(_, p)| match p {
            ReasoningContent::Summary(v) | ReasoningContent::Text(v) => v.as_str(),
        })
        .collect()
}
fn reasoning_hash(reasoning: &ReasoningItem) -> String {
    digest(&[
        &reasoning_text(reasoning),
        reasoning.replay.as_ref().map_or("", ReplayValue::as_str),
    ])
}
#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Status {
    Completed,
    Incomplete,
    InProgress,
}
impl From<ItemLifecycle> for Status {
    fn from(value: ItemLifecycle) -> Self {
        match value {
            ItemLifecycle::Completed => Self::Completed,
            ItemLifecycle::Incomplete => Self::Incomplete,
            ItemLifecycle::InProgress => Self::InProgress,
        }
    }
}
impl From<Status> for ItemLifecycle {
    fn from(value: Status) -> Self {
        match value {
            Status::Completed => Self::Completed,
            Status::Incomplete => Self::Incomplete,
            Status::InProgress => Self::InProgress,
        }
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Segment {
    bytes: usize,
    hash: String,
    replay: Option<String>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReasoningSegment {
    bytes: usize,
    summary: bool,
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum Entry {
    Message {
        status: Status,
        phase: Option<String>,
        parts: Vec<Segment>,
    },
    Call {
        status: Status,
        hash: String,
        replay: Option<String>,
    },
    Reasoning {
        status: Status,
        hash: String,
        parts: Vec<ReasoningSegment>,
    },
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Group {
    members: Vec<usize>,
    before: Option<usize>,
}
/// An encoded manifest. Client encoders place it in their own carrier before validation.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct Carrier(String);
impl Carrier {
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}
impl std::fmt::Debug for Carrier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Carrier([REDACTED])")
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Packet {
    items: Vec<Entry>,
    groups: Vec<Group>,
}
fn sealed(value: Option<&ReplayValue>) -> Result<Option<String>, CodecError> {
    value
        .map(|value| {
            if value.replay_token().is_none() {
                return Err(unsupported());
            }
            value.validate_attachment()?;
            let (_, format, _) = replay_envelope::open(value.as_str()).ok_or_else(invalid)?;
            if format != value.format() {
                return Err(invalid());
            }
            Ok(value.as_str().to_owned())
        })
        .transpose()
}
fn replay(value: Option<&str>) -> Result<Option<ReplayValue>, CodecError> {
    value
        .map(|value| {
            let (_, format, _) = replay_envelope::open(value).ok_or_else(invalid)?;
            let replay = ReplayValue::final_value(
                format,
                Text::new(value, "attachment", MAX_TEXT_BYTES).map_err(|_| invalid())?,
            );
            replay.validate_attachment()?;
            Ok(replay)
        })
        .transpose()
}
impl Packet {
    /// Consume the existing EOF-validated materialization, not a second stream buffer.
    pub(crate) fn from_response(response: &DecodedResponse) -> Result<Option<Self>, CodecError> {
        response
            .fidelity
            .check_attachment_bindings(response.semantic.items())?;
        if !response
            .semantic
            .items()
            .iter()
            .any(|(_, i)| i.has_replay_attachment())
            && !crate::protocol::phase::has_phase(response.semantic.items())
        {
            return Ok(None);
        }
        let mut items = Vec::new();
        for (owner, item) in response.semantic.items() {
            items.push(match item {
                Item::Message(message) if message.role == MessageRole::Assistant => {
                    let parts = message
                        .parts
                        .iter()
                        .enumerate()
                        .map(|(ordinal, part)| {
                            let ContentPart::Text(text) = &part.content else {
                                return Err(unsupported());
                            };
                            if part.replay.is_some() {
                                let origin = response
                                    .fidelity
                                    .part_replay_origin(*owner, part.id)
                                    .ok_or_else(invalid)?;
                                if !response.fidelity.part_replay_matches(
                                    *owner,
                                    message.status,
                                    ordinal,
                                    part,
                                    Some(origin),
                                ) {
                                    return Err(invalid());
                                }
                            }
                            Ok(Segment {
                                bytes: text.as_str().len(),
                                hash: digest(&[text.as_str()]),
                                replay: sealed(part.replay.as_ref())?,
                            })
                        })
                        .collect::<Result<_, CodecError>>()?;
                    Entry::Message {
                        status: message.status.into(),
                        phase: message.phase.map(|p| p.label().into()),
                        parts,
                    }
                }
                Item::ToolCall(call) => {
                    if call.replay.is_some() {
                        let origin = response
                            .fidelity
                            .replay_origin(*owner)
                            .ok_or_else(invalid)?;
                        if !response
                            .fidelity
                            .call_replay_matches(*owner, call, Some(origin))
                        {
                            return Err(invalid());
                        }
                    }
                    Entry::Call {
                        status: call.status.into(),
                        hash: call_hash(call)?,
                        replay: sealed(call.replay.as_ref())?,
                    }
                }
                Item::Reasoning(reasoning) => Entry::Reasoning {
                    status: reasoning.status.into(),
                    hash: reasoning_hash(reasoning),
                    parts: reasoning
                        .parts
                        .iter()
                        .map(|(_, p)| match p {
                            ReasoningContent::Summary(v) => ReasoningSegment {
                                bytes: v.as_str().len(),
                                summary: true,
                            },
                            ReasoningContent::Text(v) => ReasoningSegment {
                                bytes: v.as_str().len(),
                                summary: false,
                            },
                        })
                        .collect(),
                },
                _ => return Err(unsupported()),
            });
        }
        let ordinals: BTreeMap<_, _> = response
            .semantic
            .items()
            .iter()
            .enumerate()
            .map(|(n, (id, _))| (*id, n))
            .collect();
        let groups = response
            .semantic
            .message_envelopes()
            .iter()
            .map(|group| {
                if group.role() != MessageEnvelopeRole::Assistant {
                    return Err(unsupported());
                }
                Ok(Group {
                    members: group
                        .members()
                        .iter()
                        .map(|id| ordinals.get(id).copied().ok_or_else(invalid))
                        .collect::<Result<_, _>>()?,
                    before: group
                        .before()
                        .map(|id| ordinals.get(&id).copied().ok_or_else(invalid))
                        .transpose()?,
                })
            })
            .collect::<Result<_, CodecError>>()?;
        let packet = Self { items, groups };
        packet.validate()?;
        Ok(Some(packet))
    }
    fn validate(&self) -> Result<(), CodecError> {
        if self.items.len() > MAX_ITEMS || self.groups.len() > MAX_ITEMS {
            return Err(CodecError::Limit);
        }
        let mut parts = 0usize;
        let mut bytes = 0usize;
        let mut attached = false;
        for item in &self.items {
            let mut count = |len: usize| -> Result<(), CodecError> {
                parts = parts.checked_add(1).ok_or(CodecError::Limit)?;
                bytes = bytes.checked_add(len).ok_or(CodecError::Limit)?;
                if parts > MAX_ITEMS || len > MAX_TEXT_BYTES || bytes > MAX_TOTAL_BYTES {
                    return Err(CodecError::Limit);
                }
                Ok(())
            };
            match item {
                Entry::Message { phase, parts, .. } => {
                    attached |= phase.is_some();
                    if phase
                        .as_deref()
                        .is_some_and(|p| !matches!(p, "commentary" | "final_answer"))
                    {
                        return Err(invalid());
                    }
                    for p in parts {
                        count(p.bytes)?;
                        attached |= p.replay.is_some();
                    }
                }
                Entry::Reasoning { parts, .. } => {
                    for p in parts {
                        count(p.bytes)?;
                    }
                }
                Entry::Call { replay, .. } => attached |= replay.is_some(),
            }
        }
        let members: usize = self.groups.iter().map(|g| g.members.len()).sum();
        if members > MAX_ITEMS {
            return Err(CodecError::Limit);
        }
        let mut owners = BTreeSet::new();
        if self.groups.iter().any(|group| {
            !group.members.is_empty()
                && (group.before.is_some()
                    || group.members.windows(2).any(|w| w[0] >= w[1])
                    || group.members.iter().any(|id| !owners.insert(*id)))
        }) {
            return Err(invalid());
        }
        if !attached
            || self.groups.iter().any(|g| {
                g.members
                    .iter()
                    .chain(g.before.iter())
                    .any(|n| *n >= self.items.len())
            })
        {
            return Err(invalid());
        }
        Ok(())
    }
    pub(crate) fn encode(&self) -> Result<Carrier, CodecError> {
        self.validate()?;
        let bytes = serde_json::to_vec(self).map_err(|_| invalid())?;
        let encoded = format!("{PREFIX}{}", URL_SAFE_NO_PAD.encode(bytes));
        if encoded.len() > MAX_TEXT_BYTES {
            return Err(CodecError::Limit);
        }
        Ok(Carrier(encoded))
    }
    fn decode(value: &str) -> Result<Self, CodecError> {
        if value.len() > MAX_TEXT_BYTES {
            return Err(CodecError::Limit);
        }
        let data = value.strip_prefix(PREFIX).ok_or_else(invalid)?;
        let bytes = URL_SAFE_NO_PAD.decode(data).map_err(|_| invalid())?;
        if URL_SAFE_NO_PAD.encode(&bytes) != data {
            return Err(invalid());
        }
        let value = parse_json(&bytes, JsonLimits::ENVELOPE).map_err(|_| invalid())?;
        let packet: Self = serde_json::from_value(value).map_err(|_| invalid())?;
        packet.validate()?;
        Ok(packet)
    }
}

pub(crate) fn is_marker(value: &str) -> bool {
    value.starts_with("mca")
}
fn label(fields: &serde_json::Map<String, Value>) -> bool {
    fields.get("id").is_none_or(|id| {
        id.as_str()
            .is_some_and(|id| !id.is_empty() && id.len() <= 256)
    }) && fields.get("index").is_none_or(|n| n.as_u64().is_some())
}
/// A recognized gateway carrier. An altered or misplaced one is dropped with its hidden
/// state rather than rejecting the visible history that the client returned.
pub(crate) enum Carried {
    Manifest(String),
    Unusable,
}
/// Recognized transport records are removed before ordinary reasoning decoding.
pub(crate) fn take_chat(message: &mut Value) -> Option<Carried> {
    let assistant = message["role"] == "assistant";
    let details = message
        .get_mut("reasoning_details")
        .and_then(Value::as_array_mut)?;
    let carrier = |d: &Value| {
        d["type"] == "reasoning.encrypted"
            && d.get("data").and_then(Value::as_str).is_some_and(is_marker)
    };
    let mut found = details.iter().filter(|d| carrier(d));
    let first = found.next()?;
    let usable = assistant
        && found.next().is_none()
        && first.as_object().is_some_and(|fields| {
            label(fields)
                && fields
                    .get("format")
                    .is_none_or(|f| f.as_str() == Some("openai-responses-v1"))
        });
    let value = first["data"].as_str().unwrap_or_default().to_owned();
    details.retain(|d| !carrier(d));
    Some(if usable {
        Carried::Manifest(value)
    } else {
        Carried::Unusable
    })
}
pub(crate) fn responses_marker(fields: &serde_json::Map<String, Value>) -> Option<Carried> {
    if fields.get("type").and_then(Value::as_str) != Some("reasoning") {
        return None;
    }
    let value = fields
        .get("encrypted_content")
        .and_then(Value::as_str)
        .filter(|s| is_marker(s))?;
    let usable = label(fields)
        && ["summary", "content"].iter().all(|field| {
            fields
                .get(*field)
                .is_none_or(|v| v.as_array().is_some_and(Vec::is_empty))
        })
        && fields
            .get("status")
            .is_none_or(|v| v.as_str() == Some("completed"));
    Some(if usable {
        Carried::Manifest(value.to_owned())
    } else {
        Carried::Unusable
    })
}
pub(crate) fn messages_marker(block: &super::anthropic::Block) -> Option<Carried> {
    match block {
        super::anthropic::Block::Thinking {
            thinking,
            signature,
        } if is_marker(signature) => Some(if thinking.is_empty() {
            Carried::Manifest(signature.clone())
        } else {
            Carried::Unusable
        }),
        super::anthropic::Block::RedactedThinking(data) if is_marker(data) => {
            Some(Carried::Unusable)
        }
        _ => None,
    }
}
pub(crate) struct Marker {
    start: usize,
    end: usize,
    /// `None` when the returned carrier cannot be decoded.
    packet: Option<Packet>,
}
impl Marker {
    pub(crate) fn bounded(carried: Carried, start: usize, end: usize) -> Self {
        Self {
            start,
            end,
            packet: match carried {
                Carried::Manifest(value) => Packet::decode(&value).ok(),
                Carried::Unusable => None,
            },
        }
    }
    pub(crate) fn prefix(
        carried: Carried,
        items: &[(ItemId, Item)],
        previous: Option<&Self>,
    ) -> Self {
        let end = items.len();
        let mut start = end;
        let boundary = previous.map_or(0, |marker| marker.end);
        while start > boundary && assistant(&items[start - 1].1) {
            start -= 1;
        }
        Self::bounded(carried, start, end)
    }
}
fn assistant(item: &Item) -> bool {
    matches!(item, Item::Reasoning(_) | Item::ToolCall(_))
        || matches!(item,Item::Message(m) if m.role==MessageRole::Assistant)
}
struct TextPiece<'a> {
    part: &'a Part,
    status: ItemLifecycle,
    phase: Option<Phase>,
}
fn status(observed: ItemLifecycle, described: Status) -> Result<ItemLifecycle, CodecError> {
    let described = described.into();
    if observed != ItemLifecycle::Completed && observed != described {
        return Err(invalid());
    }
    Ok(described)
}
impl TextPiece<'_> {
    fn check(&self, desired: Status, phase: Option<Phase>) -> Result<(), CodecError> {
        status(self.status, desired)?;
        if self.phase.is_some() && self.phase != phase {
            return Err(invalid());
        }
        Ok(())
    }
}
struct TextCursor<'a> {
    parts: Vec<TextPiece<'a>>,
    index: usize,
    offset: usize,
}
impl TextCursor<'_> {
    fn take(
        &mut self,
        len: usize,
        id: PartId,
        status: Status,
        phase: Option<Phase>,
        fidelity: &mut FidelityRecords,
    ) -> Result<Part, CodecError> {
        if let Some(piece) = self.parts.get(self.index) {
            let part = piece.part;
            let ContentPart::Text(text) = &part.content else {
                return Err(unsupported());
            };
            if self.offset == 0 && text.as_str().len() == len {
                piece.check(status, phase)?;
                self.index += 1;
                let mut value = part.clone();
                // Preserve existing metadata only when its entire part survives.
                value.id = id;
                if fidelity.cache_breakpoint(part.id) {
                    fidelity.record_cache_breakpoint(id)?;
                }
                if fidelity.input_text_form(part.id) {
                    fidelity.record_input_text(id)?;
                }
                return Ok(value);
            }
        }
        let mut value = String::with_capacity(len);
        let mut cache = None;
        let mut breakpoint = false;
        while value.len() < len {
            let piece = self.parts.get(self.index).ok_or_else(invalid)?;
            piece.check(status, phase)?;
            let part = piece.part;
            let ContentPart::Text(text) = &part.content else {
                return Err(unsupported());
            };
            if !text.is_plain() {
                return Err(CodecError::Unsupported("attachment metadata split".into()));
            }
            let take = (len - value.len()).min(text.as_str().len() - self.offset);
            let end = self.offset + take;
            if !text.as_str().is_char_boundary(end) {
                return Err(invalid());
            }
            value.push_str(&text.as_str()[self.offset..end]);
            self.offset = end;
            if self.offset == text.as_str().len() {
                if part.cache.is_some() || fidelity.cache_breakpoint(part.id) {
                    if value.len() != len {
                        return Err(CodecError::Unsupported("attachment cache boundary".into()));
                    }
                    cache = part.cache;
                    breakpoint = fidelity.cache_breakpoint(part.id);
                }
                self.index += 1;
                self.offset = 0;
            }
        }
        let mut part = Part::new(
            id,
            ContentPart::Text(
                Text::allowing_empty(value, "attachment text", MAX_TEXT_BYTES)
                    .map_err(|_| invalid())?
                    .into(),
            ),
        );
        part.cache = cache;
        if breakpoint {
            fidelity.record_cache_breakpoint(id)?;
        }
        Ok(part)
    }
    fn finish(&self) -> bool {
        self.index == self.parts.len() && self.offset == 0
    }
}

/// Fresh derived identities for restored owners; skipped values after a fallback are harmless.
struct Ids {
    item: u64,
    part: u64,
    group: u64,
}
impl Ids {
    fn next(value: &mut u64) -> Result<u64, CodecError> {
        *value = value.checked_add(1).ok_or(CodecError::Limit)?;
        Ok(*value)
    }
    fn item(&mut self) -> Result<ItemId, CodecError> {
        Ok(ItemId::scoped(
            LocalScope::DERIVED,
            Self::next(&mut self.item)?,
        ))
    }
    fn part(&mut self) -> Result<PartId, CodecError> {
        Ok(PartId::scoped(
            LocalScope::DERIVED,
            Self::next(&mut self.part)?,
        ))
    }
    fn group(&mut self) -> Result<GroupId, CodecError> {
        Ok(GroupId::new(
            LocalScope::DERIVED,
            Self::next(&mut self.group)?,
        ))
    }
}
type Restored = (Vec<(ItemId, Item)>, Vec<MessageEnvelope>);
/// Rebuild one carrier's owners. Any mismatch with the returned visible history fails
/// without touching the caller's records.
fn restore_marker(
    previous: &[(ItemId, Item)],
    packet: Packet,
    fidelity: &mut FidelityRecords,
    ids: &mut Ids,
) -> Result<Restored, CodecError> {
    if previous.iter().any(|(_, i)| !assistant(i)) {
        return Err(invalid());
    }
    let mut texts = TextCursor {
        parts: vec![],
        index: 0,
        offset: 0,
    };
    let mut barriers = Vec::new();
    let mut text_bytes = 0usize;
    for (id, item) in previous {
        match item {
            Item::Message(m) => {
                if m.parts
                    .iter()
                    .any(|p| !matches!(p.content, ContentPart::Text(_)) || p.replay.is_some())
                {
                    return Err(unsupported());
                }
                if m.parts.is_empty() && m.status != ItemLifecycle::Completed {
                    return Err(invalid());
                }
                texts.parts.extend(m.parts.iter().map(|part| TextPiece {
                    part,
                    status: m.status,
                    phase: m.phase,
                }));
                for part in &m.parts {
                    let ContentPart::Text(text) = &part.content else {
                        return Err(unsupported());
                    };
                    text_bytes = text_bytes
                        .checked_add(text.as_str().len())
                        .ok_or(CodecError::Limit)?;
                }
            }
            Item::ToolCall(_) | Item::Reasoning(_) => {
                barriers.push((*id, item.clone(), text_bytes))
            }
            _ => return Err(unsupported()),
        }
    }
    let mut barriers = barriers.into_iter();
    let mut consumed_text = 0usize;
    let mut restored = Vec::new();
    for entry in packet.items {
        restored.push(match entry {
            Entry::Message {
                status,
                phase,
                parts,
            } => {
                let phase = phase
                    .as_deref()
                    .map(|p| match p {
                        "commentary" => Ok(Phase::Commentary),
                        "final_answer" => Ok(Phase::FinalAnswer),
                        _ => Err(invalid()),
                    })
                    .transpose()?;
                let owner = ids.item()?;
                let mut content = Vec::new();
                for spec in parts {
                    let mut part = texts.take(spec.bytes, ids.part()?, status, phase, fidelity)?;
                    let ContentPart::Text(text) = &part.content else {
                        return Err(invalid());
                    };
                    if digest(&[text.as_str()]) != spec.hash {
                        return Err(invalid());
                    }
                    consumed_text = consumed_text
                        .checked_add(spec.bytes)
                        .ok_or(CodecError::Limit)?;
                    part.replay = replay(spec.replay.as_deref())?;
                    fidelity.record_part_replay(
                        owner,
                        status.into(),
                        content.len(),
                        &part,
                        None,
                    )?;
                    content.push(part);
                }
                (
                    owner,
                    Item::Message(Message {
                        role: MessageRole::Assistant,
                        status: status.into(),
                        phase,
                        parts: content,
                    }),
                )
            }
            Entry::Call {
                status: described,
                hash,
                replay: sealed,
            } => {
                let (owner, Item::ToolCall(mut call), offset) =
                    barriers.next().ok_or_else(invalid)?
                else {
                    return Err(invalid());
                };
                if offset != consumed_text {
                    return Err(invalid());
                }
                if call.replay.is_some() || call_hash(&call)? != hash {
                    return Err(invalid());
                }
                call.status = status(call.status, described)?;
                call.replay = replay(sealed.as_deref())?;
                fidelity.record_call_replay(owner, &call, None)?;
                (owner, Item::ToolCall(call))
            }
            Entry::Reasoning {
                status: described,
                hash,
                parts,
            } => {
                let (owner, Item::Reasoning(mut reasoning), offset) =
                    barriers.next().ok_or_else(invalid)?
                else {
                    return Err(invalid());
                };
                if offset != consumed_text {
                    return Err(invalid());
                }
                if reasoning_hash(&reasoning) != hash {
                    return Err(invalid());
                }
                let text = reasoning_text(&reasoning);
                let mut offset = 0usize;
                let mut content = Vec::new();
                for spec in parts {
                    let end = offset.checked_add(spec.bytes).ok_or(CodecError::Limit)?;
                    let value = text.get(offset..end).ok_or_else(invalid)?;
                    let value = Text::allowing_empty(value, "attachment reasoning", MAX_TEXT_BYTES)
                        .map_err(|_| invalid())?;
                    content.push((
                        ids.part()?,
                        if spec.summary {
                            ReasoningContent::Summary(value)
                        } else {
                            ReasoningContent::Text(value)
                        },
                    ));
                    offset = end;
                }
                if offset != text.len() {
                    return Err(invalid());
                }
                reasoning.parts = content;
                reasoning.status = status(reasoning.status, described)?;
                fidelity.remove_replay(owner);
                fidelity.record_replay(owner, &reasoning, None)?;
                (owner, Item::Reasoning(reasoning))
            }
        });
    }
    if !texts.finish() || barriers.next().is_some() {
        return Err(invalid());
    }
    let mut groups = Vec::with_capacity(packet.groups.len());
    for group in packet.groups {
        let id = ids.group()?;
        groups.push(if group.members.is_empty() {
            MessageEnvelope::empty(
                id,
                MessageEnvelopeRole::Assistant,
                group.before.map(|n| restored[n].0),
            )
        } else {
            if group.before.is_some() {
                return Err(invalid());
            }
            MessageEnvelope::new(
                id,
                MessageEnvelopeRole::Assistant,
                group.members.iter().map(|n| restored[*n].0).collect(),
            )
            .map_err(|_| invalid())?
        });
    }
    if restored.is_empty() {
        return Err(invalid());
    }
    Ok((restored, groups))
}

/// Restore once, before constructing the validated request. Unchanged visible projection
/// authorizes the original segmentation; this does not prove immutable client edit identity.
/// A carrier that no longer matches is dropped with a receipt: the visible history proceeds
/// without that turn's hidden state instead of rejecting the request.
pub(crate) fn restore(
    items: &mut Vec<(ItemId, Item)>,
    groups: &mut Vec<MessageEnvelope>,
    fidelity: &mut FidelityRecords,
    markers: Vec<Marker>,
) -> Result<(), CodecError> {
    if markers.is_empty() {
        return Ok(());
    }
    let mut ids = Ids {
        item: items.iter().map(|(id, _)| id.get()).max().unwrap_or(0),
        part: items
            .iter()
            .flat_map(|(_, item)| match item {
                Item::Message(m) => m.parts.iter().map(|p| p.id.get()).collect::<Vec<_>>(),
                Item::Reasoning(r) => r.parts.iter().map(|(id, _)| id.get()).collect(),
                _ => vec![],
            })
            .max()
            .unwrap_or(0),
        group: groups.iter().map(|g| g.id().get()).max().unwrap_or(0),
    };
    let mut preceding = 0;
    let mut added_parts = 0usize;
    let mut added_groups = 0usize;
    for marker in &markers {
        if marker.start < preceding || marker.end < marker.start || marker.end > items.len() {
            return Err(invalid());
        }
        preceding = marker.end;
        let Some(packet) = &marker.packet else {
            continue;
        };
        added_groups = added_groups
            .checked_add(packet.groups.len())
            .ok_or(CodecError::Limit)?;
        for entry in &packet.items {
            let count = match entry {
                Entry::Message { parts, .. } => parts.len(),
                Entry::Reasoning { parts, .. } => parts.len(),
                Entry::Call { .. } => 0,
            };
            added_parts = added_parts.checked_add(count).ok_or(CodecError::Limit)?;
        }
        if added_parts > MAX_ITEMS || added_groups > MAX_ITEMS {
            return Err(CodecError::Limit);
        }
    }
    let mut final_items = items.len();
    let mut replacements = Vec::with_capacity(markers.len());
    let mut removed_owners = BTreeMap::new();
    let mut restored_groups = Vec::with_capacity(added_groups);
    for (marker_index, marker) in markers.into_iter().enumerate() {
        let previous = &items[marker.start..marker.end];
        let restored = marker.packet.map(|packet| {
            let mut trial = fidelity.clone();
            restore_marker(previous, packet, &mut trial, &mut ids).map(|value| (value, trial))
        });
        let ((restored, packet_groups), trial) = match restored {
            Some(Ok(value)) => value,
            Some(Err(CodecError::Invalid(_) | CodecError::Unsupported(_))) | None => {
                fidelity.record_omission(crate::protocol::fidelity::REPLAY_MANIFEST_OMISSION);
                continue;
            }
            Some(Err(error)) => return Err(error),
        };
        final_items = final_items
            .checked_sub(previous.len())
            .and_then(|n| n.checked_add(restored.len()))
            .ok_or(CodecError::Limit)?;
        *fidelity = trial;
        for (id, _) in previous {
            removed_owners.insert(*id, (marker_index, restored[0].0));
        }
        restored_groups.extend(packet_groups);
        replacements.push((marker.start, marker.end, restored));
    }
    if final_items > MAX_ITEMS {
        return Err(CodecError::Limit);
    }
    let mut kept = Vec::new();
    for group in groups.drain(..) {
        if let Some((marker, _)) = group.members().iter().find_map(|id| removed_owners.get(id)) {
            if !group
                .members()
                .iter()
                .all(|id| removed_owners.get(id).is_some_and(|(n, _)| n == marker))
            {
                return Err(invalid());
            }
            continue;
        }
        if group.members().is_empty()
            && let Some((_, first)) = group.before().and_then(|id| removed_owners.get(&id))
        {
            kept.push(MessageEnvelope::empty(
                group.id(),
                group.role(),
                Some(*first),
            ));
        } else {
            kept.push(group);
        }
    }
    kept.extend(restored_groups);
    *groups = kept;
    let mut source = std::mem::take(items).into_iter();
    let mut output = Vec::with_capacity(final_items);
    let mut cursor = 0;
    for (start, end, restored) in replacements {
        output.extend(source.by_ref().take(start - cursor));
        for _ in start..end {
            source.next();
        }
        output.extend(restored);
        cursor = end;
    }
    output.extend(source);
    *items = output;
    let positions: BTreeMap<_, _> = items
        .iter()
        .enumerate()
        .map(|(n, (id, _))| (*id, n))
        .collect();
    groups.sort_by_key(|g| {
        g.members()
            .first()
            .or(g.before().as_ref())
            .and_then(|id| positions.get(id))
            .copied()
            .unwrap_or(items.len())
    });
    fidelity.retain_owners(items);
    Ok(())
}

pub(crate) fn strip(
    response: &DecodedResponse,
    strip_phase: bool,
) -> Result<DecodedResponse, CodecError> {
    let mut copy = response.clone();
    let mut items = copy.semantic.items().to_vec();
    for (id, item) in &mut items {
        match item {
            Item::Message(m) => {
                if strip_phase {
                    m.phase = None;
                }
                for p in &mut m.parts {
                    p.replay = None;
                    copy.fidelity.remove_part_replay(*id, p.id);
                }
            }
            Item::ToolCall(c) => {
                if c.replay.take().is_some() {
                    copy.fidelity.remove_replay(*id);
                }
            }
            _ => {}
        }
    }
    let source = &response.semantic;
    copy.semantic =
        GenerationResponse::from_resources(items, source.outcome(), source.resources().clone())?
            .with_message_envelopes(source.message_envelopes().to_vec())?
            .with_progress(source.progress())?
            .with_details(source.details().clone())?
            .with_safety_reports(source.safety_reports().to_vec())?
            .with_usage_reports(source.usage_reports().clone())?;
    Ok(copy)
}
/// Only the wire view drops attachments. The validated source owns them until EOF,
/// when the same static packet builder supplies the return carrier.
pub(crate) fn wire_event(event: &StreamEvent, source: &FidelityRecords) -> Option<StreamEvent> {
    if matches!(event, StreamEvent::PartReplay { .. }) {
        return None;
    }
    let mut event = event.clone();
    match &mut event {
        StreamEvent::ItemStarted {
            kind: ItemKind::ToolCall { .. },
            replay,
            ..
        } => *replay = None,
        StreamEvent::ItemFinished { item, replay, .. } if source.is_call_replay(*item) => {
            *replay = None
        }
        _ => {}
    }
    Some(event)
}
fn carrier_id(token: &str) -> String {
    format!("rs_mca_{}", &digest(&[token])[..32])
}
pub(crate) fn response_item(token: &str) -> Value {
    json!({
        "type": "reasoning",
        "id": carrier_id(token),
        "status": "completed",
        "summary": [],
        "encrypted_content": token,
    })
}
pub(crate) fn chat_detail(token: &str) -> Value {
    json!({
        "type": "reasoning.encrypted",
        "id": carrier_id(token),
        "format": "openai-responses-v1",
        "data": token,
    })
}
pub(crate) fn messages_block(token: &str) -> Value {
    json!({"type": "thinking", "thinking": "", "signature": token})
}

#[cfg(test)]
mod tests;
