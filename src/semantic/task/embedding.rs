//! Independent samples, exact dense values and source accounting. No wire or runtime target.
use serde_json::Number;

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[error("invalid embedding value")]
pub struct EmbeddingError;

/// A logical vocabulary contract, not a model alias or a tokenizer implementation.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TokenizerScope {
    Cl100kBase,
}
impl TokenizerScope {
    /// Only ordinary mergeable tokens; special/control tokens are not admitted.
    pub fn accepts(self, id: u32) -> bool {
        match self {
            Self::Cl100kBase => id < 100_256,
        }
    }
}

#[derive(Clone, Eq, PartialEq)]
pub enum EmbeddingInput {
    /// Ordered parts jointly embedded as one sample; never separate batch entries.
    TextParts(Vec<String>),
    TokenIds {
        tokenizer: TokenizerScope,
        ids: Vec<u32>,
    },
}
impl std::fmt::Debug for EmbeddingInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::TextParts(_) => "TextParts([redacted])",
            Self::TokenIds { .. } => "TokenIds([redacted])",
        })
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EmbeddingSample {
    input: EmbeddingInput,
    dimensions: Option<usize>,
}
impl EmbeddingSample {
    pub fn new(input: EmbeddingInput, dimensions: Option<usize>) -> Result<Self, EmbeddingError> {
        let valid = match &input {
            EmbeddingInput::TextParts(parts) => {
                !parts.is_empty() && parts.iter().all(|p| !p.is_empty())
            }
            EmbeddingInput::TokenIds { tokenizer, ids } => {
                !ids.is_empty() && ids.iter().all(|id| tokenizer.accepts(*id))
            }
        };
        if !valid || dimensions == Some(0) {
            return Err(EmbeddingError);
        }
        Ok(Self { input, dimensions })
    }
    pub fn input(&self) -> &EmbeddingInput {
        &self.input
    }
    pub fn dimensions(&self) -> Option<usize> {
        self.dimensions
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EmbeddingRequest {
    samples: Vec<EmbeddingSample>,
}
impl EmbeddingRequest {
    pub fn new(samples: Vec<EmbeddingSample>) -> Result<Self, EmbeddingError> {
        if samples.is_empty() {
            return Err(EmbeddingError);
        }
        Ok(Self { samples })
    }
    pub fn text(inputs: Vec<String>) -> Result<Self, EmbeddingError> {
        Self::new(
            inputs
                .into_iter()
                .map(|text| EmbeddingSample::new(EmbeddingInput::TextParts(vec![text]), None))
                .collect::<Result<_, _>>()?,
        )
    }
    pub fn samples(&self) -> &[EmbeddingSample] {
        &self.samples
    }
    pub fn with_dimensions(&self, dimensions: Option<usize>) -> Result<Self, EmbeddingError> {
        Self::new(
            self.samples
                .iter()
                .map(|s| EmbeddingSample::new(s.input.clone(), dimensions))
                .collect::<Result<_, _>>()?,
        )
    }
}

#[derive(Clone, Eq, PartialEq)]
pub enum EmbeddingValues {
    ExactJson(Vec<Number>),
    Binary32(Vec<u32>),
}
impl EmbeddingValues {
    pub fn len(&self) -> usize {
        match self {
            Self::ExactJson(v) => v.len(),
            Self::Binary32(v) => v.len(),
        }
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
impl std::fmt::Debug for EmbeddingValues {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EmbeddingValues")
            .field("dimensions", &self.len())
            .finish()
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EmbeddingVector {
    index: usize,
    values: EmbeddingValues,
}
impl EmbeddingVector {
    pub fn new(index: usize, values: Vec<Number>) -> Result<Self, EmbeddingError> {
        Self::from_values(index, EmbeddingValues::ExactJson(values))
    }
    pub fn from_binary32(index: usize, bits: Vec<u32>) -> Result<Self, EmbeddingError> {
        Self::from_values(index, EmbeddingValues::Binary32(bits))
    }
    fn from_values(index: usize, values: EmbeddingValues) -> Result<Self, EmbeddingError> {
        if values.is_empty()
            || match &values {
                EmbeddingValues::ExactJson(values) => values
                    .iter()
                    .any(|n| n.as_str().len() > 128 || n.as_f64().is_none_or(|v| !v.is_finite())),
                EmbeddingValues::Binary32(bits) => {
                    bits.iter().any(|v| !f32::from_bits(*v).is_finite())
                }
            }
        {
            return Err(EmbeddingError);
        }
        Ok(Self { index, values })
    }
    pub fn index(&self) -> usize {
        self.index
    }
    pub fn values(&self) -> &EmbeddingValues {
        &self.values
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AccountingBasis {
    InputOnly,
    Unspecified,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InputModality {
    Unspecified,
    Text,
    Image,
    Audio,
    Video,
    Document,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModalityTokenCount {
    pub modality: InputModality,
    pub tokens: u64,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EmbeddingUsage {
    input_tokens: Option<u64>,
    total_tokens: Option<u64>,
    basis: AccountingBasis,
    details: Option<Vec<ModalityTokenCount>>,
}
impl EmbeddingUsage {
    pub fn reported(
        input: Option<u64>,
        total: Option<u64>,
        basis: AccountingBasis,
        details: Option<Vec<ModalityTokenCount>>,
    ) -> Result<Self, EmbeddingError> {
        if matches!((input,total), (Some(i),Some(t)) if t < i) {
            return Err(EmbeddingError);
        }
        Ok(Self {
            input_tokens: input,
            total_tokens: total,
            basis,
            details,
        })
    }
    pub fn new(input: u64, total: u64) -> Result<Self, EmbeddingError> {
        Self::reported(Some(input), Some(total), AccountingBasis::Unspecified, None)
    }
    pub fn from_input_only_total(total: u64) -> Self {
        Self {
            input_tokens: None,
            total_tokens: Some(total),
            basis: AccountingBasis::InputOnly,
            details: None,
        }
    }
    pub fn reported_input_tokens(&self) -> Option<u64> {
        self.input_tokens
    }
    pub fn reported_total_tokens(&self) -> Option<u64> {
        self.total_tokens
    }
    pub fn basis(&self) -> AccountingBasis {
        self.basis
    }
    pub fn details(&self) -> Option<&[ModalityTokenCount]> {
        self.details.as_deref()
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EmbeddingResponse {
    vectors: Vec<EmbeddingVector>,
    usage: Option<EmbeddingUsage>,
}
impl EmbeddingResponse {
    pub fn new(
        vectors: Vec<EmbeddingVector>,
        usage: Option<EmbeddingUsage>,
    ) -> Result<Self, EmbeddingError> {
        if vectors.is_empty() {
            return Err(EmbeddingError);
        }
        let mut seen = std::collections::BTreeSet::new();
        for vector in &vectors {
            if vector.index >= vectors.len() || !seen.insert(vector.index) {
                return Err(EmbeddingError);
            }
        }
        Ok(Self { vectors, usage })
    }
    pub fn vectors(&self) -> &[EmbeddingVector] {
        &self.vectors
    }
    pub fn usage(&self) -> Option<&EmbeddingUsage> {
        self.usage.as_ref()
    }
    pub fn validate_for(&self, request: &EmbeddingRequest) -> Result<(), EmbeddingError> {
        if self.vectors.len() != request.samples.len()
            || self.vectors.iter().any(|v| {
                request.samples[v.index]
                    .dimensions
                    .is_some_and(|d| d != v.values.len())
            })
        {
            return Err(EmbeddingError);
        }
        Ok(())
    }
}
