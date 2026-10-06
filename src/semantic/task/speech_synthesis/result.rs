//! Bounded synthesis reports. Replacement invalidates every dependent report.
use super::SpeechError;
use crate::semantic::value::AudioArtifact;

#[derive(Clone, Default, Eq, PartialEq)]
pub struct AudioReference {
    pub id: Option<String>,
    pub url: Option<String>,
    pub expires_at: Option<u64>,
}
impl AudioReference {
    pub fn validate(&self) -> Result<(), SpeechError> {
        if self
            .id
            .as_ref()
            .is_some_and(|s| s.is_empty() || s.len() > 256 || s.chars().any(char::is_control))
        {
            return Err(SpeechError);
        }
        if let Some(raw) = &self.url {
            if raw.len() > 8192 {
                return Err(SpeechError);
            }
            let url = url::Url::parse(raw).map_err(|_| SpeechError)?;
            if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
                return Err(SpeechError);
            }
        }
        Ok(())
    }
    fn size(&self) -> usize {
        self.id.as_ref().map_or(0, String::len) + self.url.as_ref().map_or(0, String::len)
    }
}
#[derive(Clone, Eq, PartialEq)]
pub struct SpeechWord {
    pub text: String,
    /// Reported sentence-relative character positions; end is exclusive.
    pub start_character: u64,
    pub end_character: u64,
    pub start_ms: u64,
    pub end_ms: u64,
}
#[derive(Clone, Eq, PartialEq)]
pub struct SpeechSentence {
    pub index: u64,
    /// Reported segmented input, not a fabricated transcript of the audio.
    pub text: Option<String>,
    pub words: Vec<SpeechWord>,
    pub reference: AudioReference,
}
#[derive(Clone, Default, Eq, PartialEq)]
pub struct SpeechReport {
    pub request_id: Option<String>,
    pub reference: AudioReference,
    /// Latest reported cumulative billable characters; snapshots are not added.
    pub billed_characters: Option<u64>,
    /// A synthesis stop does not promote an earlier usage snapshot to a final report.
    pub billed_characters_final: bool,
    pub sentences: Vec<SpeechSentence>,
}
impl SpeechReport {
    pub fn validate(&self) -> Result<(), SpeechError> {
        self.reference.validate()?;
        if self.billed_characters_final && self.billed_characters.is_none() {
            return Err(SpeechError);
        }
        if self
            .request_id
            .as_ref()
            .is_some_and(|s| s.is_empty() || s.len() > 256 || s.chars().any(char::is_control))
            || self.sentences.len() > 4096
        {
            return Err(SpeechError);
        }
        let mut size = self.reference.size() + self.request_id.as_ref().map_or(0, String::len);
        let mut words = 0usize;
        for (index, sentence) in self.sentences.iter().enumerate() {
            if sentence.index != index as u64 {
                return Err(SpeechError);
            }
            sentence.reference.validate()?;
            size = size
                .saturating_add(sentence.reference.size())
                .saturating_add(sentence.text.as_ref().map_or(0, String::len));
            words = words.saturating_add(sentence.words.len());
            if words > 65536 {
                return Err(SpeechError);
            }
            for word in &sentence.words {
                size = size.saturating_add(word.text.len()).saturating_add(32);
                if word.start_character >= word.end_character
                    || word.start_ms > word.end_ms
                    || word.text.is_empty()
                {
                    return Err(SpeechError);
                }
            }
            if size > 1 << 20 {
                return Err(SpeechError);
            }
        }
        Ok(())
    }
}
#[derive(Clone, Eq, PartialEq)]
pub struct SpeechResult {
    audio: AudioArtifact,
    report: SpeechReport,
}
impl std::fmt::Debug for SpeechResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SpeechResult([redacted])")
    }
}
impl SpeechResult {
    pub fn new(audio: AudioArtifact, report: SpeechReport) -> Result<Self, SpeechError> {
        report.validate()?;
        Ok(Self { audio, report })
    }
    pub fn audio(&self) -> &AudioArtifact {
        &self.audio
    }
    pub fn report(&self) -> &SpeechReport {
        &self.report
    }
    pub fn replace_audio(&mut self, audio: AudioArtifact) {
        self.audio = audio;
        self.report = SpeechReport::default();
    }
}
