//! Downstream carrier envelope for upstream-issued reasoning signatures and compaction tokens.
//!
//! # Format
//!
//! Downstream clients receive opaque carrier strings encoded in the `mc1.` format:
//!
//! ```text
//! mc1.<base64url-json: {o: origin_digest, f: format_tag, d: payload}>
//! ```
//!
//! - `o`: Origin fingerprint binding provider, canonical model, and deployment route.
//! - `f`: Native format identifier (e.g. `anthropic-thinking`, `openai-responses`).
//! - `d`: Raw upstream payload string.
//!
//! # Security and Trust Model
//!
//! - **Tagged, Not Encrypted**: The envelope is tagged and Base64URL-encoded, not encrypted
//!   or authenticated by MorphieCore. The upstream issuer cryptographically signs or encrypts
//!   the underlying payload (`d`).
//! - **Route Isolation**: The origin tag (`o`) prevents cross-model contamination by ensuring
//!   that opaque tokens are only forwarded back to the matching upstream deployment route.
//! - **Opaque Client Representation**: Downstream clients must treat the entire `mc1.` string
//!   as opaque and echo it verbatim without modification or parsing.
use crate::semantic::{
    task::generation::{MAX_TEXT_BYTES, ReplayFormat},
    value::{ReplayOrigin, Text},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;

/// Canonical envelope prefix distinguishing MorphieCore replay envelopes.
pub const PREFIX: &str = "mc1.";

/// Client-source compatibility label, not execution or storage authorization.
pub static CLIENT_ORIGIN: std::sync::LazyLock<ReplayOrigin> =
    std::sync::LazyLock::new(|| ReplayOrigin::new("client").expect("static origin"));

/// Computes a stable compatibility label binding provider ID and canonical model.
pub fn issuer(provider: &str, model: &str) -> ReplayOrigin {
    fingerprint(&[provider, model])
}

/// Computes a strict runtime deployment identity fingerprint.
///
/// Binds provider, model, host origin, and endpoint path. Only the digest
/// crosses the client boundary.
pub(crate) fn deployment_issuer(
    provider: &str,
    model: &str,
    origin: &str,
    path: &str,
) -> ReplayOrigin {
    fingerprint(&["deployment-v1", provider, model, origin, path])
}

fn fingerprint(parts: &[&str]) -> ReplayOrigin {
    use sha2::{Digest, Sha256};
    let mut hash = Sha256::new();
    for value in parts {
        hash.update((value.len() as u64).to_le_bytes());
        hash.update(value.as_bytes());
    }
    let digest: String = hash.finalize().iter().map(|b| format!("{b:02x}")).collect();
    ReplayOrigin::new(&format!("issuer-{digest}")).expect("bounded issuer label")
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope<'a> {
    o: Cow<'a, str>,
    f: Cow<'a, str>,
    d: Cow<'a, str>,
}

/// Converts a semantic [`ReplayFormat`] to its wire tag string.
pub fn format_tag(format: ReplayFormat) -> &'static str {
    match format {
        ReplayFormat::ResponsesEncrypted => "openai-responses",
        ReplayFormat::ResponsesCompaction => "openai-compaction",
        ReplayFormat::AnthropicMessagesThinking => "anthropic-thinking",
        ReplayFormat::AnthropicMessagesRedactedThinking => "anthropic-redacted-thinking",
        ReplayFormat::GoogleGenerateContentPart => "gemini-part",
        ReplayFormat::GoogleGenerateContentSignature => "gemini-signature",
        ReplayFormat::GoogleInteractionsV1Thought => "interactions-thought",
        ReplayFormat::GoogleInteractionsV1FunctionCall => "interactions-function-call",
    }
}

/// Parses a wire tag string into a semantic [`ReplayFormat`].
pub fn parse_format(tag: &str) -> Option<ReplayFormat> {
    Some(match tag {
        "openai-responses" => ReplayFormat::ResponsesEncrypted,
        "openai-compaction" => ReplayFormat::ResponsesCompaction,
        "anthropic-thinking" => ReplayFormat::AnthropicMessagesThinking,
        "anthropic-redacted-thinking" => ReplayFormat::AnthropicMessagesRedactedThinking,
        "gemini-part" => ReplayFormat::GoogleGenerateContentPart,
        "gemini-signature" => ReplayFormat::GoogleGenerateContentSignature,
        "interactions-thought" => ReplayFormat::GoogleInteractionsV1Thought,
        "interactions-function-call" => ReplayFormat::GoogleInteractionsV1FunctionCall,
        _ => return None,
    })
}

/// Wraps an upstream opaque value in an `mc1.` carrier envelope.
///
/// Returns `None` if the payload or the resulting sealed envelope exceeds [`MAX_TEXT_BYTES`].
pub fn seal(origin: &ReplayOrigin, format: ReplayFormat, payload: &str) -> Option<Text> {
    if payload.len() > MAX_TEXT_BYTES {
        return None;
    }
    let json = serde_json::to_vec(&Envelope {
        o: origin.as_str().into(),
        f: format_tag(format).into(),
        d: payload.into(),
    })
    .ok()?;
    let sealed = format!("{PREFIX}{}", URL_SAFE_NO_PAD.encode(json));
    (sealed.len() <= MAX_TEXT_BYTES)
        .then(|| Text::new(&sealed, "replay envelope", MAX_TEXT_BYTES).ok())
        .flatten()
}

/// Unpacks and validates a client-provided `mc1.` carrier envelope.
///
/// Validates envelope formatting, JSON grammar, and format tags. Does not inspect
/// or authenticate the enclosed payload. Returns `None` if malformed, unknown, or oversized.
pub fn open(value: &str) -> Option<(ReplayOrigin, ReplayFormat, Text)> {
    if value.len() > MAX_TEXT_BYTES {
        return None;
    }
    let encoded = value.strip_prefix(PREFIX)?;
    let json = URL_SAFE_NO_PAD.decode(encoded).ok()?;
    let value =
        crate::semantic::value::parse_json(&json, crate::semantic::value::JsonLimits::ENVELOPE)
            .ok()?;
    let envelope: Envelope<'_> = serde_json::from_value(value).ok()?;
    Some((
        ReplayOrigin::new(&envelope.o).ok()?,
        parse_format(&envelope.f)?,
        Text::new(envelope.d.as_ref(), "replay payload", MAX_TEXT_BYTES).ok()?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sealed_values_round_trip_with_origin_and_format() {
        let origin = ReplayOrigin::new("anthropic/claude-synthetic").unwrap();
        let sealed = seal(
            &origin,
            ReplayFormat::AnthropicMessagesThinking,
            "synthetic-signature/+=",
        )
        .unwrap();
        assert!(sealed.as_str().starts_with(PREFIX));
        assert!(!sealed.as_str().contains("synthetic-signature"));
        let (o, f, d) = open(sealed.as_str()).unwrap();
        assert_eq!(o, origin);
        assert_eq!(f, ReplayFormat::AnthropicMessagesThinking);
        assert_eq!(d.as_str(), "synthetic-signature/+=");
    }

    #[test]
    fn foreign_or_damaged_values_have_no_origin() {
        let origin = ReplayOrigin::new("openai/model").unwrap();
        let sealed = seal(&origin, ReplayFormat::ResponsesEncrypted, "token").unwrap();
        for value in [
            "raw-upstream-token",
            "mc1.",
            "mc1.!!!",
            &sealed.as_str()[..sealed.as_str().len() - 2],
            &format!(
                "{PREFIX}{}",
                URL_SAFE_NO_PAD.encode(br#"{"o":"x","f":"unknown","d":"t"}"#)
            ),
        ] {
            assert!(open(value).is_none(), "{value}");
        }
    }

    #[test]
    fn oversized_payloads_are_not_sealed() {
        let origin = ReplayOrigin::new("p/m").unwrap();
        let payload = "x".repeat(MAX_TEXT_BYTES);
        assert!(seal(&origin, ReplayFormat::ResponsesEncrypted, &payload).is_none());
    }

    #[test]
    fn independent_wire_and_escaped_payload_are_decoded() {
        let body = br#"{"o":"issuer","f":"anthropic-thinking","d":"quoted\"\\payload"}"#;
        let wire = format!("mc1.{}", URL_SAFE_NO_PAD.encode(body));
        let (origin, format, payload) = open(&wire).unwrap();
        assert_eq!(origin.as_str(), "issuer");
        assert_eq!(format, ReplayFormat::AnthropicMessagesThinking);
        assert_eq!(payload.as_str(), "quoted\"\\payload");
        assert_eq!(
            seal(&origin, format, payload.as_str()).unwrap().as_str(),
            wire
        );
        assert!(open(&format!("mc1.{}", "a".repeat(MAX_TEXT_BYTES))).is_none());
    }

    #[test]
    fn issuer_uses_both_labels_without_concatenation_ambiguity() {
        assert_eq!(issuer("p", "m"), issuer("p", "m"));
        assert_ne!(issuer("p", "m"), issuer("q", "m"));
        assert_ne!(issuer("p", "m"), issuer("p", "n"));
        assert_ne!(issuer("ab", "c"), issuer("a", "bc"));
    }
}
