//! Chat choice probabilities are owned by the matching text/refusal part.
//! Missing/null carriers mean unreported; a reported empty array is still a fact.
//! Pinned shapes: https://github.com/openai/openai-python/blob/be9d66628ad7377bd36fe5a76ae6d735843f0e76/src/openai/types/chat/chat_completion_token_logprob.py
use super::{CodecError, common::*};
use crate::semantic::{task::generation::*, value::Presence};
use serde_json::{Value, json};

pub(crate) fn validate(probs: &[Logprob]) -> Result<(), CodecError> {
    validate_logprobs(probs)?;
    if probs.iter().any(|p| {
        p.top_logprobs
            .as_ref()
            .is_none_or(|top| top.iter().any(|p| p.token.is_none() || p.logprob.is_none()))
    }) {
        return Err(CodecError::Invalid("Chat probability details"));
    }
    Ok(())
}
pub(super) fn read(value: Option<&Value>) -> Result<Vec<(PartKind, Vec<Logprob>)>, CodecError> {
    let Some(value) = value.filter(|v| !v.is_null()) else {
        return Ok(vec![]);
    };
    let o = object(value)?;
    fields(o, &["content", "refusal"])?;
    let mut result = Vec::new();
    for (key, kind) in [("content", PartKind::Text), ("refusal", PartKind::Refusal)] {
        if let Some(value) = o.get(key).filter(|v| !v.is_null()) {
            let probs = super::text::read_logprobs(value)?;
            validate(&probs)?;
            result.push((kind, probs));
        }
    }
    Ok(result)
}
/// Byte representations are optional in Chat; never infer them from token strings.
pub(super) fn write(probs: &[Logprob]) -> Value {
    json!(probs.iter().map(|p| json!({"token":p.token,"logprob":p.logprob,"bytes":p.bytes,
        "top_logprobs":p.top_logprobs.as_deref().expect("validated Chat probabilities").iter().map(|p|json!({"token":p.token,"logprob":p.logprob,"bytes":p.bytes})).collect::<Vec<_>>()
    })).collect::<Vec<_>>())
}
pub(super) fn carrier(
    kind: PartKind,
    probs: &Presence<Vec<Logprob>>,
) -> Result<Option<Value>, CodecError> {
    let key = match kind {
        PartKind::Text => "content",
        PartKind::Refusal => "refusal",
        _ => return Err(CodecError::Invalid("probability owner")),
    };
    Ok(match probs {
        Presence::Absent => None,
        Presence::Null => Some(json!({key:Value::Null})),
        Presence::Value(probs) => {
            validate(probs)?;
            Some(json!({key:write(probs)}))
        }
    })
}
pub(super) fn attach(
    items: &mut [(ItemId, Item)],
    value: Option<&Value>,
) -> Result<(), CodecError> {
    for (kind, probs) in read(value)? {
        let content = items
            .iter_mut()
            .find_map(|(_, item)| match item {
                Item::Message(message) => {
                    message
                        .parts
                        .iter_mut()
                        .find_map(|p| match (&p.content, kind) {
                            (ContentPart::Text(_), PartKind::Text)
                            | (ContentPart::Refusal(_), PartKind::Refusal) => Some(&mut p.content),
                            _ => None,
                        })
                }
                _ => None,
            })
            .ok_or(CodecError::Invalid("probability owner"))?;
        *content = match content {
            ContentPart::Text(text) => {
                ContentPart::Text(text.clone().with_logprobs(Presence::Value(probs))?)
            }
            ContentPart::Refusal(text) => {
                ContentPart::Refusal(text.clone().with_logprobs(Presence::Value(probs))?)
            }
            _ => unreachable!(),
        };
    }
    Ok(())
}
pub(super) fn choice(items: &[(ItemId, Item)]) -> Result<Option<Value>, CodecError> {
    let mut result = None;
    for (_, item) in items {
        if let Item::Message(message) = item {
            for part in &message.parts {
                let next = match &part.content {
                    ContentPart::Text(text) => carrier(PartKind::Text, text.logprobs())?,
                    ContentPart::Refusal(text) => carrier(PartKind::Refusal, text.logprobs())?,
                    _ => None,
                };
                if let Some(next) = next {
                    if result.is_some() {
                        return Err(CodecError::Invalid("multiple probability owners"));
                    }
                    result = Some(next);
                }
            }
        }
    }
    Ok(result)
}
