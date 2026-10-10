//! Gateway-owned continuation capsules. Key loading and public activation are external.
use super::{
    CodecError, DecodedRequest,
    anthropic::{ReplayTarget, prefix_digest},
};
use crate::semantic::{
    task::generation::*,
    value::{JsonLimits, Text, parse_json},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use ring::aead::{AES_256_GCM, Aad, LessSafeKey, Nonce, UnboundKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zeroize::{Zeroize, Zeroizing};

const PREFIX: &str = "mcgr2.";
const NONCE_BYTES: usize = 12;
const AAD_DOMAIN: &[u8] = b"MorphieCore.GatewayReasoning.v2";
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ContainerReceipt {
    assistant: bool,
    members: Vec<usize>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Capsule {
    version: u8,
    provider: String,
    model: String,
    prefix: [u8; 32],
    readable: [u8; 32],
    signature: String,
    expires_at: u64,
    owner: usize,
    containers: Vec<ContainerReceipt>,
}
impl Drop for Capsule {
    fn drop(&mut self) {
        self.signature.zeroize();
    }
}
pub struct GatewayReplayCodec {
    key: LessSafeKey,
}
impl std::fmt::Debug for GatewayReplayCodec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("GatewayReplayCodec([redacted])")
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReplayDisposition {
    Restored,
    DiscardedOnTargetChange,
}
pub struct RestoredReplay {
    pub request: DecodedRequest,
    pub disposition: ReplayDisposition,
}
fn invalid() -> CodecError {
    CodecError::Invalid("gateway continuation")
}
fn aad(principal: &str) -> Result<Vec<u8>, CodecError> {
    if principal.is_empty() || principal.len() > 256 {
        return Err(invalid());
    }
    let mut bytes = AAD_DOMAIN.to_vec();
    bytes.extend_from_slice(&(principal.len() as u64).to_le_bytes());
    bytes.extend_from_slice(principal.as_bytes());
    Ok(bytes)
}
fn reasoning(request: &GenerationRequest, owner: ItemId) -> Result<&ReasoningItem, CodecError> {
    match request.items().iter().find(|(id, _)| *id == owner) {
        Some((_, Item::Reasoning(r))) if r.status == ItemLifecycle::Completed => Ok(r),
        _ => Err(invalid()),
    }
}
fn readable(r: &ReasoningItem) -> Result<[u8; 32], CodecError> {
    if r.parts.len() > 1 {
        return Err(invalid());
    }
    let values = r
        .parts
        .iter()
        .map(|(_, part)| {
            let ReasoningContent::Summary(t) = part else {
                return Err(invalid());
            };
            Ok(serde_json::json!({"type":"summary_text","text":t.as_str()}))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Sha256::digest(serde_json::to_vec(&values).map_err(|_| invalid())?).into())
}
fn container_receipts(
    source: &GenerationRequest,
    owner: usize,
) -> Result<Vec<ContainerReceipt>, CodecError> {
    source
        .message_envelopes()
        .iter()
        .map(|group| {
            let members = group
                .members()
                .iter()
                .map(|id| {
                    source
                        .items()
                        .iter()
                        .position(|(candidate, _)| candidate == id)
                        .ok_or_else(invalid)
                })
                .collect::<Result<Vec<_>, _>>()?;
            let members = members
                .into_iter()
                .take_while(|position| *position <= owner)
                .collect::<Vec<_>>();
            let assistant = match group.role() {
                MessageEnvelopeRole::Assistant => true,
                MessageEnvelopeRole::User => false,
                _ => return Err(invalid()),
            };
            Ok(ContainerReceipt { assistant, members })
        })
        .collect::<Result<Vec<_>, _>>()
        .map(|groups| {
            groups
                .into_iter()
                .filter(|g| !g.members.is_empty())
                .collect()
        })
}
fn restore_containers(
    input: &GenerationRequest,
    owner: ItemId,
    capsule: &Capsule,
) -> Result<GenerationRequest, CodecError> {
    let items = input.items();
    if capsule.owner >= items.len()
        || items[capsule.owner].0 != owner
        || capsule.containers.len() > MAX_ITEMS
    {
        return Err(invalid());
    }
    if !input.message_envelopes().is_empty() {
        return Ok(input.clone());
    }
    let mut groups = Vec::new();
    let mut last = None;
    for receipt in &capsule.containers {
        if receipt.members.is_empty() || receipt.members.len() > MAX_ITEMS {
            return Err(invalid());
        }
        let mut members = Vec::new();
        for position in &receipt.members {
            if *position > capsule.owner || last.is_some_and(|last| *position <= last) {
                return Err(invalid());
            }
            members.push(items.get(*position).ok_or_else(invalid)?.0);
            last = Some(*position);
        }
        groups.push(MessageEnvelope::new(
            GroupId::new(owner.scope(), groups.len() as u64 + 1),
            if receipt.assistant {
                MessageEnvelopeRole::Assistant
            } else {
                MessageEnvelopeRole::User
            },
            members,
        )?);
    }
    if last != Some(capsule.owner) {
        return Err(invalid());
    }
    // Only the authenticated prefix is source ownership. Later independent
    // Responses values are assembled for this target, never moved or invented.
    for (id, item) in &items[capsule.owner + 1..] {
        let role = match item {
            Item::Message(m) if m.role == MessageRole::User => MessageEnvelopeRole::User,
            Item::Message(m) if m.role == MessageRole::Assistant => MessageEnvelopeRole::Assistant,
            Item::Reasoning(_) | Item::ToolCall(_) => MessageEnvelopeRole::Assistant,
            Item::ToolResult(_) => MessageEnvelopeRole::User,
            _ => return Err(invalid()),
        };
        if role == MessageEnvelopeRole::Assistant && groups.last().is_some_and(|g| g.role() == role)
        {
            let group = groups.pop().ok_or_else(invalid)?;
            let mut members = group.members().to_vec();
            members.push(*id);
            groups.push(MessageEnvelope::new(group.id(), role, members)?);
        } else {
            groups.push(MessageEnvelope::new(
                GroupId::new(owner.scope(), groups.len() as u64 + 1),
                role,
                vec![*id],
            )?);
        }
    }
    Ok(input.clone().with_message_envelopes(groups)?)
}
impl GatewayReplayCodec {
    /// Caller-owned key only. No default, filesystem access, generation, rotation or registry.
    pub fn new(mut key: [u8; 32]) -> Result<Self, CodecError> {
        let unbound = UnboundKey::new(&AES_256_GCM, &key);
        key.zeroize();
        Ok(Self {
            key: LessSafeKey::new(unbound.map_err(|_| invalid())?),
        })
    }
    /// `principal` must come from authenticated ingress, never request metadata.
    pub fn seal(
        &self,
        source: &DecodedRequest,
        owner: ItemId,
        target: &ReplayTarget,
        principal: &str,
        expires_at: u64,
    ) -> Result<String, CodecError> {
        source.semantic.validate()?;
        source
            .fidelity
            .require_reasoning_replay(&source.semantic, &target.origin())?;
        let r = reasoning(&source.semantic, owner)?;
        let replay = r.replay.as_ref().ok_or_else(invalid)?;
        if replay.format() != ReplayFormat::AnthropicMessagesThinking {
            return Err(invalid());
        }
        let signature = replay.replay_token().ok_or_else(invalid)?;
        let capsule = Capsule {
            version: 2,
            provider: target.provider().into(),
            model: target.model().into(),
            prefix: prefix_digest(&source.semantic, owner, &source.fidelity, target)?,
            readable: readable(r)?,
            signature: signature.into(),
            expires_at,
            owner: source
                .semantic
                .items()
                .iter()
                .position(|(id, _)| *id == owner)
                .ok_or_else(invalid)?,
            containers: container_receipts(
                &source.semantic,
                source
                    .semantic
                    .items()
                    .iter()
                    .position(|(id, _)| *id == owner)
                    .ok_or_else(invalid)?,
            )?,
        };
        let mut body = Zeroizing::new(serde_json::to_vec(&capsule).map_err(|_| invalid())?);
        if body.len()
            > (MAX_TEXT_BYTES - PREFIX.len()) / 4 * 3 - NONCE_BYTES - AES_256_GCM.tag_len()
        {
            return Err(CodecError::Limit);
        }
        let aad = aad(principal)?;
        let mut nonce = [0; NONCE_BYTES];
        getrandom::fill(&mut nonce).map_err(|_| invalid())?;
        self.key
            .seal_in_place_append_tag(
                Nonce::assume_unique_for_key(nonce),
                Aad::from(aad),
                &mut *body,
            )
            .map_err(|_| invalid())?;
        let mut packed = nonce.to_vec();
        packed.extend_from_slice(&body);
        let token = format!("{PREFIX}{}", URL_SAFE_NO_PAD.encode(packed));
        if token.len() > MAX_TEXT_BYTES {
            return Err(CodecError::Limit);
        }
        Ok(token)
    }
    /// Restore only after authentication and cross-request prefix verification.
    /// Changed Provider/Model yields a target-local copy without any opaque reasoning.
    pub fn restore(
        &self,
        input: &DecodedRequest,
        owner: ItemId,
        target: &ReplayTarget,
        principal: &str,
        now: u64,
    ) -> Result<RestoredReplay, CodecError> {
        input.semantic.validate()?;
        let r = reasoning(&input.semantic, owner)?;
        if r.replay
            .as_ref()
            .is_none_or(|v| v.format() != ReplayFormat::GatewayContinuation)
        {
            return Err(invalid());
        }
        let token = r
            .replay
            .as_ref()
            .and_then(ReplayValue::replay_token)
            .ok_or_else(invalid)?;
        if token.len() > MAX_TEXT_BYTES {
            return Err(CodecError::Limit);
        }
        let encoded = token.strip_prefix(PREFIX).ok_or_else(invalid)?;
        let mut packed = Zeroizing::new(URL_SAFE_NO_PAD.decode(encoded).map_err(|_| invalid())?);
        if packed.len() < NONCE_BYTES + AES_256_GCM.tag_len() {
            return Err(invalid());
        }
        let nonce: [u8; NONCE_BYTES] = packed[..NONCE_BYTES].try_into().map_err(|_| invalid())?;
        let plaintext = self
            .key
            .open_in_place(
                Nonce::assume_unique_for_key(nonce),
                Aad::from(aad(principal)?),
                &mut packed[NONCE_BYTES..],
            )
            .map_err(|_| invalid())?;
        let value = parse_json(plaintext, JsonLimits::ENVELOPE).map_err(|_| invalid())?;
        let capsule: Capsule = serde_json::from_value(value).map_err(|_| invalid())?;
        if capsule.version != 2 || now >= capsule.expires_at {
            return Err(invalid());
        }
        let mut request = input.clone();
        let mut items = input.semantic.items().to_vec();
        if capsule.provider != target.provider() || capsule.model != target.model() {
            for (id, item) in &mut items {
                if let Item::Reasoning(r) = item {
                    r.replay = None;
                    request.fidelity.remove_replay(*id);
                }
            }
            items.retain(|(_, item)| !matches!(item,Item::Reasoning(r) if r.parts.is_empty()));
            request.semantic = request.semantic.with_items(items)?;
            return Ok(RestoredReplay {
                request,
                disposition: ReplayDisposition::DiscardedOnTargetChange,
            });
        }
        request.semantic = restore_containers(&input.semantic, owner, &capsule)?;
        if capsule.readable != readable(r)?
            || capsule.prefix != prefix_digest(&request.semantic, owner, &input.fidelity, target)?
        {
            return Err(invalid());
        }
        let Item::Reasoning(r) = &mut items
            .iter_mut()
            .find(|(id, _)| *id == owner)
            .ok_or_else(invalid)?
            .1
        else {
            return Err(invalid());
        };
        r.replay = Some(ReplayValue::final_value(
            ReplayFormat::AnthropicMessagesThinking,
            Text::new(&capsule.signature, "native signature", MAX_TEXT_BYTES)
                .map_err(|_| invalid())?,
        ));
        request.semantic = request.semantic.with_items(items)?;
        let r = reasoning(&request.semantic, owner)?;
        // The previous wire binding is a different format. Replace it only after
        // authenticated capsule contents matched the final caller-supplied history.
        request.fidelity.remove_replay(owner);
        request
            .fidelity
            .record_replay(owner, r, Some(target.origin()))?;
        let proof = RequestDependencyProof::capture(
            &request.semantic,
            HistoryDependency::PrefixThrough(owner),
            SettingsDependency::only(SettingsField::Instructions)
                .union(SettingsDependency::only(SettingsField::Tools)),
        )?;
        request
            .fidelity
            .bind_replay_dependency(owner, proof, &request.semantic)?;
        Ok(RestoredReplay {
            request,
            disposition: ReplayDisposition::Restored,
        })
    }
    /// Explicit Gateway carrier; no native signature is relabeled as Provider encryption.
    pub fn seal_responses_reasoning_item(
        &self,
        source: &DecodedRequest,
        owner: ItemId,
        target: &ReplayTarget,
        principal: &str,
        expires_at: u64,
        item_id: &str,
    ) -> Result<serde_json::Value, CodecError> {
        if item_id.is_empty()
            || item_id.len() > 64
            || !item_id
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-'))
        {
            return Err(CodecError::Invalid("gateway reasoning id"));
        }
        let token = self.seal(source, owner, target, principal, expires_at)?;
        let mut item = reasoning(&source.semantic, owner)?.clone();
        item.replay = Some(ReplayValue::final_value(
            ReplayFormat::GatewayContinuation,
            Text::new(token, "gateway continuation", MAX_TEXT_BYTES).map_err(|_| invalid())?,
        ));
        let mut wire = super::openai::encode_reasoning_item(&item, true);
        wire["id"] = serde_json::json!(item_id);
        Ok(wire)
    }
    /// Authenticated dispatch selected by the caller's Gateway boundary, not a prefix heuristic.
    pub fn restore_responses_replay(
        &self,
        input: &DecodedRequest,
        owner: ItemId,
        target: &ReplayTarget,
        principal: &str,
        now: u64,
    ) -> Result<RestoredReplay, CodecError> {
        let mut request = input.clone();
        let mut items = input.semantic.items().to_vec();
        let Item::Reasoning(item) = &mut items
            .iter_mut()
            .find(|(id, _)| *id == owner)
            .ok_or_else(invalid)?
            .1
        else {
            return Err(invalid());
        };
        let replay = item
            .replay
            .as_ref()
            .filter(|r| r.format() == ReplayFormat::ResponsesEncrypted)
            .ok_or_else(invalid)?;
        item.replay = Some(ReplayValue::final_value(
            ReplayFormat::GatewayContinuation,
            Text::new(
                replay.replay_token().ok_or_else(invalid)?,
                "gateway continuation",
                MAX_TEXT_BYTES,
            )
            .map_err(|_| invalid())?,
        ));
        request.semantic = request.semantic.with_items(items)?;
        self.restore(&request, owner, target, principal, now)
    }
}
