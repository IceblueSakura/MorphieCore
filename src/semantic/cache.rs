//! Explicit prefix stability intent. No cache store, TTL execution or routing state.
use super::{
    context::{ContextError, ExecutionHints},
    task::generation::{
        GenerationRequest, HistoryDependency, ItemId, MAX_TEXT_BYTES, RequestDependencyProof,
    },
    value::Text,
};
use sha2::{Digest, Sha256};

/// Trusted non-secret compatibility label, independent of replay, session and key.
/// Equality is a caller-declared scope constraint, not issuer authentication.
#[derive(Clone, Eq, PartialEq)]
pub struct CachePrefixScope(Text);
impl CachePrefixScope {
    pub fn new(value: &str) -> Result<Self, ContextError> {
        if value.chars().any(char::is_control) {
            return Err(ContextError::Invalid("cache prefix scope"));
        }
        Ok(Self(Text::new(value, "cache prefix scope", 256).map_err(
            |_| ContextError::Invalid("cache prefix scope"),
        )?))
    }
}
impl std::fmt::Debug for CachePrefixScope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CachePrefixScope([redacted])")
    }
}
/// Select an entire item prefix, without implicitly sending a cache directive.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CachePrefixIntent {
    through: ItemId,
}
impl CachePrefixIntent {
    pub const fn through(owner: ItemId) -> Self {
        Self { through: owner }
    }
    fn check_boundary(self, request: &GenerationRequest) -> Result<(), ContextError> {
        request.validate()?;
        if !request.items().iter().any(|(id, _)| *id == self.through)
            || request.message_groups().any(|group| {
                group.items().iter().any(|(id, _)| *id == self.through)
                    && group
                        .items()
                        .last()
                        .is_some_and(|(id, _)| *id != self.through)
            })
        {
            return Err(ContextError::Invalid("cache prefix boundary"));
        }
        Ok(())
    }
    pub fn capture(
        self,
        request: &GenerationRequest,
        context: CachePrefixContext<'_>,
        scope: &CachePrefixScope,
    ) -> Result<CachePrefixProof, ContextError> {
        self.check_boundary(request)?;
        Ok(CachePrefixProof {
            intent: self,
            scope: scope.clone(),
            history: RequestDependencyProof::capture(
                request,
                HistoryDependency::PrefixThrough(self.through),
                crate::semantic::task::generation::SettingsDependency::All,
            )?,
            context: context_digest(context)?,
        })
    }
}
/// Borrow actual binding/hints/carrier values; none becomes a second authority.
/// Check on final candidate-local input when a caller needs target prefix stability.
#[derive(Clone, Copy)]
pub struct CachePrefixContext<'a> {
    pub model: &'a str,
    pub hints: &'a ExecutionHints,
    pub grouping: Option<&'a str>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CachePrefixProof {
    intent: CachePrefixIntent,
    scope: CachePrefixScope,
    history: RequestDependencyProof,
    context: [u8; 32],
}
impl CachePrefixProof {
    pub fn check(
        &self,
        request: &GenerationRequest,
        context: CachePrefixContext<'_>,
        scope: &CachePrefixScope,
    ) -> Result<(), ContextError> {
        self.intent.check_boundary(request)?;
        self.history.check(request)?;
        if self.scope != *scope || self.context != context_digest(context)? {
            return Err(ContextError::Invalid("cache prefix context changed"));
        }
        Ok(())
    }
}
fn context_digest(context: CachePrefixContext<'_>) -> Result<[u8; 32], ContextError> {
    context.hints.validate()?;
    if context.model.is_empty()
        || context.model.len() > 256
        || context.model.chars().any(char::is_control)
        || context.grouping.is_some_and(|value| {
            value.is_empty()
                || value.len() > 1024
                || value.chars().count() > 256
                || value.chars().any(char::is_control)
        })
    {
        return Err(ContextError::Invalid("cache prefix context"));
    }
    struct HashWriter {
        hash: Sha256,
        remaining: usize,
    }
    impl std::io::Write for HashWriter {
        fn write(&mut self, value: &[u8]) -> std::io::Result<usize> {
            self.remaining = self
                .remaining
                .checked_sub(value.len())
                .ok_or_else(|| std::io::Error::other("cache prefix limit"))?;
            self.hash.update(value);
            Ok(value.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut writer = HashWriter {
        hash: Sha256::new(),
        remaining: MAX_TEXT_BYTES,
    };
    // Streaming, length-delimited JSON avoids both ambiguous concatenation and a
    // second serialized context body. All reported/hinted presence stays visible.
    serde_json::to_writer(
        &mut writer,
        &(context.model, context.hints, context.grouping),
    )
    .map_err(|_| ContextError::Limit)?;
    Ok(writer.hash.finalize().into())
}
