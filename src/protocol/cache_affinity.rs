//! Bounded digests of final target prompts, never history or replay proofs.
use super::openai::Profile;
use serde_json::Value;
use sha2::{Digest, Sha256};

pub(crate) const MAX_CHECKPOINTS: usize = 32;
pub(crate) const MAX_PROMPT_BYTES: usize = 256 << 10;

/// Only call after final target encoding/validation. This does not grant admission.
pub(crate) fn prefixes(wire: &Value, profile: Profile) -> Option<Vec<[u8; 32]>> {
    crate::semantic::value::json_size(wire, MAX_PROMPT_BYTES).ok()?;
    let field = match profile {
        Profile::Chat => "messages",
        Profile::Responses => "input",
    };
    let items = wire.get(field)?.as_array()?;
    if items.is_empty() || !items.iter().all(|v| supported(v, profile)) {
        return None;
    }
    let mut base = wire.as_object()?.clone();
    base.shift_remove(field);
    for key in [
        "stream",
        "stream_options",
        "prompt_cache_key",
        "prompt_cache_retention",
        "prompt_cache_options",
        "metadata",
        "service_tier",
        "user",
        "safety_identifier",
        "include",
        "store",
    ] {
        base.shift_remove(key);
    }
    let mut hash = Sha256::new();
    hash.update(b"MorphieCore.cache-prefix.v1\0");
    hash.update([if profile == Profile::Chat { 0 } else { 1 }]);
    charge(&mut hash, &Value::Object(base))?;
    let mut checkpoints = std::collections::VecDeque::with_capacity(MAX_CHECKPOINTS);
    for item in items {
        charge(&mut hash, item)?;
        if checkpoints.len() == MAX_CHECKPOINTS {
            checkpoints.pop_front();
        }
        checkpoints.push_back(hash.clone().finalize().into());
    }
    Some(checkpoints.into())
}
fn charge(hash: &mut Sha256, value: &Value) -> Option<()> {
    let bytes = serde_json::to_vec(value).ok()?;
    hash.update((bytes.len() as u64).to_be_bytes());
    hash.update(bytes);
    Some(())
}
fn text_parts(value: Option<&Value>, kinds: &[&str]) -> bool {
    match value {
        None | Some(Value::Null | Value::String(_)) => true,
        Some(Value::Array(parts)) => parts.iter().all(|part| {
            part.get("type")
                .and_then(Value::as_str)
                .is_some_and(|kind| kinds.contains(&kind))
        }),
        _ => false,
    }
}
fn supported(value: &Value, profile: Profile) -> bool {
    if value.get("status").is_some_and(|s| s != "completed") {
        return false;
    }
    match profile {
        Profile::Chat => {
            matches!(
                value.get("role").and_then(Value::as_str),
                Some("system" | "developer" | "user" | "assistant" | "tool")
            ) && text_parts(value.get("content"), &["text"])
                && value.get("audio").is_none_or(Value::is_null)
                && value.get("refusal").is_none_or(Value::is_null)
                && value.get("function_call").is_none_or(Value::is_null)
                && value.get("tool_calls").is_none_or(|v| {
                    v.is_null()
                        || v.as_array().is_some_and(|calls| {
                            calls.iter().all(|call| {
                                call.get("type").and_then(Value::as_str) == Some("function")
                            })
                        })
                })
                && value.get("reasoning_details").is_none_or(|v| {
                    v.is_null()
                        || v.as_array().is_some_and(|parts| {
                            parts.iter().all(|part| {
                                matches!(
                                    part.get("type").and_then(Value::as_str),
                                    Some("reasoning.text" | "reasoning.summary")
                                )
                            })
                        })
                })
        }
        Profile::Responses => match value.get("type").and_then(Value::as_str) {
            None | Some("message") => {
                text_parts(value.get("content"), &["input_text", "output_text"])
            }
            Some("function_call") => true,
            Some("function_call_output") => {
                text_parts(value.get("output"), &["input_text", "output_text"])
            }
            Some("reasoning") => {
                value.get("encrypted_content").is_none_or(Value::is_null)
                    && text_parts(value.get("content"), &["reasoning_text"])
                    && text_parts(value.get("summary"), &["summary_text"])
            }
            _ => false,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn exact_prefixes_extend_without_changing_the_old_digest() {
        let first = json!({"model":"upstream","messages":[{"role":"user","content":"hello"}]});
        let mut next = first.clone();
        next["messages"]
            .as_array_mut()
            .unwrap()
            .push(json!({"role":"assistant","content":"answer"}));
        let one = prefixes(&first, Profile::Chat).unwrap();
        assert_eq!(
            one[0]
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>(),
            "fd9a9169570377a30c62a05dd35d1d307fbe342fa0f59355d8561614a2a46e84"
        );
        let two = prefixes(&next, Profile::Chat).unwrap();
        assert_eq!(one.len(), 1);
        assert_eq!(two.len(), 2);
        assert_eq!(one[0], two[0]);
        for field in ["stream", "stream_options", "prompt_cache_key", "metadata"] {
            let mut edited = next.clone();
            edited[field] = json!(null);
            assert_eq!(two, prefixes(&edited, Profile::Chat).unwrap());
        }
        for field in ["instructions", "tools", "temperature"] {
            let mut edited = next.clone();
            edited[field] = json!("changed");
            assert_ne!(two, prefixes(&edited, Profile::Chat).unwrap());
        }
        next["messages"][0]["content"] = json!("edited");
        assert_ne!(one[0], prefixes(&next, Profile::Chat).unwrap()[0]);
    }

    #[test]
    fn order_precision_identity_and_resource_boundaries_are_not_erased() {
        let base = br#"{"model":"upstream","input":[{"type":"function_call","call_id":"call","name":"lookup","arguments":"{\"z\":1.000000000000000001,\"a\":2}"}]}"#;
        let value: Value = serde_json::from_slice(base).unwrap();
        let original = prefixes(&value, Profile::Responses).unwrap();
        for arguments in [r#"{"a":2,"z":1.000000000000000001}"#, r#"{"z":1.0,"a":2}"#] {
            let mut edited = value.clone();
            edited["input"][0]["arguments"] = json!(arguments);
            assert_ne!(original, prefixes(&edited, Profile::Responses).unwrap());
        }
        let mut edited = value;
        edited["input"][0]["call_id"] = json!("another");
        assert_ne!(original, prefixes(&edited, Profile::Responses).unwrap());
        for content in [
            json!([{"type":"input_image","image_url":"https://example.invalid/image"}]),
            json!([{"type":"input_file","file_data":"AQID"}]),
        ] {
            assert!(
                prefixes(
                    &json!({"input":[{"type":"message","role":"user","content":content}]}),
                    Profile::Responses
                )
                .is_none()
            );
        }
        assert!(
            prefixes(
                &json!({"input":[{"type":"reasoning","encrypted_content":"synthetic"}]}),
                Profile::Responses
            )
            .is_none()
        );
        assert!(
            prefixes(
                &json!({"messages":[{"role":"user","content":"x".repeat(MAX_PROMPT_BYTES)}]}),
                Profile::Chat
            )
            .is_none()
        );
        let many = json!({"input":(0..100).map(|n|json!({"type":"message","role":"user","content":n.to_string()})).collect::<Vec<_>>()});
        assert_eq!(
            prefixes(&many, Profile::Responses).unwrap().len(),
            MAX_CHECKPOINTS
        );
    }
}
