//! Gateway-only transport of part/call ownership and message phases through client carriers.
//! The manifest stores boundaries and dependencies, never replacement business content.
mod carriers;
mod restore;
pub(crate) use carriers::{Carried, chat_detail, check_chat_support, is_marker, take_chat};
#[cfg(test)]
use restore::{Ids, restore_marker};
pub(crate) use restore::{Marker, restore};

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
            // Slot purpose was checked by the response's owner-bound fidelity.
            value.validate()?;
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
            // Restoration validates purpose when recording the specific part/call.
            replay.validate()?;
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
/// Only the wire view drops attachments. Execution must either retain typed managed
/// Responses history or provide the remaining legacy Chat return carrier.
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
#[cfg(test)]
mod tests;
