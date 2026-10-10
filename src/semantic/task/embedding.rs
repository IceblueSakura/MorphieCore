//! Bounded ordered text inputs and exact dense vector reports; no wire or runtime target.
use crate::semantic::value::Presence;
use serde_json::Number;

pub const MAX_INPUTS: usize = 128;
pub const MAX_INPUT_BYTES: usize = 256 << 10;
pub const MAX_DIMENSIONS: usize = 8192;
pub const MAX_VECTOR_VALUES: usize = 60_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[error("invalid or over-budget embedding value")]
pub struct EmbeddingError;

#[derive(Clone, Eq, PartialEq)]
pub struct EmbeddingRequest {
    inputs: Vec<String>,
    pub dimensions: Presence<usize>,
}
impl std::fmt::Debug for EmbeddingRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EmbeddingRequest")
            .field("inputs", &"[redacted]")
            .field("dimensions", &self.dimensions)
            .finish()
    }
}
impl EmbeddingRequest {
    pub fn new(inputs: Vec<String>) -> Result<Self, EmbeddingError> {
        let value = Self {
            inputs,
            dimensions: Presence::Absent,
        };
        value.validate()?;
        Ok(value)
    }
    pub fn inputs(&self) -> &[String] {
        &self.inputs
    }
    pub fn validate(&self) -> Result<(), EmbeddingError> {
        let bytes = self.inputs.iter().try_fold(0usize, |total, text| {
            if text.is_empty() {
                return Err(EmbeddingError);
            }
            total.checked_add(text.len()).ok_or(EmbeddingError)
        })?;
        if !(1..=MAX_INPUTS).contains(&self.inputs.len())
            || bytes > MAX_INPUT_BYTES
            || self.dimensions == Presence::Null
            || self
                .dimensions
                .value()
                .is_some_and(|d| !(1..=MAX_DIMENSIONS).contains(d))
        {
            return Err(EmbeddingError);
        }
        Ok(())
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct EmbeddingVector {
    index: usize,
    values: Vec<Number>,
}
impl std::fmt::Debug for EmbeddingVector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EmbeddingVector")
            .field("index", &self.index)
            .field("dimensions", &self.values.len())
            .finish()
    }
}
impl EmbeddingVector {
    pub fn new(index: usize, values: Vec<Number>) -> Result<Self, EmbeddingError> {
        let value = Self { index, values };
        value.validate()?;
        Ok(value)
    }
    pub fn index(&self) -> usize {
        self.index
    }
    pub fn values(&self) -> &[Number] {
        &self.values
    }
    pub fn validate(&self) -> Result<(), EmbeddingError> {
        if self.index >= MAX_INPUTS
            || !(1..=MAX_DIMENSIONS).contains(&self.values.len())
            || self
                .values
                .iter()
                .any(|n| n.to_string().len() > 128 || n.as_f64().is_none_or(|v| !v.is_finite()))
        {
            return Err(EmbeddingError);
        }
        // Floating-point conversion is only a range check; the exact Number stays authoritative.
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum InputTokenReport {
    Explicit(u64),
    InputOnlyTotal,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EmbeddingUsage {
    input: InputTokenReport,
    total_tokens: u64,
}
impl EmbeddingUsage {
    pub fn new(input_tokens: u64, total_tokens: u64) -> Result<Self, EmbeddingError> {
        if total_tokens < input_tokens {
            return Err(EmbeddingError);
        }
        Ok(Self {
            input: InputTokenReport::Explicit(input_tokens),
            total_tokens,
        })
    }
    /// The aggregate explicitly measures input tokens; it is not an unknown prompt report.
    pub fn from_input_only_total(total_tokens: u64) -> Self {
        Self {
            input: InputTokenReport::InputOnlyTotal,
            total_tokens,
        }
    }
    pub fn input_tokens(self) -> u64 {
        match self.input {
            InputTokenReport::Explicit(tokens) => tokens,
            InputTokenReport::InputOnlyTotal => self.total_tokens,
        }
    }
    pub fn reported_input_tokens(self) -> Option<u64> {
        match self.input {
            InputTokenReport::Explicit(tokens) => Some(tokens),
            InputTokenReport::InputOnlyTotal => None,
        }
    }
    pub fn total_tokens(self) -> u64 {
        self.total_tokens
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EmbeddingResponse {
    vectors: Vec<EmbeddingVector>,
    pub usage: Presence<EmbeddingUsage>,
}
impl EmbeddingResponse {
    pub fn new(
        vectors: Vec<EmbeddingVector>,
        usage: EmbeddingUsage,
    ) -> Result<Self, EmbeddingError> {
        let value = Self {
            vectors,
            usage: Presence::Value(usage),
        };
        value.validate()?;
        Ok(value)
    }
    pub fn vectors(&self) -> &[EmbeddingVector] {
        &self.vectors
    }
    pub fn validate(&self) -> Result<(), EmbeddingError> {
        if !(1..=MAX_INPUTS).contains(&self.vectors.len()) {
            return Err(EmbeddingError);
        }
        let dimensions = self.vectors[0].values.len();
        let mut seen = std::collections::BTreeSet::new();
        let mut total = 0usize;
        for vector in &self.vectors {
            vector.validate()?;
            total = total
                .checked_add(vector.values.len())
                .ok_or(EmbeddingError)?;
            if total > MAX_VECTOR_VALUES
                || vector.values.len() != dimensions
                || !seen.insert(vector.index)
            {
                return Err(EmbeddingError);
            }
        }
        Ok(())
    }
    pub fn validate_for(&self, request: &EmbeddingRequest) -> Result<(), EmbeddingError> {
        request.validate()?;
        self.validate()?;
        if self.vectors.len() != request.inputs.len()
            || self.vectors.iter().any(|v| {
                v.index >= request.inputs.len()
                    || request
                        .dimensions
                        .value()
                        .is_some_and(|d| *d != v.values.len())
            })
        {
            return Err(EmbeddingError);
        }
        Ok(())
    }
}
