//! Target admission of final embedding values, independent of registry, credentials and I/O.
use crate::{
    adapter::embeddings::{Request, Response},
    semantic::{task::embedding::*, value::Presence},
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Contract {
    pub max_inputs: usize,
    pub dimensions: DimensionSupport,
    pub user: bool,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DimensionSupport {
    UpTo(usize),
    Selected(Vec<usize>),
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Projection {
    pub response: Response,
    pub omitted_response_identifier: bool,
}
/// Only ancillary response identity is omitted; task values and usage basis remain unchanged.
pub fn project_response(source: &Response) -> Result<Projection, EmbeddingError> {
    source.validate()?;
    let omitted_response_identifier = source.id != Presence::Absent;
    let mut response = source.clone();
    response.id = Presence::Absent;
    response.validate()?;
    Ok(Projection {
        response,
        omitted_response_identifier,
    })
}
impl Contract {
    pub fn validate(&self) -> Result<(), EmbeddingError> {
        if !(1..=MAX_INPUTS).contains(&self.max_inputs) {
            return Err(EmbeddingError);
        }
        match &self.dimensions {
            DimensionSupport::UpTo(max) if (1..=MAX_DIMENSIONS).contains(max) => Ok(()),
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
        self.validate()?;
        request.task.validate()?;
        if request.task.inputs().len() > self.max_inputs
            || request.encoding == Presence::Null
            || request.task.dimensions.value().is_some_and(|d| {
                d.checked_mul(request.task.inputs().len())
                    .is_none_or(|values| values > MAX_VECTOR_VALUES)
            })
            || request
                .task
                .dimensions
                .value()
                .is_some_and(|d| match &self.dimensions {
                    DimensionSupport::UpTo(max) => d > max,
                    DimensionSupport::Selected(values) => !values.contains(d),
                })
            || !self.user && request.identity.user != Presence::Absent
        {
            return Err(EmbeddingError);
        }
        Ok(())
    }
}
