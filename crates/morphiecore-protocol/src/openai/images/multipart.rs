//! Bounded OpenAI Images Edit multipart intake and encoding.
//! No filesystem access, URL fetching, or pixel/alpha decoding.
use crate::{
    CodecError,
    image::Request,
    multipart::{Part, encode},
    semantic::{
        context::{ClientIdentityHints, DeliveryIntent},
        task::image_generation::{
            ImageEdit, ImageGenerationRequest, ImageInput, ImageMask, InputFidelity,
            MAX_EDIT_INPUT_BYTES, MAX_EDIT_MASK_BYTES_EXCLUSIVE, MAX_EDIT_SOURCES,
            MAX_PROMPT_BYTES,
        },
        value::{ImageFormat, Presence},
    },
};
use bytes::Bytes;
use std::collections::BTreeMap;

pub const MAX_EDIT_UPLOAD_BYTES: usize = MAX_EDIT_INPUT_BYTES + (256 << 10);

fn single_envelope(content_type: &str, bytes: &[u8]) -> Result<String, CodecError> {
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
    Ok(boundary)
}

fn image_format(
    filename: Option<&str>,
    media: Option<&mime::Mime>,
) -> Result<ImageFormat, CodecError> {
    if media.is_some_and(|m| m.params().next().is_some()) {
        return Err(CodecError::invalid());
    }
    let declared = match media.map(mime::Mime::essence_str) {
        Some("image/png") => Some(ImageFormat::Png),
        Some("image/jpeg") => Some(ImageFormat::Jpeg),
        Some("image/webp") => Some(ImageFormat::Webp),
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
            .map(|(_, ext)| ext.to_ascii_lowercase())
            .as_deref()
        {
            Some("png") => Some(ImageFormat::Png),
            Some("jpg" | "jpeg") => Some(ImageFormat::Jpeg),
            Some("webp") => Some(ImageFormat::Webp),
            _ => None,
        }
    } else {
        None
    };
    match (declared, named) {
        (Some(d), Some(n)) => {
            if d == n {
                Ok(d)
            } else {
                Err(CodecError::invalid())
            }
        }
        (Some(d), None) => Ok(d),
        (None, Some(n)) => Ok(n),
        (None, None) => Err(CodecError::invalid()),
    }
}

fn map_multer_error(err: multer::Error) -> CodecError {
    match err {
        multer::Error::StreamSizeExceeded { .. } => CodecError::Limit,
        multer::Error::FieldSizeExceeded { field_name, .. }
            if field_name.as_deref() != Some("mask") =>
        {
            CodecError::Limit
        }
        _ => CodecError::invalid(),
    }
}

fn parse_u8(raw: &str) -> Result<u8, CodecError> {
    if raw.is_empty() || raw.trim() != raw || !raw.bytes().all(|b| b.is_ascii_digit()) {
        return Err(CodecError::invalid());
    }
    raw.parse().map_err(|_| CodecError::invalid())
}

pub async fn decode_edit_request(content_type: &str, bytes: Bytes) -> Result<Request, CodecError> {
    if bytes.len() > MAX_EDIT_UPLOAD_BYTES {
        return Err(CodecError::Limit);
    }
    let boundary = single_envelope(content_type, &bytes)?;
    let constraints = multer::Constraints::new()
        .allowed_fields(vec![
            "image",
            "image[]",
            "mask",
            "model",
            "prompt",
            "n",
            "size",
            "quality",
            "background",
            "output_format",
            "output_compression",
            "input_fidelity",
            "stream",
            "user",
            "safety_identifier",
        ])
        .size_limit(
            multer::SizeLimit::new()
                .whole_stream(MAX_EDIT_UPLOAD_BYTES as u64)
                .per_field(2048)
                .for_field("prompt", MAX_PROMPT_BYTES as u64)
                .for_field("image", MAX_EDIT_INPUT_BYTES as u64)
                .for_field("image[]", MAX_EDIT_INPUT_BYTES as u64)
                .for_field("mask", MAX_EDIT_MASK_BYTES_EXCLUSIVE as u64),
        );
    let stream =
        futures_util::stream::once(std::future::ready(Ok::<_, std::convert::Infallible>(bytes)));
    let mut multipart = multer::Multipart::with_constraints(stream, boundary, constraints);

    let mut fields = BTreeMap::new();
    let mut sources: Vec<ImageInput> = Vec::new();
    let mut saw_singular_image = false;
    let mut saw_array_image = false;
    let mut mask: Option<ImageMask> = None;
    let mut total_input_bytes = 0usize;

    while let Some(field) = multipart.next_field().await.map_err(map_multer_error)? {
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
        if matches!(name.as_str(), "image" | "image[]" | "mask") {
            let format = image_format(field.file_name(), field.content_type())?;
            let data = field.bytes().await.map_err(map_multer_error)?;
            total_input_bytes = total_input_bytes
                .checked_add(data.len())
                .ok_or(CodecError::Limit)?;
            if total_input_bytes > MAX_EDIT_INPUT_BYTES {
                return Err(CodecError::Limit);
            }
            let input = ImageInput::new(data, format).map_err(|_| CodecError::invalid())?;
            match name.as_str() {
                "image" => {
                    if saw_singular_image || saw_array_image {
                        return Err(CodecError::invalid());
                    }
                    saw_singular_image = true;
                    sources.push(input);
                }
                "image[]" => {
                    if saw_singular_image || sources.len() >= MAX_EDIT_SOURCES {
                        return Err(CodecError::invalid());
                    }
                    saw_array_image = true;
                    sources.push(input);
                }
                "mask" => {
                    if mask.is_some()
                        || input.format() != ImageFormat::Png
                        || input.data().len() >= MAX_EDIT_MASK_BYTES_EXCLUSIVE
                    {
                        return Err(CodecError::invalid());
                    }
                    mask = Some(ImageMask {
                        source_index: 0,
                        input,
                    });
                }
                _ => unreachable!(),
            }
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
            let data = field.bytes().await.map_err(map_multer_error)?;
            let text = std::str::from_utf8(&data)
                .map_err(|_| CodecError::invalid())?
                .to_owned();
            if fields.insert(name, text).is_some() {
                return Err(CodecError::invalid());
            }
        }
    }

    let model = fields.remove("model").ok_or_else(CodecError::invalid)?;
    let prompt = fields.remove("prompt").ok_or_else(CodecError::invalid)?;
    let stream = match fields.remove("stream").as_deref() {
        None => None,
        Some("false") => Some(false),
        Some(_) => return Err(CodecError::invalid()),
    };

    let mut task =
        ImageGenerationRequest::new_edit(prompt, sources).map_err(|_| CodecError::invalid())?;
    if let Some(mask) = mask {
        let edit = task.edit_mut().expect("edit request");
        mask.validate(&edit.sources)
            .map_err(|_| CodecError::invalid())?;
        edit.mask = Some(mask);
    }
    if let Some(raw) = fields.remove("input_fidelity") {
        let fidelity: InputFidelity = raw.parse().map_err(|_| CodecError::invalid())?;
        task.edit_mut().expect("edit request").input_fidelity = Presence::Value(fidelity);
    }
    if let Some(raw) = fields.remove("n") {
        task.count = Presence::Value(parse_u8(&raw)?);
    }
    if let Some(raw) = fields.remove("output_format") {
        task.format = Presence::Value(super::format_str(&raw)?);
    }
    if let Some(raw) = fields.remove("size") {
        task.size = Presence::Value(super::size_request_str(&raw)?);
    }
    if let Some(raw) = fields.remove("quality") {
        task.quality = Presence::Value(super::quality_request_str(&raw)?);
    }
    if let Some(raw) = fields.remove("background") {
        task.background = Presence::Value(super::background_request_str(&raw)?);
    }
    if let Some(raw) = fields.remove("output_compression") {
        task.compression = Presence::Value(parse_u8(&raw)?);
    }

    let request = Request {
        model,
        task,
        delivery: DeliveryIntent {
            stream,
            options: None,
        },
        identity: ClientIdentityHints {
            user: fields.remove("user"),
            safety_identifier: fields.remove("safety_identifier"),
        },
    };
    super::validate_request(&request)?;
    Ok(request)
}

pub fn encode_edit_request(
    request: &Request,
    model: &str,
    limit: usize,
) -> Result<crate::multipart::EncodedMultipart, CodecError> {
    super::validate_request(request)?;
    if !super::valid_model(model) {
        return Err(CodecError::invalid());
    }
    let Some(edit): Option<&ImageEdit> = request.task.edit() else {
        return Err(CodecError::invalid());
    };

    let count = request.task.count.value().map(|n| n.to_string());
    let compression = request.task.compression.value().map(|c| c.to_string());
    let size = request.task.size.value().map(|s| match s {
        crate::semantic::task::image_generation::ImageSizeRequest::Auto => "auto".to_owned(),
        crate::semantic::task::image_generation::ImageSizeRequest::Exact(dims) => {
            format!("{}x{}", dims.width, dims.height)
        }
    });

    let mut parts = Vec::with_capacity(edit.sources.len() + 14);
    parts.push(Part::Text {
        name: "model",
        value: model,
    });
    parts.push(Part::Text {
        name: "prompt",
        value: request.task.prompt(),
    });
    if let Some(ref count) = count {
        parts.push(Part::Text {
            name: "n",
            value: count,
        });
    }
    if let Some(stream) = request.delivery.stream {
        parts.push(Part::Text {
            name: "stream",
            value: if stream { "true" } else { "false" },
        });
    }
    if let Some(format) = request.task.format.value() {
        parts.push(Part::Text {
            name: "output_format",
            value: super::format_label(format),
        });
    }
    if let Some(ref size) = size {
        parts.push(Part::Text {
            name: "size",
            value: size,
        });
    }
    if let Some(quality) = request.task.quality.value() {
        parts.push(Part::Text {
            name: "quality",
            value: quality.label(),
        });
    }
    if let Some(background) = request.task.background.value() {
        parts.push(Part::Text {
            name: "background",
            value: background.label(),
        });
    }
    if let Some(ref compression) = compression {
        parts.push(Part::Text {
            name: "output_compression",
            value: compression,
        });
    }
    if let Some(fidelity) = edit.input_fidelity.value() {
        let value: &str = (*fidelity).into();
        parts.push(Part::Text {
            name: "input_fidelity",
            value,
        });
    }
    if let Some(user) = &request.identity.user {
        parts.push(Part::Text {
            name: "user",
            value: user,
        });
    }
    if let Some(safety_identifier) = &request.identity.safety_identifier {
        parts.push(Part::Text {
            name: "safety_identifier",
            value: safety_identifier,
        });
    }

    let multiple = edit.sources.len() > 1;
    let source_name = if multiple { "image[]" } else { "image" };
    let filenames: Vec<String> = edit
        .sources
        .iter()
        .enumerate()
        .map(|(i, src)| format!("image-{i}.{}", src.extension()))
        .collect();
    for (src, filename) in edit.sources.iter().zip(filenames.iter()) {
        parts.push(Part::File {
            name: source_name,
            filename,
            content_type: src.mime(),
            data: src.data(),
        });
    }
    if let Some(mask) = &edit.mask {
        if mask.source_index != 0 {
            return Err(CodecError::invalid());
        }
        parts.push(Part::File {
            name: "mask",
            filename: "mask.png",
            content_type: "image/png",
            data: mask.input.data(),
        });
    }
    encode(&parts, limit)
}
