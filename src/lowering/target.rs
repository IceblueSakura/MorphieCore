//! Protocol-owned endpoint capabilities, not a client protocol or execution policy.
use crate::protocol::openai::TargetCaps as OpenAiCaps;
use crate::{
    protocol::{
        anthropic, cache::CacheProjection, google::generate_content as google_generate_content,
        google::interactions as google_interactions,
    },
    semantic::task::generation::GenerationSemanticContract,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TargetCaps {
    OpenAi(Box<OpenAiCaps>),
    AnthropicMessages(Box<anthropic::messages::TargetCaps>),
    GoogleGenerateContent(Box<google_generate_content::TargetCaps>),
    GoogleInteractions(Box<google_interactions::TargetCaps>),
}
impl From<OpenAiCaps> for TargetCaps {
    fn from(value: OpenAiCaps) -> Self {
        Self::OpenAi(Box::new(value))
    }
}
impl From<anthropic::messages::TargetCaps> for TargetCaps {
    fn from(value: anthropic::messages::TargetCaps) -> Self {
        Self::AnthropicMessages(Box::new(value))
    }
}
impl From<google_generate_content::TargetCaps> for TargetCaps {
    fn from(value: google_generate_content::TargetCaps) -> Self {
        Self::GoogleGenerateContent(Box::new(value))
    }
}
impl From<google_interactions::TargetCaps> for TargetCaps {
    fn from(value: google_interactions::TargetCaps) -> Self {
        Self::GoogleInteractions(Box::new(value))
    }
}
impl TargetCaps {
    pub fn model_limits(&self) -> &crate::protocol::model_limits::ModelLimits {
        match self {
            Self::OpenAi(c) => &c.model_limits,
            Self::AnthropicMessages(c) => &c.model_limits,
            Self::GoogleGenerateContent(c) => &c.model_limits,
            Self::GoogleInteractions(c) => &c.model_limits,
        }
    }
    pub fn model_limits_mut(&mut self) -> &mut crate::protocol::model_limits::ModelLimits {
        match self {
            Self::OpenAi(c) => &mut c.model_limits,
            Self::AnthropicMessages(c) => &mut c.model_limits,
            Self::GoogleGenerateContent(c) => &mut c.model_limits,
            Self::GoogleInteractions(c) => &mut c.model_limits,
        }
    }
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
    pub fn messages(&self) -> Option<&anthropic::messages::TargetCaps> {
        match self {
            Self::AnthropicMessages(c) => Some(c),
            _ => None,
        }
    }
    pub fn messages_mut(&mut self) -> Option<&mut anthropic::messages::TargetCaps> {
        match self {
            Self::AnthropicMessages(c) => Some(c),
            _ => None,
        }
    }
    pub fn semantics(&self) -> &GenerationSemanticContract {
        match self {
            Self::OpenAi(c) => &c.semantics,
            Self::AnthropicMessages(c) => &c.semantics,
            Self::GoogleGenerateContent(c) => &c.semantics,
            Self::GoogleInteractions(c) => &c.semantics,
        }
    }
    pub fn cache(&self) -> CacheProjection {
        match self {
            Self::OpenAi(c) => c.cache,
            Self::AnthropicMessages(c) => c.cache,
            Self::GoogleGenerateContent(c) => c.cache,
            Self::GoogleInteractions(c) => c.cache,
        }
    }
    pub fn standard_context(&self) -> bool {
        match self {
            Self::OpenAi(c) => c.standard_context,
            Self::AnthropicMessages(_)
            | Self::GoogleGenerateContent(_)
            | Self::GoogleInteractions(_) => false,
        }
    }
    pub fn forced_stream(&self) -> bool {
        match self {
            Self::OpenAi(c) => c.adaptation.rules.responses_forced_stream,
            Self::AnthropicMessages(_) => false,
            // Google bindings always stream; JSON clients get the aggregate.
            Self::GoogleGenerateContent(_) | Self::GoogleInteractions(_) => true,
        }
    }
}
