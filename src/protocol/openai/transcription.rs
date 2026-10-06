//! Standard synchronous transcription JSON, with an explicit ancillary-report projection.
//! Source: https://developers.openai.com/api/reference/resources/audio/subresources/transcriptions/methods/create
use crate::{protocol::CodecError, semantic::task::speech_recognition::RecognitionResult};
use serde_json::{Value, json};
mod multipart;
pub use multipart::{MAX_UPLOAD_BYTES, decode_request};
pub struct Projection {
    pub body: Value,
    pub omitted_alignment: bool,
    pub omitted_request_id: bool,
}
pub fn project_result(result: &RecognitionResult) -> Result<Projection, CodecError> {
    result
        .report()
        .validate(result.text())
        .map_err(|_| CodecError::Invalid("transcription report"))?;
    let mut body = json!({"text":result.text()});
    if let Some(seconds) = result.report().processed_seconds {
        body["usage"] = json!({"type":"duration","seconds":seconds});
    }
    Ok(Projection {
        body,
        omitted_alignment: result.report().last_sentence.is_some(),
        omitted_request_id: result.report().request_id.is_some(),
    })
}
