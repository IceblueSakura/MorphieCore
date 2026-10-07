//! One authoritative schema and explicit inert resources, never a network resolver.
use super::*;
use crate::semantic::value::Text;
use serde_json::Value;

pub const MAX_SCHEMA_RESOURCES: usize = 32;
pub const MAX_SCHEMA_REFERENCES: usize = 8192;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SchemaDialect {
    /// No dialect was reported; only the existing local Generation subset is admitted.
    Unspecified,
    /// The admitted 2020-12 vocabulary, not a claim of full instance evaluation.
    Draft202012,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SchemaResource {
    pub id: Text,
    pub value: Value,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SchemaDocument {
    dialect: SchemaDialect,
    root: Value,
    resources: Vec<SchemaResource>,
}
impl From<Value> for SchemaDocument {
    /// Raw unversioned input. Owning request/settings validation remains mandatory.
    fn from(root: Value) -> Self {
        Self {
            dialect: SchemaDialect::Unspecified,
            root,
            resources: vec![],
        }
    }
}
impl SchemaDocument {
    pub fn new(
        dialect: SchemaDialect,
        root: Value,
        resources: Vec<SchemaResource>,
    ) -> Result<Self, GenerationError> {
        let schema = Self {
            dialect,
            root,
            resources,
        };
        schema.validate()?;
        Ok(schema)
    }
    pub const fn dialect(&self) -> SchemaDialect {
        self.dialect
    }
    pub fn root(&self) -> &Value {
        &self.root
    }
    pub fn root_mut(&mut self) -> &mut Value {
        &mut self.root
    }
    pub fn resources(&self) -> &[SchemaResource] {
        &self.resources
    }
    pub fn validate(&self) -> Result<usize, GenerationError> {
        super::schema::validate(self, super::schema::Mode::General)
    }
    /// Current standard profiles neither declare a dialect nor carry supplied documents.
    pub fn is_unversioned_local(&self) -> bool {
        self.dialect == SchemaDialect::Unspecified && self.resources.is_empty()
    }
    pub(super) fn same_authority(&self, other: &Self) -> bool {
        self == other
            && super::tool::ordered_json_equal(&self.root, &other.root)
            && self
                .resources
                .iter()
                .zip(&other.resources)
                .all(|(a, b)| super::tool::ordered_json_equal(&a.value, &b.value))
    }
}
