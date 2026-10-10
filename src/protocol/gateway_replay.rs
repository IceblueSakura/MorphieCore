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

const PREFIX: &str = "mcgr1.";
const NONCE_BYTES: usize = 12;
const AAD_DOMAIN: &[u8] = b"MorphieCore.GatewayReasoning.v1";
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
            version: 1,
            provider: target.provider().into(),
            model: target.model().into(),
            prefix: prefix_digest(&source.semantic, owner, &source.fidelity, target)?,
            readable: readable(r)?,
            signature: signature.into(),
            expires_at,
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
        if capsule.version != 1 || now >= capsule.expires_at {
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
        if capsule.readable != readable(r)?
            || capsule.prefix != prefix_digest(&input.semantic, owner, &input.fidelity, target)?
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
}
