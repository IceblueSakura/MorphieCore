//! Protocol-owned endpoint capabilities, not a client protocol or execution policy.
use crate::protocol::openai::TargetCaps as OpenAiCaps;
use crate::{
    protocol::{anthropic, cache::CacheProjection},
    semantic::task::generation::GenerationSemanticContract,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TargetCaps {
    OpenAi(Box<OpenAiCaps>),
    AnthropicMessages(Box<anthropic::TargetCaps>),
}
impl From<OpenAiCaps> for TargetCaps {
    fn from(value: OpenAiCaps) -> Self {
        Self::OpenAi(Box::new(value))
    }
}
impl From<anthropic::TargetCaps> for TargetCaps {
    fn from(value: anthropic::TargetCaps) -> Self {
        Self::AnthropicMessages(Box::new(value))
    }
}
impl TargetCaps {
    pub fn openai(&self) -> Option<&OpenAiCaps> {
        match self {
            Self::OpenAi(c) => Some(c),
            _ => None,
        }
    }
    pub fn openai_mut(&mut self) -> Option<&mut OpenAiCaps> {
        match self {
            Self::OpenAi(c) => Some(c),
            _ => None,
        }
    }
    pub fn messages(&self) -> Option<&anthropic::TargetCaps> {
        match self {
            Self::AnthropicMessages(c) => Some(c),
            _ => None,
        }
    }
    pub fn messages_mut(&mut self) -> Option<&mut anthropic::TargetCaps> {
        match self {
            Self::AnthropicMessages(c) => Some(c),
            _ => None,
        }
    }
    pub fn semantics(&self) -> &GenerationSemanticContract {
        match self {
            Self::OpenAi(c) => &c.semantics,
            Self::AnthropicMessages(c) => &c.semantics,
        }
    }
    pub fn cache(&self) -> CacheProjection {
        match self {
            Self::OpenAi(c) => c.cache,
            Self::AnthropicMessages(c) => c.cache,
        }
    }
    pub fn standard_context(&self) -> bool {
        match self {
            Self::OpenAi(c) => c.standard_context,
            Self::AnthropicMessages(_) => false,
        }
    }
    pub fn forced_stream(&self) -> bool {
        match self {
            Self::OpenAi(c) => c.adaptation.rules.responses_forced_stream,
            Self::AnthropicMessages(_) => false,
        }
    }
}
