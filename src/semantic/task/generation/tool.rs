//! Client-executed tools. Payloads and grammar are data; this module never executes them.
use super::{GenerationError, MAX_TEXT_BYTES, StructuredValue};
use crate::semantic::value::Text;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StrictDefault {
    NonStrict,
    NormalizeSchema,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FunctionStrictness {
    Omitted(StrictDefault),
    Explicit(bool),
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CallerMode {
    Direct,
    Programmatic,
}
/// Active tool-definition dispatch. Inactive defaults stay absent.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ToolDispatch {
    pub async_call: bool,
    pub defer_loading: bool,
    pub allowed_callers: Option<Vec<CallerMode>>,
}
impl ToolDispatch {
    pub fn is_inactive(&self) -> bool {
        !self.async_call && !self.defer_loading && self.allowed_callers.is_none()
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CallOrigin {
    Program { caller_id: Text },
}
/// Active call context. Direct callers, empty namespaces and async false stay absent.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CallContext {
    pub namespace: Option<Text>,
    pub async_call: bool,
    pub caller: Option<CallOrigin>,
    /// Explicit native reference domain; never inferred from a result's owner.
    pub alias_domain: Option<super::NativeAliasDomain>,
    /// Historical definition association on function/custom calls only, never results.
    pub definition: Option<super::ToolDefinitionBinding>,
}
impl CallContext {
    pub fn is_direct(&self) -> bool {
        self.namespace.is_none() && !self.async_call && self.caller.is_none()
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FunctionTool {
    pub name: Text,
    pub description: Option<String>,
    pub parameters: Option<super::SchemaDocument>,
    pub strict: FunctionStrictness,
    pub output_schema: Option<super::SchemaDocument>,
    pub dispatch: ToolDispatch,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CustomFormat {
    Text,
    Grammar {
        syntax: GrammarSyntax,
        definition: Text,
    },
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GrammarSyntax {
    Lark,
    Regex,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CustomTool {
    pub name: Text,
    pub description: Option<String>,
    pub format: Option<CustomFormat>,
    pub dispatch: ToolDispatch,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToolNamespace {
    pub name: Text,
    pub description: String,
    /// Ordered leaf definitions. Nested namespaces are invalid.
    pub tools: Vec<ToolDefinition>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ToolDefinition {
    Function(FunctionTool),
    Custom(CustomTool),
    Namespace(ToolNamespace),
}
impl ToolDefinition {
    pub fn name(&self) -> &Text {
        match self {
            Self::Function(t) => &t.name,
            Self::Custom(t) => &t.name,
            Self::Namespace(t) => &t.name,
        }
    }
    pub fn dispatch_inactive(&self) -> bool {
        match self {
            Self::Function(t) => t.dispatch.is_inactive(),
            Self::Custom(t) => t.dispatch.is_inactive(),
            Self::Namespace(t) => t.tools.iter().all(Self::dispatch_inactive),
        }
    }
    pub fn kind(&self) -> Option<ToolKind> {
        match self {
            Self::Function(_) => Some(ToolKind::Function),
            Self::Custom(_) => Some(ToolKind::Custom),
            Self::Namespace(_) => None,
        }
    }
    /// A validated namespace contains only leaves; this view does not flatten identity.
    pub fn leaves(&self) -> &[Self] {
        match self {
            Self::Namespace(group) => &group.tools,
            _ => std::slice::from_ref(self),
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ToolKind {
    Function,
    Custom,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToolReference {
    pub kind: ToolKind,
    pub name: Text,
    pub namespace: Option<Text>,
}
#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub enum ToolChoice {
    None,
    #[default]
    Auto,
    Required,
    Specific(Text),
    Custom(Text),
    Qualified(ToolReference),
    Allowed {
        required: bool,
        tools: Vec<ToolReference>,
    },
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ItemLifecycle {
    #[default]
    Completed,
    Incomplete,
    InProgress,
}
/// A parsed view cannot replace raw authority; structured values own no second string.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArgumentFormat {
    Raw,
    Json,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ToolArguments {
    Raw(String),
    Structured(StructuredValue),
    StructuredPartial(String),
}
impl ToolArguments {
    pub(crate) fn same_authority(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Raw(a), Self::Raw(b))
            | (Self::StructuredPartial(a), Self::StructuredPartial(b)) => a == b,
            (Self::Structured(a), Self::Structured(b)) => ordered_json_equal(a.value(), b.value()),
            _ => false,
        }
    }
    pub fn as_raw(&self) -> Option<&str> {
        match self {
            Self::Raw(value) => Some(value),
            Self::Structured(_) | Self::StructuredPartial(_) => None,
        }
    }
    pub fn as_structured(&self) -> Option<&StructuredValue> {
        match self {
            Self::Structured(value) => Some(value),
            Self::Raw(_) | Self::StructuredPartial(_) => None,
        }
    }
    pub(crate) fn bytes(&self) -> Result<usize, GenerationError> {
        match self {
            Self::Raw(value) | Self::StructuredPartial(value) if value.len() <= MAX_TEXT_BYTES => {
                Ok(value.len())
            }
            Self::Raw(_) | Self::StructuredPartial(_) => Err(GenerationError::Limit),
            Self::Structured(value) => value.bytes(),
        }
    }
}
impl From<String> for ToolArguments {
    fn from(value: String) -> Self {
        Self::Raw(value)
    }
}
// Both trees passed the structured-value depth/node budget. Object order is
// authority here even though serde_json::Value equality ignores it.
pub(super) fn ordered_json_equal(left: &serde_json::Value, right: &serde_json::Value) -> bool {
    use serde_json::Value;
    match (left, right) {
        (Value::Object(a), Value::Object(b)) => {
            a.len() == b.len()
                && a.iter()
                    .zip(b)
                    .all(|((ak, av), (bk, bv))| ak == bk && ordered_json_equal(av, bv))
        }
        (Value::Array(a), Value::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| ordered_json_equal(a, b))
        }
        _ => left == right,
    }
}
impl From<&str> for ToolArguments {
    fn from(value: &str) -> Self {
        Self::Raw(value.into())
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToolCall {
    pub call_id: Text,
    pub name: Text,
    pub arguments: ToolArguments,
    pub status: ItemLifecycle,
    pub context: CallContext,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CustomCall {
    pub call_id: Text,
    pub name: Text,
    pub input: String,
    pub context: CallContext,
}
/// Programmatic-calling program item. Opaque `code` and `fingerprint` round-trip
/// verbatim; no code may rebuild or validate them.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Program {
    pub call_id: Text,
    pub code: String,
    pub fingerprint: String,
}
/// Terminal result of a program item, keyed by the program call ID.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProgramOutput {
    pub call_id: Text,
    pub result: String,
    pub status: ItemLifecycle,
}
