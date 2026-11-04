//! Client-executed tools. Payloads and grammar are data; this module never executes them.
pub(in crate::semantic::task::generation) mod provider;
pub(in crate::semantic::task::generation) mod result;
use crate::semantic::task::generation::{
    GenerationError, GenerationSettings, Item, ItemId, MAX_TEXT_BYTES, ReplayValue,
    StructuredValue, ToolChange, ToolChangeAction,
};
use crate::semantic::value::Text;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StrictDefault {
    NonStrict,
    /// Conditional strictness under the source's compatibility rule.
    Conditional,
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
    pub(in crate::semantic::task::generation) fn validate(&self) -> Result<usize, GenerationError> {
        let Some(callers) = &self.allowed_callers else {
            return Ok(0);
        };
        if callers.is_empty() || callers.len() > 2 || callers.len() == 2 && callers[0] == callers[1]
        {
            return Err(GenerationError::InvalidToolDefinition);
        }
        Ok(std::mem::size_of_val(callers.as_slice()))
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
    pub alias_domain: Option<crate::semantic::task::generation::NativeAliasDomain>,
}
impl CallContext {
    pub fn is_direct(&self) -> bool {
        self.namespace.is_none() && !self.async_call && self.caller.is_none()
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PrefixCacheTtl {
    FiveMinutes,
    OneHour,
}
/// An exact prefix boundary owned by one content use.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PrefixCacheBoundary {
    /// Use the request's cache lifetime policy; never copy an effective default here.
    RequestLifetime,
    /// The boundary supplies its own TTL, independent of request-wide caching.
    Ttl(PrefixCacheTtl),
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FunctionTool {
    pub name: Text,
    pub description: Option<String>,
    pub parameters: Option<crate::semantic::task::generation::SchemaDocument>,
    /// Ordered model-visible input objects, not locally schema-validated instances.
    /// An explicit empty list differs from omission.
    pub input_examples: Option<Vec<StructuredValue>>,
    pub strict: FunctionStrictness,
    pub output_schema: Option<crate::semantic::task::generation::SchemaDocument>,
    pub dispatch: ToolDispatch,
    /// An explicit prefix boundary ending at this tool definition, not cache affinity.
    pub cache: Option<PrefixCacheTtl>,
    /// Unbuffered, unvalidated argument streaming; absent keeps the target default.
    pub eager_input: Option<bool>,
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
    pub(crate) fn leaves_mut(&mut self) -> &mut [Self] {
        match self {
            Self::Namespace(group) => &mut group.tools,
            _ => std::slice::from_mut(self),
        }
    }
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
/// Visit owned functions without changing batch, namespace or item identity.
pub(crate) fn for_each_function_tool_mut(
    settings: &mut GenerationSettings,
    items: &mut [(ItemId, Item)],
    mut visit: impl FnMut(Option<ItemId>, &mut FunctionTool),
) {
    fn definitions(
        tools: &mut [ToolDefinition],
        owner: Option<ItemId>,
        visit: &mut impl FnMut(Option<ItemId>, &mut FunctionTool),
    ) {
        for tool in tools.iter_mut().flat_map(ToolDefinition::leaves_mut) {
            if let ToolDefinition::Function(tool) = tool {
                visit(owner, tool);
            }
        }
    }
    definitions(
        settings.tools.as_deref_mut().unwrap_or_default(),
        None,
        &mut visit,
    );
    for (id, item) in items {
        match item {
            Item::ToolBatch(batch) => definitions(&mut batch.tools, Some(*id), &mut visit),
            Item::ToolChange(ToolChange {
                action: ToolChangeAction::AddDefinition(tool),
                ..
            }) => visit(Some(*id), tool),
            _ => {}
        }
    }
}
/// Borrowed declaration syntax, including function-only replacement additions.
#[derive(Clone, Copy)]
pub enum ToolDeclaration<'a> {
    Function(&'a FunctionTool),
    Custom(&'a CustomTool),
    Namespace(&'a ToolNamespace),
}
impl<'a> From<&'a ToolDefinition> for ToolDeclaration<'a> {
    fn from(value: &'a ToolDefinition) -> Self {
        match value {
            ToolDefinition::Function(tool) => Self::Function(tool),
            ToolDefinition::Custom(tool) => Self::Custom(tool),
            ToolDefinition::Namespace(group) => Self::Namespace(group),
        }
    }
}
impl<'a> ToolDeclaration<'a> {
    pub fn dispatch_inactive(self) -> bool {
        match self {
            Self::Function(tool) => tool.dispatch.is_inactive(),
            Self::Custom(tool) => tool.dispatch.is_inactive(),
            Self::Namespace(group) => group.tools.iter().all(ToolDefinition::dispatch_inactive),
        }
    }
    pub fn leaves(self) -> impl Iterator<Item = Self> {
        let group = match self {
            Self::Namespace(group) => Some(group),
            _ => None,
        };
        std::iter::once(self)
            .filter(move |_| group.is_none())
            .chain(
                group
                    .into_iter()
                    .flat_map(|group| group.tools.iter().map(Self::from)),
            )
    }
    pub fn function(self) -> Option<&'a FunctionTool> {
        match self {
            Self::Function(tool) => Some(tool),
            _ => None,
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
pub(in crate::semantic::task::generation) fn ordered_json_equal(
    left: &serde_json::Value,
    right: &serde_json::Value,
) -> bool {
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
    pub cache: Option<PrefixCacheTtl>,
    pub replay: Option<ReplayValue>,
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
