//! OpenAI stream obfuscation shared by Chat and Responses.
use crate::{
    protocol::stream::SseError,
    semantic::{task::generation::MAX_TOTAL_BYTES, value::json_size},
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
/// Supply an unpredictable per-response seed from the trusted I/O boundary in non-test use.
/// The codec itself uses no random source, clock, filesystem, credential or network client.
pub enum Obfuscation {
    Disabled,
    Seeded([u8; 32]),
}
pub(in crate::protocol::openai) fn pad_payload(
    value: &mut Value,
    seed: &[u8; 32],
    ordinal: usize,
    limit: usize,
    charged: &mut usize,
    maximum: usize,
) -> Result<(), SseError> {
    value["obfuscation"] = json!("");
    let size = json_size(value, MAX_TOTAL_BYTES).map_err(|_| SseError::Limit)?;
    if size > limit {
        return Err(SseError::Limit);
    }
    let n = (size.saturating_add(1023) / 1024 * 1024).min(limit) - size;
    // Charge before allocation; never silently disable requested padding on exhaustion.
    charge_padding(charged, n, maximum)?;
    let mut pad = String::with_capacity(n);
    let mut block = 0u64;
    while pad.len() < n {
        let mut hash = Sha256::new();
        hash.update(seed);
        hash.update((ordinal as u64).to_le_bytes());
        hash.update(block.to_le_bytes());
        for b in hash.finalize() {
            use std::fmt::Write;
            write!(&mut pad, "{b:02x}").expect("String formatting");
        }
        block += 1;
    }
    pad.truncate(n);
    value["obfuscation"] = json!(pad);
    Ok(())
}
pub(in crate::protocol::openai) fn charge_padding(
    total: &mut usize,
    bytes: usize,
    maximum: usize,
) -> Result<(), SseError> {
    let next = total.checked_add(bytes).ok_or(SseError::Limit)?;
    if next > maximum {
        return Err(SseError::Limit);
    }
    *total = next;
    Ok(())
}
