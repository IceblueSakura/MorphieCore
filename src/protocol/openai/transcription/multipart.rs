//! Bounded standard multipart parsing. No filesystem access, URLs or audio decoding.
use crate::{
    adapter::transcription::Request,
    protocol::CodecError,
    semantic::task::speech_recognition::{
        AudioInput, InputFormat, MAX_INPUT_BYTES, RecognitionRequest,
    },
};
use bytes::Bytes;
use std::collections::BTreeMap;
pub const MAX_UPLOAD_BYTES: usize = MAX_INPUT_BYTES + (16 << 10);
fn invalid() -> CodecError {
    CodecError::Invalid("transcription upload")
}
pub async fn decode_request(content_type: &str, bytes: Bytes) -> Result<Request, CodecError> {
    if bytes.len() > MAX_UPLOAD_BYTES {
        return Err(CodecError::Limit);
    }
    let mime: mime::Mime = content_type.parse().map_err(|_| invalid())?;
    if mime.essence_str() != "multipart/form-data" || mime.params().count() != 1 {
        return Err(invalid());
    }
    let boundary = multer::parse_boundary(content_type).map_err(|_| invalid())?;
    if boundary.is_empty()
        || boundary.len() > 70
        || !boundary
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"'()+_,-./:=?".contains(&b))
    {
        return Err(invalid());
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
        return Err(invalid());
    }
    let constraints = multer::Constraints::new()
        .allowed_fields(vec![
            "model",
            "file",
            "language",
            "response_format",
            "stream",
        ])
        .size_limit(
            multer::SizeLimit::new()
                .whole_stream(MAX_UPLOAD_BYTES as u64)
                .per_field(256)
                .for_field("file", MAX_INPUT_BYTES as u64),
        );
    let stream =
        futures_util::stream::once(std::future::ready(Ok::<_, std::convert::Infallible>(bytes)));
    let mut multipart = multer::Multipart::with_constraints(stream, boundary, constraints);
    let mut fields = BTreeMap::new();
    let mut audio = None;
    while let Some(field) = multipart.next_field().await.map_err(|_| invalid())? {
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
            return Err(invalid());
        }
        let name = field.name().ok_or_else(invalid)?.to_owned();
        if name == "file" {
            if audio.is_some() {
                return Err(invalid());
            }
            let format = input_format(field.file_name(), field.content_type())?;
            let data = field.bytes().await.map_err(|_| invalid())?;
            audio = Some(AudioInput::new(data, format).map_err(|_| invalid())?);
        } else {
            if field.file_name().is_some() || fields.contains_key(&name) {
                return Err(invalid());
            }
            if field.content_type().is_some_and(|m| {
                m.essence_str() != "text/plain"
                    || m.params()
                        .any(|(k, v)| k != mime::CHARSET || v != mime::UTF_8)
            }) {
                return Err(invalid());
            }
            let data = field.bytes().await.map_err(|_| invalid())?;
            let text = std::str::from_utf8(&data)
                .map_err(|_| invalid())?
                .to_owned();
            fields.insert(name, text);
        }
    }
    let model = fields.remove("model").ok_or_else(invalid)?;
    if !super::super::speech::valid_model(&model)
        || fields.get("response_format").is_some_and(|s| s != "json")
        || fields.get("stream").is_some_and(|s| s != "false")
    {
        return Err(invalid());
    }
    let mut task = RecognitionRequest::new(audio.ok_or_else(invalid)?);
    task.language = fields.remove("language");
    task.validate().map_err(|_| invalid())?;
    Ok(Request { model, task })
}
fn input_format(
    filename: Option<&str>,
    media: Option<&mime::Mime>,
) -> Result<InputFormat, CodecError> {
    let declared = match media.map(mime::Mime::essence_str) {
        Some("audio/wav" | "audio/x-wav") => Some(InputFormat::Wav),
        Some("audio/mpeg" | "audio/mp3") => Some(InputFormat::Mp3),
        None | Some("application/octet-stream") => None,
        _ => return Err(invalid()),
    };
    if media.is_some_and(|m| m.params().next().is_some()) {
        return Err(invalid());
    }
    let named = if let Some(name) = filename {
        if name.is_empty()
            || name.len() > 255
            || name
                .chars()
                .any(|c| c.is_control() || matches!(c, '/' | '\\'))
        {
            return Err(invalid());
        }
        match name
            .rsplit_once('.')
            .map(|(_, s)| s.to_ascii_lowercase())
            .as_deref()
        {
            Some("wav") => Some(InputFormat::Wav),
            Some("mp3") => Some(InputFormat::Mp3),
            _ => None,
        }
    } else {
        None
    };
    if declared.is_some() && named.is_some() && declared != named {
        return Err(invalid());
    }
    declared.or(named).ok_or_else(invalid)
}
