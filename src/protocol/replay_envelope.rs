//! Downstream carrier for upstream-issued reasoning and compaction content.
//! The envelope is tagged, not encrypted or authenticated: the upstream
//! issuer validates the payload, and the tag only decides whether a value may return
//! to the same Provider/model. Clients must treat the whole string as opaque.
use crate::semantic::{
    task::generation::{MAX_TEXT_BYTES, ReplayFormat},
    value::{ReplayOrigin, Text},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;

const PREFIX: &str = "mc1.";

/// Stable compatibility label; neither credentials nor public aliases participate.
pub fn issuer(provider: &str, model: &str) -> ReplayOrigin {
    use sha2::{Digest, Sha256};
    let mut hash = Sha256::new();
    for value in [provider, model] {
        hash.update((value.len() as u64).to_le_bytes());
        hash.update(value.as_bytes());
    }
    let digest: String = hash.finalize().iter().map(|b| format!("{b:02x}")).collect();
    ReplayOrigin::new(&format!("issuer-{digest}")).expect("bounded issuer label")
}

#[derive(Serialize, Deserialize)]
struct Envelope<'a> {
    o: Cow<'a, str>,
    f: Cow<'a, str>,
    d: Cow<'a, str>,
}

fn format_tag(format: ReplayFormat) -> &'static str {
    match format {
        ReplayFormat::ResponsesEncrypted => "openai-responses",
        ReplayFormat::ResponsesCompaction => "openai-compaction",
        ReplayFormat::AnthropicMessagesThinking => "anthropic-thinking",
        ReplayFormat::AnthropicMessagesRedactedThinking => "anthropic-redacted-thinking",
        ReplayFormat::GoogleGenerateContentPart => "gemini-part",
        ReplayFormat::GoogleInteractionsV1Thought => "interactions-thought",
        ReplayFormat::GoogleInteractionsV1Step => "interactions-step",
    }
}
fn parse_format(tag: &str) -> Option<ReplayFormat> {
    Some(match tag {
        "openai-responses" => ReplayFormat::ResponsesEncrypted,
        "openai-compaction" => ReplayFormat::ResponsesCompaction,
        "anthropic-thinking" => ReplayFormat::AnthropicMessagesThinking,
        "anthropic-redacted-thinking" => ReplayFormat::AnthropicMessagesRedactedThinking,
        "gemini-part" => ReplayFormat::GoogleGenerateContentPart,
        "interactions-thought" => ReplayFormat::GoogleInteractionsV1Thought,
        "interactions-step" => ReplayFormat::GoogleInteractionsV1Step,
        _ => return None,
    })
}

/// Wrap one upstream value for the downstream carrier. Fails only when the result
/// would exceed the shared text limit.
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

/// Parse a client carrier. This validates its shape, not its claimed issuer.
/// `None` means malformed, unsupported or oversized input.
pub fn open(value: &str) -> Option<(ReplayOrigin, ReplayFormat, Text)> {
    if value.len() > MAX_TEXT_BYTES {
        return None;
    }
    let encoded = value.strip_prefix(PREFIX)?;
    let json = URL_SAFE_NO_PAD.decode(encoded).ok()?;
    let envelope: Envelope<'_> = serde_json::from_slice(&json).ok()?;
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
