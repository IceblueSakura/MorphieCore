//! Standard Chat audio request/complete-message mappings, with no format inference.
//! https://github.com/openai/openai-python/blob/be9d66628ad7377bd36fe5a76ae6d735843f0e76/src/openai/types/chat/chat_completion_audio.py
use super::{CodecError, common::*};
use crate::semantic::{task::generation::*, value::Text};
use serde_json::{Map, Value, json};
pub(super) fn settings(
    o: &Map<String, Value>,
    s: &mut GenerationSettings,
) -> Result<(), CodecError> {
    s.output_modalities = read_presence(o, "modalities", |v| {
        let v = v.as_array().ok_or(CodecError::Invalid("modalities"))?;
        if v.len() > 2 {
            return Err(CodecError::Limit);
        }
        v.iter()
            .map(|v| {
                v.as_str()
                    .ok_or(CodecError::Invalid("modality"))?
                    .parse()
                    .map_err(|_| CodecError::Unsupported("output modality".into()))
            })
            .collect()
    })?;
    s.audio = read_presence(o, "audio", |v| {
        let o = object(v)?;
        fields(o, &["format", "voice"])?;
        Ok(AudioOutputOptions {
            format: string(o, "format")?
                .parse()
                .map_err(|_| CodecError::Unsupported("audio format".into()))?,
            voice: text(string(o, "voice")?, "voice", 128)?,
        })
    })?;
    Ok(())
}
pub(super) fn write_settings(s: &GenerationSettings, o: &mut Map<String, Value>) {
    put_presence(o, "modalities", &s.output_modalities, |v| {
        json!(v.iter().map(|v| v.label()).collect::<Vec<_>>())
    });
    put_presence(
        o,
        "audio",
        &s.audio,
        |v| json!({"format":v.format.label(),"voice":v.voice.as_str()}),
    );
}
pub(super) fn read(value: &Value, history: bool) -> Result<ContentPart, CodecError> {
    let o = object(value)?;
    if history && o.len() == 1 {
        fields(o, &["id"])?;
        return Ok(ContentPart::AudioReference(AudioReference::new(
            text(string(o, "id")?, "audio id", 256)?,
            None,
        )?));
    }
    fields(o, &["id", "data", "expires_at", "transcript"])?;
    let reference = AudioReference::new(
        text(string(o, "id")?, "audio id", 256)?,
        Some(
            o.get("expires_at")
                .and_then(Value::as_u64)
                .ok_or(CodecError::Invalid("audio expiry"))?,
        ),
    )?;
    Ok(ContentPart::Audio(GeneratedAudio::new(
        reference,
        text(string(o, "data")?, "audio data", MAX_TEXT_BYTES)?,
        Text::allowing_empty(string(o, "transcript")?, "transcript", MAX_TEXT_BYTES)
            .map_err(|_| CodecError::Limit)?,
    )?))
}
// Fixed official helper concatenates base64 text, not independently padded chunks.
// https://github.com/openai/openai-node/blob/5ee5d86/src/lib/ChatCompletionStream.ts
#[derive(Default)]
pub(super) struct ChunkBytes {
    pending: String,
    padded: bool,
}
impl ChunkBytes {
    fn push(&mut self, fragment: &str) -> Result<Vec<u8>, CodecError> {
        use base64::Engine;
        if self.padded || !fragment.is_ascii() || fragment.len() > MAX_TEXT_BYTES {
            return Err(CodecError::Invalid("audio base64 fragment"));
        }
        self.pending.push_str(fragment);
        let complete = self.pending.len() / 4 * 4;
        let tail = self.pending.split_off(complete);
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(&self.pending)
            .map_err(|_| CodecError::Invalid("audio base64 fragment"))?;
        self.padded = self.pending.contains('=');
        if self.padded && !tail.is_empty() {
            return Err(CodecError::Invalid("audio padding"));
        }
        self.pending = tail;
        Ok(bytes)
    }
}
pub(super) fn read_update(
    value: &Value,
    bytes: &mut ChunkBytes,
) -> Result<AudioUpdate, CodecError> {
    let o = object(value)?;
    fields(o, &["id", "data", "transcript", "expires_at"])?;
    let mut update = AudioUpdate::default();
    if let Some(id) = o.get("id") {
        update.id = Some(text(
            id.as_str().ok_or(CodecError::Invalid("audio id"))?,
            "audio id",
            256,
        )?);
    }
    if let Some(data) = o.get("data") {
        let data = data.as_str().ok_or(CodecError::Invalid("audio data"))?;
        if data.len() > MAX_TEXT_BYTES {
            return Err(CodecError::Limit);
        }
        update.data = Some(bytes.push(data)?);
    }
    if let Some(transcript) = o.get("transcript") {
        update.transcript = Some(
            Text::allowing_empty(
                transcript
                    .as_str()
                    .ok_or(CodecError::Invalid("transcript"))?,
                "transcript",
                MAX_TEXT_BYTES,
            )
            .map_err(|_| CodecError::Limit)?,
        );
    }
    if let Some(expiry) = o.get("expires_at") {
        if o.len() != 1 || !bytes.pending.is_empty() {
            return Err(CodecError::Invalid("audio completion marker"));
        }
        update.expires_at = Some(expiry.as_u64().ok_or(CodecError::Invalid("audio expiry"))?);
    }
    Ok(update)
}
pub(super) fn write_update(update: &AudioUpdate, pending: &mut Vec<u8>) -> Vec<Value> {
    use base64::Engine;
    let mut o = Map::new();
    if let Some(id) = &update.id {
        o.insert("id".into(), json!(id.as_str()));
    }
    if let Some(data) = &update.data {
        pending.extend_from_slice(data);
        let complete = pending.len() / 3 * 3;
        if complete > 0 {
            o.insert(
                "data".into(),
                json!(base64::engine::general_purpose::STANDARD.encode(&pending[..complete])),
            );
            pending.drain(..complete);
        }
    }
    if let Some(text) = &update.transcript {
        o.insert("transcript".into(), json!(text.as_str()));
    }
    let mut values = Vec::new();
    if !o.is_empty() {
        values.push(Value::Object(o));
    }
    if let Some(expiry) = update.expires_at {
        if !pending.is_empty() {
            values
                .push(json!({"data":base64::engine::general_purpose::STANDARD.encode(&*pending)}));
            pending.clear();
        }
        values.push(json!({"expires_at":expiry}));
    }
    values
}
pub(super) fn write(audio: &GeneratedAudio) -> Value {
    json!({"id":audio.reference().id(),"data":audio.data_base64(),"transcript":audio.transcript(),"expires_at":audio.reference().expires_at()})
}
