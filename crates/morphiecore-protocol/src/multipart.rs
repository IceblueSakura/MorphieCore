//! Pure, bounded in-memory `multipart/form-data` encoder for upstream media uploads.
use crate::CodecError;

pub enum Part<'a> {
    Text {
        name: &'a str,
        value: &'a str,
    },
    File {
        name: &'a str,
        filename: &'a str,
        content_type: &'a str,
        data: &'a [u8],
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EncodedMultipart {
    pub content_type: String,
    pub body: Vec<u8>,
}

fn valid_header_token(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 255
        && !value
            .bytes()
            .any(|b| b < 0x20 || b == 0x7f || matches!(b, b'"' | b'\\' | b';'))
}

fn contains_subslice(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty() && haystack.windows(needle.len()).any(|w| w == needle)
}

fn pick_boundary(parts: &[Part<'_>]) -> String {
    let mut index = 0u64;
    loop {
        let boundary = format!("morphiecore-part-{index:016x}");
        let marker = format!("--{boundary}");
        let needle = marker.as_bytes();
        let collides = parts.iter().any(|part| match part {
            Part::Text { value, .. } => contains_subslice(value.as_bytes(), needle),
            Part::File { data, .. } => contains_subslice(data, needle),
        });
        if !collides {
            return boundary;
        }
        index += 1;
    }
}

pub fn encode(parts: &[Part<'_>], limit: usize) -> Result<EncodedMultipart, CodecError> {
    if parts.is_empty() {
        return Err(CodecError::invalid());
    }
    for part in parts {
        match part {
            Part::Text { name, .. } => {
                if !valid_header_token(name) {
                    return Err(CodecError::invalid());
                }
            }
            Part::File {
                name,
                filename,
                content_type,
                ..
            } => {
                if !valid_header_token(name)
                    || !valid_header_token(filename)
                    || !valid_header_token(content_type)
                    || filename.chars().any(|c| matches!(c, '/' | '\\'))
                {
                    return Err(CodecError::invalid());
                }
            }
        }
    }

    let boundary = pick_boundary(parts);
    let mut total = 0usize;
    let mut headers = Vec::with_capacity(parts.len());
    for part in parts {
        let header = match part {
            Part::Text { name, .. } => {
                format!("--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n")
            }
            Part::File {
                name,
                filename,
                content_type,
                ..
            } => format!(
                "--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"; filename=\"{filename}\"\r\nContent-Type: {content_type}\r\n\r\n"
            ),
        };
        let payload_len = match part {
            Part::Text { value, .. } => value.len(),
            Part::File { data, .. } => data.len(),
        };
        total = total
            .checked_add(header.len())
            .and_then(|n| n.checked_add(payload_len))
            .and_then(|n| n.checked_add(2))
            .ok_or(CodecError::Limit)?;
        headers.push(header);
    }
    let closing = format!("--{boundary}--\r\n");
    total = total.checked_add(closing.len()).ok_or(CodecError::Limit)?;
    if total > limit {
        return Err(CodecError::Limit);
    }

    let mut body = Vec::with_capacity(total);
    for (header, part) in headers.into_iter().zip(parts) {
        body.extend_from_slice(header.as_bytes());
        match part {
            Part::Text { value, .. } => body.extend_from_slice(value.as_bytes()),
            Part::File { data, .. } => body.extend_from_slice(data),
        }
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(closing.as_bytes());
    Ok(EncodedMultipart {
        content_type: format!("multipart/form-data; boundary={boundary}"),
        body,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_parts_avoids_boundary_collision_and_enforces_safety_limits() {
        let colliding = b"payload --morphiecore-part-0000000000000000 tail";
        let encoded = encode(
            &[
                Part::Text {
                    name: "model",
                    value: "gpt-transcribe",
                },
                Part::Text {
                    name: "keywords[]",
                    value: "alpha",
                },
                Part::Text {
                    name: "keywords[]",
                    value: "beta",
                },
                Part::File {
                    name: "file",
                    filename: "clip.flac",
                    content_type: "audio/flac",
                    data: colliding,
                },
            ],
            4096,
        )
        .unwrap();
        assert_eq!(
            encoded.content_type,
            "multipart/form-data; boundary=morphiecore-part-0000000000000001"
        );
        let text = String::from_utf8_lossy(&encoded.body);
        assert!(text.contains("name=\"keywords[]\"\r\n\r\nalpha\r\n"));
        assert!(text.contains("name=\"keywords[]\"\r\n\r\nbeta\r\n"));
        assert!(text.contains(
            "name=\"file\"; filename=\"clip.flac\"\r\nContent-Type: audio/flac\r\n\r\npayload "
        ));
        assert!(matches!(
            encode(
                &[Part::Text {
                    name: "model",
                    value: "gpt-transcribe"
                }],
                10,
            ),
            Err(CodecError::Limit)
        ));
        assert!(
            encode(
                &[Part::File {
                    name: "file",
                    filename: "evil\"\r\nX: y",
                    content_type: "audio/wav",
                    data: b"abc",
                }],
                4096,
            )
            .is_err()
        );
    }
}
