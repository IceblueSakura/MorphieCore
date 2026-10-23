//! Pure target admission, request expectations and explicit client delivery projection.
use crate::{
    adapter::embeddings::{valid_model, *},
    semantic::{task::embedding::*, value::Presence},
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Contract {
    pub max_inputs: usize,
    pub default_dimensions: usize,
    pub dimensions: DimensionSupport,
    pub encodings: EncodingSupport,
    pub tokenizer: Option<TokenizerScope>,
    pub max_input_tokens: Option<usize>,
    pub joint_text: bool,
    pub user: UserPolicy,
    pub reports: ReportPolicy,
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
            Self::Selected(v) => v.contains(&d),
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
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UserPolicy {
    Forward,
    Omit,
    Reject,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReportPolicy {
    Strict,
    Compatible,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeliveryPolicy {
    pub model: String,
    pub encoding: Encoding,
    pub reports: ReportPolicy,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Expectation {
    pub dimensions: Vec<usize>,
    pub upstream_encoding: Encoding,
}
impl Expectation {
    pub fn validate_response(&self, response: &Response) -> Result<(), EmbeddingError> {
        response.validate()?;
        if response.vectors().len() != self.dimensions.len()
            || response
                .vectors()
                .iter()
                .any(|v| self.dimensions.get(v.index()) != Some(&v.values().len()))
        {
            return Err(EmbeddingError);
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Prepared {
    pub target: TargetRequest,
    pub expectation: Expectation,
    pub delivery: DeliveryPolicy,
    pub receipts: Vec<Receipt>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
pub enum Rule {
    #[serde(rename = "embedding.float-json-to-binary32.v1")]
    RoundBinary32,
    #[serde(rename = "embedding.omit-response-id.v1")]
    OmitResponseId,
    #[serde(rename = "embedding.bound-model-label.v1")]
    BoundModel,
    #[serde(rename = "embedding.input-only-total-from-prompt.v1")]
    DeriveTotal,
    #[serde(rename = "embedding.input-from-input-only-total.v1")]
    InputFromTotal,
    #[serde(rename = "embedding.omit-unavailable-usage.v1")]
    OmitUsage,
    #[serde(rename = "embedding.omit-usage-details.v1")]
    OmitDetails,
    #[serde(rename = "embedding.omit-user-hint.v1")]
    OmitUser,
}
impl Rule {
    pub fn name(self) -> &'static str {
        match self {
            Self::RoundBinary32 => "embedding.float-json-to-binary32.v1",
            Self::OmitResponseId => "embedding.omit-response-id.v1",
            Self::BoundModel => "embedding.bound-model-label.v1",
            Self::DeriveTotal => "embedding.input-only-total-from-prompt.v1",
            Self::InputFromTotal => "embedding.input-from-input-only-total.v1",
            Self::OmitUsage => "embedding.omit-unavailable-usage.v1",
            Self::OmitDetails => "embedding.omit-usage-details.v1",
            Self::OmitUser => "embedding.omit-user-hint.v1",
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Request,
    Delivery,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    TotalDerived,
    InputInterpreted,
    UsageOmitted,
    DetailsOmitted,
}
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct Receipt {
    pub rule: Rule,
    pub direction: Direction,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<Action>,
}
fn receipt(rule: Rule, actions: Vec<Action>) -> Receipt {
    Receipt {
        rule,
        direction: Direction::Delivery,
        actions,
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Projection {
    pub response: Delivery,
    pub receipts: Vec<Receipt>,
}

pub fn project_response(
    source: &Response,
    policy: &DeliveryPolicy,
) -> Result<Projection, EmbeddingError> {
    source.validate()?;
    if !valid_model(&policy.model) {
        return Err(EmbeddingError);
    }
    let compatible = policy.reports == ReportPolicy::Compatible;
    let mut receipts = vec![];
    let model = match &source.model {
        Some(model) => model.clone(),
        None if compatible => {
            receipts.push(receipt(Rule::BoundModel, vec![]));
            policy.model.clone()
        }
        None => return Err(EmbeddingError),
    };
    if source.id.is_some() {
        receipts.push(receipt(Rule::OmitResponseId, vec![]));
    }
    let report = source.task.usage();
    let details = report.is_some_and(|u| u.details().is_some());
    let mut usage_rule = None;
    let usage = match report.map(|u| {
        (
            u.reported_input_tokens(),
            u.reported_total_tokens(),
            u.basis(),
        )
    }) {
        Some((Some(input), Some(total), _)) => Some(DeliveryUsage {
            input_tokens: input,
            total_tokens: total,
        }),
        Some((None, Some(total), AccountingBasis::InputOnly)) => {
            usage_rule = Some((Rule::InputFromTotal, Action::InputInterpreted));
            Some(DeliveryUsage {
                input_tokens: total,
                total_tokens: total,
            })
        }
        Some((Some(input), None, AccountingBasis::InputOnly)) if compatible => {
            usage_rule = Some((Rule::DeriveTotal, Action::TotalDerived));
            Some(DeliveryUsage {
                input_tokens: input,
                total_tokens: input,
            })
        }
        _ if compatible => {
            usage_rule = Some((Rule::OmitUsage, Action::UsageOmitted));
            None
        }
        _ => return Err(EmbeddingError),
    };
    if let Some((rule, action)) = usage_rule {
        let mut actions = vec![action];
        if details {
            actions.push(Action::DetailsOmitted);
        }
        receipts.push(receipt(rule, actions));
    } else if details {
        receipts.push(receipt(Rule::OmitDetails, vec![Action::DetailsOmitted]));
    }
    let mut rounded = false;
    let vectors = source
        .vectors()
        .iter()
        .map(|v| match (policy.encoding, v.values()) {
            (Encoding::Base64, EmbeddingValues::ExactJson(values)) => {
                rounded = true;
                let bits = values
                    .iter()
                    .map(|n| {
                        let decimal = n.as_str();
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
                EmbeddingVector::from_binary32(v.index(), bits)
            }
            _ => Ok(v.clone()),
        })
        .collect::<Result<_, _>>()?;
    if rounded {
        receipts.push(receipt(Rule::RoundBinary32, vec![]));
    }
    Ok(Projection {
        response: Delivery {
            model,
            vectors,
            usage,
        },
        receipts,
    })
}
impl Contract {
    pub fn validate(&self) -> Result<(), EmbeddingError> {
        if !(1..=MAX_INPUTS).contains(&self.max_inputs)
            || !(1..=MAX_DIMENSIONS).contains(&self.default_dimensions)
            || self.max_input_tokens == Some(0)
            || self.tokenizer.is_some() && self.max_input_tokens.is_none()
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
            DimensionSupport::Selected(v)
                if !v.is_empty()
                    && v.len() <= MAX_DIMENSIONS
                    && v.iter().all(|d| (1..=MAX_DIMENSIONS).contains(d))
                    && v.iter().collect::<std::collections::BTreeSet<_>>().len() == v.len() =>
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
        request.validate()?;
        if request.task.samples().len() > self.max_inputs {
            return Err(EmbeddingError);
        }
        let mut text_bytes = 0usize;
        let mut token_ids = 0usize;
        let mut values = 0usize;
        let mut dimensions = vec![];
        for sample in request.task.samples() {
            match sample.input() {
                EmbeddingInput::TextParts(parts) => {
                    if !self.joint_text && parts.len() != 1 {
                        return Err(EmbeddingError);
                    }
                    for p in parts {
                        text_bytes = text_bytes.checked_add(p.len()).ok_or(EmbeddingError)?;
                    }
                }
                EmbeddingInput::TokenIds { tokenizer, ids } => {
                    if Some(*tokenizer) != self.tokenizer
                        || self.max_input_tokens.is_none_or(|max| ids.len() > max)
                    {
                        return Err(EmbeddingError);
                    }
                    token_ids = token_ids.checked_add(ids.len()).ok_or(EmbeddingError)?;
                }
            }
            let d = sample.dimensions().unwrap_or(self.default_dimensions);
            if sample
                .dimensions()
                .is_some_and(|d| !self.dimensions.admits(d))
            {
                return Err(EmbeddingError);
            }
            values = values.checked_add(d).ok_or(EmbeddingError)?;
            dimensions.push(d);
        }
        if text_bytes > MAX_INPUT_BYTES || token_ids > MAX_TOKEN_IDS || values > MAX_VECTOR_VALUES {
            return Err(EmbeddingError);
        }
        let mut receipts = vec![];
        let user = match (&request.identity.user, self.user) {
            (Presence::Value(_), UserPolicy::Reject) => return Err(EmbeddingError),
            (Presence::Value(_), UserPolicy::Omit) => {
                receipts.push(Receipt {
                    rule: Rule::OmitUser,
                    direction: Direction::Request,
                    actions: vec![],
                });
                None
            }
            (Presence::Value(s), UserPolicy::Forward) => Some(s.clone()),
            _ => None,
        };
        let encoding = request.encoding.unwrap_or(Encoding::Float);
        let upstream_encoding = self.encodings.select(encoding);
        Ok(Prepared {
            target: TargetRequest {
                task: request.task.clone(),
                encoding: upstream_encoding,
                user,
            },
            expectation: Expectation {
                dimensions,
                upstream_encoding,
            },
            delivery: DeliveryPolicy {
                model: request.model.clone(),
                encoding,
                reports: self.reports,
            },
            receipts,
        })
    }
}
