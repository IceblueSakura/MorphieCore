//! Closed inference-server placeholders and reported usage aliases; no payload fidelity.
use super::{
    CodecError,
    common::{fields, object},
};
use serde_json::{Map, Value};

fn inactive(object: &mut Map<String, Value>, names: &[&str]) -> Result<(), CodecError> {
    for name in names {
        if let Some(value) = object.shift_remove(*name)
            && !value.is_null()
        {
            return Err(CodecError::Unsupported(
                "active inference-server field".into(),
            ));
        }
    }
    Ok(())
}
fn counter(object: &mut Map<String, Value>, name: &str, value: Value) -> Result<(), CodecError> {
    if value.is_null() {
        return Ok(());
    }
    if value.as_u64().is_none()
        || object
            .get(name)
            .is_some_and(|old| !old.is_null() && old != &value)
    {
        return Err(CodecError::Invalid("inference usage alias"));
    }
    object.insert(name.into(), value);
    Ok(())
}
pub(super) fn decode(root: &mut Map<String, Value>) -> Result<(), CodecError> {
    // These optional transport/debug fields have no semantic owner when null.
    // Active values still require a dedicated supported mapping.
    // https://github.com/vllm-project/vllm/blob/main/vllm/entrypoints/openai/chat_completion/protocol.py
    inactive(
        root,
        &[
            "prompt_logprobs",
            "prompt_token_ids",
            "prompt_text",
            "kv_transfer_params",
            "ec_transfer_params",
            "metrics",
        ],
    )?;
    if let Some(choices) = root.get_mut("choices").and_then(Value::as_array_mut) {
        for choice in choices {
            if let Some(choice) = choice.as_object_mut() {
                inactive(choice, &["stop_reason", "token_ids", "routed_experts"])?;
            }
        }
    }
    if let Some(details) = root
        .get_mut("usage")
        .and_then(Value::as_object_mut)
        .and_then(|usage| usage.get_mut("prompt_tokens_details"))
        .filter(|v| !v.is_null())
    {
        let details = details
            .as_object_mut()
            .ok_or(CodecError::Invalid("inference usage details"))?;
        // Both aliases report existing prompt-token subsets, not new totals.
        // https://github.com/vllm-project/vllm/blob/main/vllm/entrypoints/openai/chat_completion/serving.py
        if let Some(value) = details.shift_remove("created_cache_tokens") {
            counter(details, "cache_write_tokens", value)?;
        }
        if let Some(value) = details
            .shift_remove("multimodal_tokens")
            .filter(|v| !v.is_null())
        {
            let values = object(&value)?;
            fields(values, &["image", "audio", "video"])?;
            for name in ["audio", "video"] {
                if values
                    .get(name)
                    .is_some_and(|v| !v.is_null() && v.as_u64() != Some(0))
                {
                    return Err(CodecError::Unsupported(
                        "active unmapped modality usage".into(),
                    ));
                }
            }
            if let Some(images) = values.get("image") {
                counter(details, "image_tokens", images.clone())?;
            }
        }
    }
    Ok(())
}
