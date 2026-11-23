//! Bounded standard multipart parsing. No filesystem access, URLs or audio decoding.
use crate::{
    CodecError,
    semantic::task::speech_recognition::{
        AudioInput, InputFormat, MAX_INPUT_BYTES, MAX_KEYWORD_BYTES, MAX_KEYWORDS,
        MAX_KEYWORDS_TOTAL_BYTES, MAX_LANGUAGES, MAX_PROMPT_BYTES, OutputFormat,
        RecognitionRequest, Temperature,
    },
    transcription::Request,
};
use bytes::Bytes;
use std::collections::BTreeMap;

pub const MAX_UPLOAD_BYTES: usize = MAX_INPUT_BYTES + (64 << 10);

pub async fn decode_request(content_type: &str, bytes: Bytes) -> Result<Request, CodecError> {
    if bytes.len() > MAX_UPLOAD_BYTES {
        return Err(CodecError::Limit);
    }
    let mime: mime::Mime = content_type.parse().map_err(|_| CodecError::invalid())?;
    if mime.essence_str() != "multipart/form-data" || mime.params().count() != 1 {
        return Err(CodecError::invalid());
    }
    let boundary = multer::parse_boundary(content_type).map_err(|_| CodecError::invalid())?;
    if boundary.is_empty()
        || boundary.len() > 70
        || !boundary
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"'()+_,-./:=?".contains(&b))
    {
        return Err(CodecError::invalid());
    }
    // Admit canonical single-envelope uploads; never ignore a second envelope or postamble.
    let closing = format!("\r\n--{boundary}--");
    let end = if bytes.ends_with(b"\r\n") {
        bytes.len() - 2
    } else {
        bytes.len()
    };
    if !bytes.starts_with(format!("--{boundary}\r\n").as_bytes())
        || !bytes[..end].ends_with(closing.as_bytes())
        || bytes
            .windows(closing.len())
            .position(|w| w == closing.as_bytes())
            != Some(end - closing.len())
    {
        return Err(CodecError::invalid());
    }
    let constraints = multer::Constraints::new()
        .allowed_fields(vec![
            "model",
            "file",
            "language",
            "languages[]",
            "keywords[]",
            "prompt",
            "temperature",
            "response_format",
            "stream",
        ])
        .size_limit(
            multer::SizeLimit::new()
                .whole_stream(MAX_UPLOAD_BYTES as u64)
                .per_field(MAX_KEYWORD_BYTES as u64)
                .for_field("prompt", MAX_PROMPT_BYTES as u64)
                .for_field("file", MAX_INPUT_BYTES as u64),
        );
    let stream =
        futures_util::stream::once(std::future::ready(Ok::<_, std::convert::Infallible>(bytes)));
    let mut multipart = multer::Multipart::with_constraints(stream, boundary, constraints);
    let mut fields = BTreeMap::new();
    let mut languages = Vec::new();
    let mut keywords = Vec::new();
    let mut keyword_bytes = 0usize;
    let mut audio = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|_| CodecError::invalid())?
    {
        if field
            .headers()
            .keys()
            .any(|k| !matches!(k.as_str(), "content-disposition" | "content-type"))
            || field
                .headers()
                .get_all("content-disposition")
                .iter()
                .count()
                != 1
            || field.headers().get_all("content-type").iter().count() > 1
        {
            return Err(CodecError::invalid());
        }
        let name = field.name().ok_or_else(CodecError::invalid)?.to_owned();
        if name == "file" {
            if audio.is_some() {
                return Err(CodecError::invalid());
            }
            let format = input_format(field.file_name(), field.content_type())?;
            let data = field.bytes().await.map_err(|_| CodecError::invalid())?;
            audio = Some(AudioInput::new(data, format).map_err(|_| CodecError::invalid())?);
        } else {
            if field.file_name().is_some() {
                return Err(CodecError::invalid());
            }
            if field.content_type().is_some_and(|m| {
                m.essence_str() != "text/plain"
                    || m.params()
                        .any(|(k, v)| k != mime::CHARSET || v != mime::UTF_8)
            }) {
                return Err(CodecError::invalid());
            }
            let data = field.bytes().await.map_err(|_| CodecError::invalid())?;
            let text = std::str::from_utf8(&data)
                .map_err(|_| CodecError::invalid())?
                .to_owned();
            match name.as_str() {
                "languages[]" => {
                    if languages.len() >= MAX_LANGUAGES {
                        return Err(CodecError::invalid());
                    }
                    languages.push(text);
                }
                "keywords[]" => {
                    if keywords.len() >= MAX_KEYWORDS {
                        return Err(CodecError::invalid());
                    }
                    keyword_bytes = keyword_bytes
                        .checked_add(text.len())
                        .filter(|n| *n <= MAX_KEYWORDS_TOTAL_BYTES)
                        .ok_or_else(CodecError::invalid)?;
                    keywords.push(text);
                }
                _ => {
                    if fields.insert(name, text).is_some() {
                        return Err(CodecError::invalid());
                    }
                }
            }
        }
    }
    let model = fields.remove("model").ok_or_else(CodecError::invalid)?;
    if !crate::openai::speech::valid_model(&model)
        || fields.get("stream").is_some_and(|s| s != "false")
    {
        return Err(CodecError::invalid());
    }
    let response_format = match fields.remove("response_format").as_deref() {
        None | Some("json") => OutputFormat::Json,
        Some("text") => OutputFormat::Text,
        Some(_) => return Err(CodecError::invalid()),
    };
    let temperature = fields
        .remove("temperature")
        .map(|raw| {
            if raw.trim() != raw {
                return Err(CodecError::invalid());
            }
            let number: serde_json::Number =
                serde_json::from_str(&raw).map_err(|_| CodecError::invalid())?;
            Temperature::new(number).map_err(|_| CodecError::invalid())
        })
        .transpose()?;
    let mut task = RecognitionRequest::new(audio.ok_or_else(CodecError::invalid)?);
    task.language = fields.remove("language");
    task.languages = languages;
    task.keywords = keywords;
    task.prompt = fields.remove("prompt");
    task.temperature = temperature;
    task.response_format = response_format;
    task.validate().map_err(|_| CodecError::invalid())?;
    Ok(Request { model, task })
}

fn input_format(
    filename: Option<&str>,
    media: Option<&mime::Mime>,
) -> Result<InputFormat, CodecError> {
    if media.is_some_and(|m| m.params().next().is_some()) {
        return Err(CodecError::invalid());
    }
    let declared: Option<&[InputFormat]> = match media.map(mime::Mime::essence_str) {
        Some("audio/wav" | "audio/x-wav") => Some(&[InputFormat::Wav]),
        Some("audio/mpeg") => Some(&[InputFormat::Mp3, InputFormat::Mpeg, InputFormat::Mpga]),
        Some("audio/mp3") => Some(&[InputFormat::Mp3]),
        Some("audio/flac" | "audio/x-flac") => Some(&[InputFormat::Flac]),
        Some("audio/mp4" | "video/mp4") => Some(&[InputFormat::Mp4, InputFormat::M4a]),
        Some("audio/m4a" | "audio/x-m4a") => Some(&[InputFormat::M4a]),
        Some("audio/ogg" | "application/ogg") => Some(&[InputFormat::Ogg]),
        Some("audio/webm" | "video/webm") => Some(&[InputFormat::Webm]),
        None | Some("application/octet-stream") => None,
        _ => return Err(CodecError::invalid()),
    };
    let named = if let Some(name) = filename {
        if name.is_empty()
            || name.len() > 255
            || name
                .chars()
                .any(|c| c.is_control() || matches!(c, '/' | '\\'))
        {
            return Err(CodecError::invalid());
        }
        match name
            .rsplit_once('.')
            .map(|(_, s)| s.to_ascii_lowercase())
            .as_deref()
        {
            Some("wav") => Some(InputFormat::Wav),
            Some("mp3") => Some(InputFormat::Mp3),
            Some("flac") => Some(InputFormat::Flac),
            Some("mp4") => Some(InputFormat::Mp4),
            Some("mpeg") => Some(InputFormat::Mpeg),
            Some("mpga") => Some(InputFormat::Mpga),
            Some("m4a") => Some(InputFormat::M4a),
            Some("ogg") => Some(InputFormat::Ogg),
            Some("webm") => Some(InputFormat::Webm),
            _ => None,
        }
    } else {
        None
    };
    match (declared, named) {
        (Some(allowed), Some(ext)) => {
            if allowed.contains(&ext) {
                Ok(ext)
            } else {
                Err(CodecError::invalid())
            }
        }
        (Some(allowed), None) => Ok(allowed[0]),
        (None, Some(ext)) => Ok(ext),
        (None, None) => Err(CodecError::invalid()),
    }
}
