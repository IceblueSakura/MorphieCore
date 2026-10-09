//! Standard user input audio: inline WAV/MP3 only, never a transcript or a URL.
//! https://developers.openai.com/api/reference/resources/chat/subresources/completions/methods/create
use super::{CodecError, common::*};
use crate::semantic::{task::generation::*, value::Text};
use serde_json::{Map, Value, json};

pub(super) fn read(part: &Map<String, Value>) -> Result<Resource, CodecError> {
    fields(part, &["type", "input_audio"])?;
    let audio = object(
        part.get("input_audio")
            .ok_or(CodecError::Invalid("input_audio"))?,
    )?;
    fields(audio, &["data", "format"])?;
    let mime = match string(audio, "format")? {
        "wav" => "audio/wav",
        "mp3" => "audio/mpeg",
        _ => return Err(CodecError::Unsupported("input audio format".into())),
    };
    let resource = Resource {
        description: ResourceDescription::Audio,
        location: ResourceLocation::Inline {
            media_type: Text::new(mime, "audio media type", 64).map_err(|_| CodecError::Limit)?,
            data_base64: Text::new(string(audio, "data")?, "audio data", MAX_TEXT_BYTES)
                .map_err(|_| CodecError::Invalid("audio data"))?,
        },
    };
    resource.validate()?;
    Ok(resource)
}

pub(crate) fn representable(resource: ResourceView<'_>) -> bool {
    resource.kind() == ResourceKind::Audio
        && matches!(resource.location,
        ResourceLocation::Inline{media_type,..} if matches!(media_type.as_str(),"audio/wav"|"audio/mpeg"))
}

pub(super) fn write(resource: ResourceView<'_>) -> Value {
    let ResourceLocation::Inline {
        media_type,
        data_base64,
    } = resource.location
    else {
        unreachable!("lowering rejects non-inline input audio");
    };
    let format = match media_type.as_str() {
        "audio/wav" => "wav",
        "audio/mpeg" => "mp3",
        _ => unreachable!("lowering checks input audio formats"),
    };
    json!({"type":"input_audio","input_audio":{"data":data_base64.as_str(),"format":format}})
}
