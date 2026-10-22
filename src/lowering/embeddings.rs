//! Pure request expectations and delivery projection; no registry, credentials or I/O.
use crate::{
    adapter::embeddings::{Encoding, Request, Response},
    semantic::{task::embedding::*, value::Presence},
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Contract {
    pub max_inputs: usize,
    pub default_dimensions: usize,
    pub dimensions: DimensionSupport,
    pub encodings: EncodingSupport,
    pub user: bool,
}
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DimensionSupport {
    UpTo(usize),
    Range { min: usize, max: usize },
    Selected(Vec<usize>),
    Unsupported,
}
impl DimensionSupport {
    fn admits(&self, d: usize) -> bool {
        match self {
            Self::UpTo(max) => (1..=*max).contains(&d),
            Self::Range { min, max } => (*min..=*max).contains(&d),
            Self::Selected(values) => values.contains(&d),
            Self::Unsupported => false,
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EncodingSupport {
    Float,
    Base64,
    Both,
}
impl EncodingSupport {
    fn select(self, client: Encoding) -> Encoding {
        match self {
            Self::Float => Encoding::Float,
            Self::Base64 => Encoding::Base64,
            Self::Both => client,
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Expectation {
    pub inputs: usize,
    pub dimensions: usize,
    pub upstream_encoding: Encoding,
    pub client_encoding: Encoding,
}
impl Expectation {
    pub fn validate_response(&self, response: &Response) -> Result<(), EmbeddingError> {
        response.validate()?;
        if response.vectors().len() != self.inputs
            || response
                .vectors()
                .iter()
                .any(|v| v.index() >= self.inputs || v.values().len() != self.dimensions)
        {
            return Err(EmbeddingError);
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Prepared {
    pub target: Request,
    pub expectation: Expectation,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
pub enum Rule {
    #[serde(rename = "embedding.float-json-to-binary32.v1")]
    RoundBinary32,
    #[serde(rename = "embedding.omit-response-id.v1")]
    OmitResponseId,
}
impl Rule {
    pub fn name(self) -> &'static str {
        match self {
            Self::RoundBinary32 => "embedding.float-json-to-binary32.v1",
            Self::OmitResponseId => "embedding.omit-response-id.v1",
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Delivery,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
pub struct Receipt {
    pub rule: Rule,
    pub direction: Direction,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Projection {
    pub response: Response,
    pub receipts: Vec<Receipt>,
}
/// Convert only a delivery copy; reported decimal values and identities stay unchanged.
pub fn project_response(
    source: &Response,
    encoding: Encoding,
) -> Result<Projection, EmbeddingError> {
    source.validate()?;
    let mut receipts = vec![];
    if source.id != Presence::Absent {
        receipts.push(Receipt {
            rule: Rule::OmitResponseId,
            direction: Direction::Delivery,
        });
    }
    let mut response = source.clone();
    response.id = Presence::Absent;
    if encoding == Encoding::Base64 {
        let mut rounded = false;
        let vectors = source
            .vectors()
            .iter()
            .map(|vector| {
                match vector.values() {
                    EmbeddingValues::Binary32(_) => Ok(vector.clone()),
                    EmbeddingValues::ExactJson(values) => {
                        rounded = true;
                        let bits = values
                            .iter()
                            .map(|number| {
                                let decimal = number.as_str();
                                // Parse directly to binary32: parsing via f64 can double-round midpoints.
                                let value = decimal.parse::<f32>().map_err(|_| EmbeddingError)?;
                                let nonzero = decimal
                                    .split(['e', 'E'])
                                    .next()
                                    .unwrap_or("")
                                    .bytes()
                                    .any(|b| matches!(b, b'1'..=b'9'));
                                if !value.is_finite() || value == 0.0 && nonzero {
                                    return Err(EmbeddingError);
                                }
                                Ok(value.to_bits())
                            })
                            .collect::<Result<_, _>>()?;
                        EmbeddingVector::from_binary32(vector.index(), bits)
                    }
                }
            })
            .collect::<Result<Vec<_>, _>>()?;
        response.task = source.task.with_vectors(vectors)?;
        if rounded {
            receipts.push(Receipt {
                rule: Rule::RoundBinary32,
                direction: Direction::Delivery,
            });
        }
    }
    response.validate()?;
    Ok(Projection { response, receipts })
}
impl Contract {
    pub fn validate(&self) -> Result<(), EmbeddingError> {
        if !(1..=MAX_INPUTS).contains(&self.max_inputs)
            || !(1..=MAX_DIMENSIONS).contains(&self.default_dimensions)
        {
            return Err(EmbeddingError);
        }
        match &self.dimensions {
            DimensionSupport::UpTo(max) if (1..=MAX_DIMENSIONS).contains(max) => Ok(()),
            DimensionSupport::Range { min, max }
                if *min >= 1 && min <= max && *max <= MAX_DIMENSIONS =>
            {
                Ok(())
            }
            DimensionSupport::Unsupported => Ok(()),
            DimensionSupport::Selected(values)
                if !values.is_empty()
                    && values.len() <= MAX_DIMENSIONS
                    && values.iter().all(|d| (1..=MAX_DIMENSIONS).contains(d))
                    && values
                        .iter()
                        .collect::<std::collections::BTreeSet<_>>()
                        .len()
                        == values.len() =>
            {
                Ok(())
            }
            _ => Err(EmbeddingError),
        }
    }
    pub fn admit(&self, request: &Request) -> Result<(), EmbeddingError> {
        self.prepare(request).map(|_| ())
    }
    pub fn prepare(&self, request: &Request) -> Result<Prepared, EmbeddingError> {
        self.validate()?;
        request.task.validate()?;
        let inputs = request.task.inputs().len();
        let dimensions = request
            .task
            .dimensions
            .value()
            .copied()
            .unwrap_or(self.default_dimensions);
        if inputs > self.max_inputs
            || request.encoding == Presence::Null
            || dimensions
                .checked_mul(inputs)
                .is_none_or(|values| values > MAX_VECTOR_VALUES)
            || request
                .task
                .dimensions
                .value()
                .is_some_and(|d| !self.dimensions.admits(*d))
            || !self.user && request.identity.user != Presence::Absent
        {
            return Err(EmbeddingError);
        }
        let client_encoding = request.encoding.value().copied().unwrap_or(Encoding::Float);
        let upstream_encoding = self.encodings.select(client_encoding);
        let mut target = request.clone();
        target.encoding = Presence::Value(upstream_encoding);
        Ok(Prepared {
            target,
            expectation: Expectation {
                inputs,
                dimensions,
                upstream_encoding,
                client_encoding,
            },
        })
    }
}
