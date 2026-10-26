//! Protocol-owned endpoint capabilities, not a client protocol or execution policy.
use crate::{
    protocol::{
        anthropic,
        cache::CacheTargetPolicy,
        google::{
            generate_content as google_generate_content, interactions as google_interactions,
        },
        openai::TargetContract as OpenAiCaps,
    },
    semantic::task::generation::GenerationFeatureSet,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TargetContract {
    OpenAi(Box<OpenAiCaps>),
    AnthropicMessages(Box<anthropic::messages::TargetContract>),
    GoogleGenerateContent(Box<google_generate_content::TargetContract>),
    GoogleInteractions(Box<google_interactions::TargetContract>),
}
impl From<OpenAiCaps> for TargetContract {
    fn from(value: OpenAiCaps) -> Self {
        Self::OpenAi(Box::new(value))
    }
}
impl From<anthropic::messages::TargetContract> for TargetContract {
    fn from(value: anthropic::messages::TargetContract) -> Self {
        Self::AnthropicMessages(Box::new(value))
    }
}
impl From<google_generate_content::TargetContract> for TargetContract {
    fn from(value: google_generate_content::TargetContract) -> Self {
        Self::GoogleGenerateContent(Box::new(value))
    }
}
impl From<google_interactions::TargetContract> for TargetContract {
    fn from(value: google_interactions::TargetContract) -> Self {
        Self::GoogleInteractions(Box::new(value))
    }
}
impl TargetContract {
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
    pub fn messages(&self) -> Option<&anthropic::messages::TargetContract> {
        match self {
            Self::AnthropicMessages(c) => Some(c),
            _ => None,
        }
    }
    pub fn messages_mut(&mut self) -> Option<&mut anthropic::messages::TargetContract> {
        match self {
            Self::AnthropicMessages(c) => Some(c),
            _ => None,
        }
    }
    pub fn semantics(&self) -> &GenerationFeatureSet {
        match self {
            Self::OpenAi(c) => &c.semantics,
            Self::AnthropicMessages(c) => &c.semantics,
            Self::GoogleGenerateContent(c) => &c.semantics,
            Self::GoogleInteractions(c) => &c.semantics,
        }
    }
    pub(crate) fn messages_terminal_policy(
        &self,
        request: &crate::protocol::GenerationRequestEnvelope,
    ) -> Option<anthropic::messages::client::GoogleTerminalPolicy> {
        // Both Google encoders forward nonempty stop controls unchanged and never
        // insert defaults. Absence here is therefore an effective-request fact.
        if !request.task.semantic.controls().stop_sequences.is_empty() {
            return None;
        }
        self.google_messages_terminal_profile()
    }
    pub(crate) fn google_messages_terminal_profile(
        &self,
    ) -> Option<anthropic::messages::client::GoogleTerminalPolicy> {
        use anthropic::messages::client::GoogleTerminalPolicy as Policy;
        match self {
            Self::GoogleGenerateContent(c) if c.messages_terminal_projection => {
                Some(Policy::GenerateContentWithoutStops)
            }
            Self::GoogleInteractions(c) if c.messages_terminal_projection => {
                Some(Policy::InteractionsWithoutStops)
            }
            _ => None,
        }
    }
    /// Accepted request features after selected adaptation, not native support.
    /// Value/mode restrictions still belong to the shared target encoder.
    pub(crate) fn accepted_semantics(&self) -> GenerationFeatureSet {
        use crate::{
            protocol::reasoning::ReasoningConversion, semantic::task::generation::EffortScope,
        };
        let mut contract = self.semantics().clone();
        if matches!(self, Self::AnthropicMessages(c)
            if c.sampling == anthropic::messages::SamplingPolicy::Fixed)
        {
            // Selected omission admits other source protocols' controls without
            // claiming removed fields as native Messages carriers.
            contract.temperature = true;
            contract.top_p = true;
        }
        if matches!(self, Self::GoogleGenerateContent(c)
            if c.sampling == crate::protocol::google::SamplingPolicy::Ignored)
            || matches!(self, Self::GoogleInteractions(c)
                if c.sampling == crate::protocol::google::SamplingPolicy::Ignored)
        {
            contract.temperature = true;
            contract.top_p = true;
        }
        let policy = match self {
            Self::GoogleGenerateContent(c) => c.reasoning_policy.as_ref(),
            Self::GoogleInteractions(c) => c.reasoning_policy.as_ref(),
            _ => None,
        };
        if contract.reasoning
            && policy.is_some_and(|p| {
                matches!(
                    p.conversion,
                    ReasoningConversion::Scope {
                        from: EffortScope::Response,
                        to: EffortScope::Reasoning,
                        ..
                    }
                )
            })
        {
            contract.response_effort = true;
        }
        contract
    }
    pub fn cache(&self) -> CacheTargetPolicy {
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
