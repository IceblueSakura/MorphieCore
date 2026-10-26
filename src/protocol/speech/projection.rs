//! Explicit single-target admission; unsupported speech controls are never dropped.
use crate::semantic::{
    task::speech_synthesis::{SpeechError, SpeechRequest, valid_voice},
    value::AudioEncoding,
};
use std::collections::BTreeSet;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SpeechCapabilities {
    pub voices: Vec<String>,
    pub formats: Vec<AudioEncoding>,
    pub instructions: bool,
    pub speed: bool,
}
impl SpeechCapabilities {
    pub fn validate(&self) -> Result<(), SpeechError> {
        if !(1..=64).contains(&self.voices.len())
            || self.voices.iter().any(|v| !valid_voice(v))
            || self.voices.iter().collect::<BTreeSet<_>>().len() != self.voices.len()
            || !(1..=6).contains(&self.formats.len())
            || self.formats.iter().collect::<BTreeSet<_>>().len() != self.formats.len()
        {
            return Err(SpeechError);
        }
        Ok(())
    }
}
pub fn check_request(
    request: &SpeechRequest,
    target: &SpeechCapabilities,
) -> Result<(), SpeechError> {
    request.validate()?;
    target.validate()?;
    if !target.voices.iter().any(|v| v == request.voice())
        || !target.formats.contains(&request.requested_format())
        || request.instructions.is_some() && !target.instructions
        || request.speed.is_some() && !target.speed
    {
        return Err(SpeechError);
    }
    Ok(())
}
/// Ancillary loss is distinct from an absent upstream report.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BinaryOmissions {
    pub reference: bool,
    pub accounting: bool,
    pub alignment: bool,
    pub request_id: bool,
}
pub struct BinaryProjection {
    pub audio: crate::semantic::value::AudioArtifact,
    pub omitted: BinaryOmissions,
}
pub fn project_binary(
    result: &crate::semantic::task::speech_synthesis::SpeechResult,
) -> Result<BinaryProjection, SpeechError> {
    let report = result.report();
    report.validate()?;
    let empty = crate::semantic::task::speech_synthesis::AudioReference::default();
    Ok(BinaryProjection {
        audio: result.audio().clone(),
        omitted: BinaryOmissions {
            reference: report.reference != empty
                || report.sentences.iter().any(|s| s.reference != empty),
            accounting: report.billed_characters.is_some(),
            alignment: !report.sentences.is_empty(),
            request_id: report.request_id.is_some(),
        },
    })
}
