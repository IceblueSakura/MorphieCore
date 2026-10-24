//! Explicit deployment parameters over existing catalog bindings.
use super::config::{
    ChatProfile, EmbeddingEntry, Entry, ImageEntry, SpeechEntry, StartupError, TaskEntries,
    TranscriptionEntry,
};
use crate::{
    credential::CredentialManager,
    gateway::Credentials,
    lowering::target::TargetCaps,
    protocol::{
        openai::Profile,
        reasoning::{ReasoningConversion, ReasoningRequestPolicy, SHARED_BUDGETS, ThinkingBudgets},
    },
    provider::{CredentialBindingId, EndpointPath, ProviderDefinition, TrustedOrigin},
    semantic::task::generation::{EffortLevel, EffortScope},
    topology::{self, CompiledTopology, Endpoint, EndpointId, ProtocolProfile},
};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Models {
    pub models: BTreeMap<String, Model>,
    #[serde(default = "enabled")]
    pub implicit_cache_affinity: bool,
}
fn enabled() -> bool {
    true
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Model {
    binding: Option<String>,
    #[serde(default)]
    candidates: Vec<Candidate>,
    max_attempts: Option<usize>,
    total_timeout_ms: Option<u64>,
    thinking_tokens: Option<ThinkingTokens>,
    chat_profile: Option<ChatProfile>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Candidate {
    binding: String,
    allowed_effort: Option<Vec<String>>,
    maximum_output_tokens: Option<u64>,
    #[serde(default)]
    default_output_tokens: crate::semantic::value::Presence<u64>,
    base_url: Option<String>,
    upstream_model: Option<String>,
    credential: Option<String>,
    attempt_timeout_ms: Option<u64>,
    #[serde(default)]
    reasoning_as_text: bool,
    thinking_conversion: Option<ThinkingConversion>,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ThinkingConversion {
    BudgetToEffort,
    EffortToBudget,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ThinkingTokens {
    minimal: u64,
    low: u64,
    medium: u64,
    high: u64,
    xhigh: u64,
    max: u64,
}
impl ThinkingTokens {
    fn table(&self) -> ThinkingBudgets {
        ThinkingBudgets {
            entries: vec![
                (EffortLevel::Minimal, self.minimal),
                (EffortLevel::Low, self.low),
                (EffortLevel::Medium, self.medium),
                (EffortLevel::High, self.high),
                (EffortLevel::XHigh, self.xhigh),
                (EffortLevel::Max, self.max),
            ]
            .into(),
        }
    }
}

pub(super) struct Plan {
    pub topology: CompiledTopology,
    pub entries: TaskEntries,
    pub credentials: Credentials,
}
fn invalid() -> StartupError {
    StartupError::Configuration("models")
}
fn timeout(ms: u64) -> Result<u64, StartupError> {
    if (1..=86_400_000).contains(&ms) {
        Ok(ms)
    } else {
        Err(invalid())
    }
}

impl Models {
    pub fn bind(self, manager: &CredentialManager) -> Result<Plan, StartupError> {
        if self.models.is_empty() || self.models.len() > 64 {
            return Err(invalid());
        }
        let catalog = topology::catalog::default_topology().map_err(|_| StartupError::Binding)?;
        let pools: BTreeMap<_, _> = manager
            .pools()
            .map_err(|_| StartupError::Credentials)?
            .into_iter()
            .map(|(provider, id, status)| ((provider, id), status.config))
            .collect();
        let mut credentials = Credentials::new();
        let mut entries = TaskEntries::default();
        let mut endpoints = Vec::new();
        let mut routes = Vec::new();
        let mut public_models = Vec::new();
        let mut bind_pool = |provider: &str, pool: &str, id: CredentialBindingId| {
            let config = pools
                .get(&(provider.to_owned(), pool.to_owned()))
                .ok_or(StartupError::Credentials)?;
            credentials.insert_pool(
                id,
                manager
                    .bind_pool(provider, config)
                    .map_err(|_| StartupError::Credentials)?,
            )
        };
        for (model_index, (label, config)) in self.models.into_iter().enumerate() {
            let transcription = match &config.binding {
                Some(binding) => Some(
                    catalog
                        .transcription_binding(
                            &label,
                            &EndpointId::new(binding).map_err(|_| invalid())?,
                        )
                        .ok_or(StartupError::Binding)?,
                ),
                None => catalog.transcription_route(&label),
            };
            let Some(public) = catalog.model(&label) else {
                // Frozen tasks keep their single catalog target and existing pool rules.
                if !config.candidates.is_empty()
                    || config.max_attempts.is_some()
                    || config.total_timeout_ms.is_some()
                    || config.thinking_tokens.is_some()
                    || config.chat_profile.is_some()
                {
                    return Err(invalid());
                }
                let (provider, credential) = if let Some(route) = catalog.image_route(&label) {
                    entries.images.push(ImageEntry { model: label });
                    (&route.endpoint.provider, &route.endpoint.credential)
                } else if let Some(route) = catalog.speech_route(&label) {
                    entries.speech.push(SpeechEntry { model: label });
                    (&route.endpoint.provider, &route.endpoint.credential)
                } else if let Some(route) = transcription {
                    entries.transcriptions.push(TranscriptionEntry {
                        model: label,
                        binding: Some(route.endpoint.id.clone()),
                    });
                    (&route.endpoint.provider, &route.endpoint.credential)
                } else if let Some(route) = catalog.embedding_route(&label) {
                    entries.embeddings.push(EmbeddingEntry { model: label });
                    (&route.endpoint.provider, &route.endpoint.credential)
                } else {
                    return Err(StartupError::Binding);
                };
                bind_pool(provider.as_str(), credential.as_str(), credential.clone())?;
                continue;
            };
            if config.binding.is_some()
                || config.candidates.is_empty()
                || config.candidates.len() > 64
                || config.max_attempts.is_some_and(|n| !(1..=64).contains(&n))
                || config.thinking_tokens.is_some()
                    && config
                        .candidates
                        .iter()
                        .all(|c| c.thinking_conversion.is_none())
            {
                return Err(invalid());
            }
            let template_route = catalog.route(&public.route).ok_or(StartupError::Binding)?;
            let mut route = template_route.clone();
            route.endpoints.clear();
            if config.candidates.len() > 1 {
                route.policy.candidates = topology::CandidatePolicy::SkipUnrepresentable;
                route.policy.fallback = topology::FallbackPolicy::BeforeCommit;
            }
            if let Some(attempts) = config.max_attempts {
                route.policy.max_attempts = attempts;
            }
            route.policy.total_timeout_ms = config.total_timeout_ms.map(timeout).transpose()?;
            let table = config
                .thinking_tokens
                .as_ref()
                .map_or(SHARED_BUDGETS, ThinkingTokens::table);
            table.validate().map_err(|_| invalid())?;
            let single = config.candidates.len() == 1;
            let mut deployments = BTreeSet::new();
            let mut native_owner_cache = true;
            let mut native_agent_profile = true;
            let mut native_tool_examples = true;
            let mut native_document_context = true;
            let mut native_automatic_cache = true;
            for (candidate_index, candidate) in config.candidates.into_iter().enumerate() {
                let binding = EndpointId::new(&candidate.binding).map_err(|_| invalid())?;
                if !template_route.endpoints.contains(&binding) {
                    return Err(StartupError::Binding);
                }
                let mut endpoint = catalog
                    .endpoint(&binding)
                    .ok_or(StartupError::Binding)?
                    .clone();
                let mut provider = catalog
                    .endpoint_provider(&endpoint)
                    .ok_or(StartupError::Binding)?
                    .clone();
                let credential = candidate
                    .credential
                    .unwrap_or_else(|| endpoint.credential.as_str().to_owned());
                let credential_id = CredentialBindingId::new(&format!(
                    "configured-{model_index}-{candidate_index}"
                ))
                .map_err(|_| invalid())?;
                bind_pool(provider.id.as_str(), &credential, credential_id.clone())?;
                endpoint.credential = credential_id;
                endpoint.id =
                    EndpointId::new(&format!("configured-{model_index}-{candidate_index}"))
                        .map_err(|_| invalid())?;
                if let Some(model) = candidate.upstream_model {
                    endpoint.upstream_model = model;
                }
                if let Some(base_url) = candidate.base_url {
                    deploy(&mut endpoint, &mut provider, &base_url)?;
                }
                if endpoint.protocol == ProtocolProfile::GoogleGenerateContent {
                    if endpoint.upstream_model.is_empty()
                        || !endpoint
                            .upstream_model
                            .bytes()
                            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
                    {
                        return Err(invalid());
                    }
                    let entry = provider
                        .generation_entry(endpoint.protocol.operation())
                        .ok_or(StartupError::Binding)?;
                    endpoint.target.path =
                        endpoint.declared_path(entry.path).map_err(|_| invalid())?;
                }
                if let Some(ms) = candidate.attempt_timeout_ms {
                    endpoint.execution.timeout_ms = timeout(ms)?;
                }
                if !deployments.insert((
                    provider.id.as_str().to_owned(),
                    endpoint.target.origin.as_str().to_owned(),
                    endpoint.target.path.as_str().to_owned(),
                    endpoint.upstream_model.clone(),
                    credential,
                )) {
                    return Err(StartupError::Binding);
                }
                if single
                    && endpoint.execution.credential_kind
                        == crate::provider::CredentialKind::OAuth("openai")
                {
                    route.policy.fallback = topology::FallbackPolicy::Disabled;
                    if config.max_attempts.is_none() {
                        route.policy.max_attempts = 1;
                    }
                }
                reasoning(
                    &mut endpoint,
                    candidate.reasoning_as_text,
                    candidate.thinking_conversion,
                    &table,
                )?;
                let limits = crate::protocol::model_limits::ModelLimits {
                    allowed_effort: candidate
                        .allowed_effort
                        .map(|levels| {
                            if levels.is_empty() || levels.len() > 6 {
                                return Err(invalid());
                            }
                            levels
                                .into_iter()
                                .map(|level| level.parse().map_err(|_| invalid()))
                                .collect::<Result<Box<[_]>, _>>()
                        })
                        .transpose()?,
                    maximum_output_tokens: candidate.maximum_output_tokens,
                };
                *endpoint.representation.model_limits_mut() = endpoint
                    .representation
                    .model_limits()
                    .narrowed(limits)
                    .map_err(|_| invalid())?;
                match (
                    &mut endpoint.representation,
                    candidate.default_output_tokens,
                ) {
                    (TargetCaps::AnthropicMessages(caps), configured) => {
                        use crate::semantic::value::Presence;
                        match configured {
                            Presence::Absent => {}
                            Presence::Null => caps.default_output_tokens = None,
                            Presence::Value(value) => caps.default_output_tokens = Some(value),
                        }
                        caps.validate().map_err(|_| invalid())?;
                    }
                    (_, crate::semantic::value::Presence::Absent) => {}
                    _ => return Err(invalid()),
                }
                let native = endpoint.protocol == ProtocolProfile::AnthropicMessages;
                let semantics = endpoint.representation.semantics();
                native_owner_cache &= native && semantics.explicit_cache;
                native_tool_examples &= native && semantics.tool_examples;
                native_document_context &= native && semantics.document_context;
                native_automatic_cache &= native && semantics.automatic_prefix_cache;
                native_agent_profile &= native
                    && semantics.strict_tools
                    && semantics.ordered_instructions
                    && semantics.tool_changes;
                route.endpoints.push(endpoint.id.clone());
                // All public Generation client protocols use the same explicit route.
                entries
                    .generation
                    .extend([Profile::Chat, Profile::Responses].map(|protocol| Entry {
                        model: label.clone(),
                        protocol,
                        chat_profile: if protocol == Profile::Chat {
                            config.chat_profile.unwrap_or_default()
                        } else {
                            ChatProfile::Standard
                        },
                        endpoint: endpoint.id.clone(),
                    }));
                endpoints.push((endpoint, provider));
            }
            routes.push(route);
            let mut public = public.clone();
            // Publish only the selected all-native contract, never a union that
            // admits cache-bearing requests by skipping incompatible candidates.
            public.contract.explicit_cache |= native_owner_cache;
            public.contract.tool_examples |= native_tool_examples;
            public.contract.document_context |= native_document_context;
            public.contract.automatic_prefix_cache |= native_automatic_cache;
            // pi's built-in Messages profile: strict tools, mid-conversation system
            // messages and tool changes, under the same all-native rule.
            if native_agent_profile {
                public.contract.strict_tools = true;
                public.contract.ordered_instructions = true;
                public.contract.tool_changes = true;
            }
            public_models.push(public);
        }
        let topology = catalog
            .with_generation(endpoints, routes, public_models)
            .map_err(|_| StartupError::Binding)?;
        Ok(Plan {
            topology,
            entries,
            credentials,
        })
    }
}

fn deploy(
    endpoint: &mut Endpoint,
    provider: &mut ProviderDefinition,
    base: &str,
) -> Result<(), StartupError> {
    let (scheme, authority_path) = base.split_once("://").ok_or_else(invalid)?;
    let (authority, path) = authority_path
        .split_once('/')
        .unwrap_or((authority_path, ""));
    if path.starts_with('/')
        || path.contains(['?', '#', '%', '\\'])
        || path
            .bytes()
            .any(|b| b.is_ascii_control() || b.is_ascii_whitespace())
    {
        return Err(invalid());
    }
    let origin = TrustedOrigin::parse(&format!("{scheme}://{authority}")).map_err(|_| invalid())?;
    let prefix = path.strip_suffix('/').unwrap_or(path);
    let suffix = match endpoint.protocol {
        ProtocolProfile::OpenAiChat => "chat/completions",
        ProtocolProfile::OpenAiResponses => "responses",
        ProtocolProfile::AnthropicMessages => "messages",
        ProtocolProfile::GoogleGenerateContent => "models",
        ProtocolProfile::GoogleInteractions => "interactions",
    };
    let path = EndpointPath::new(&if prefix.is_empty() {
        format!("/{suffix}")
    } else {
        format!("/{prefix}/{suffix}")
    })
    .map_err(|_| invalid())?;
    provider.origin = origin.clone();
    provider
        .endpoints
        .get_mut(&endpoint.protocol.operation().into())
        .ok_or(StartupError::Binding)?
        .path = path.clone();
    endpoint.target.origin = origin;
    endpoint.target.path = path;
    Ok(())
}

fn reasoning(
    endpoint: &mut Endpoint,
    as_text: bool,
    conversion: Option<ThinkingConversion>,
    table: &ThinkingBudgets,
) -> Result<(), StartupError> {
    let policy = conversion.map(|conversion| {
        let (rule, conversion) = match conversion {
            ThinkingConversion::BudgetToEffort => (
                "configured.budget-to-effort.v1",
                ReasoningConversion::BudgetToEffort {
                    scope: if endpoint.protocol == ProtocolProfile::AnthropicMessages {
                        EffortScope::Response
                    } else {
                        EffortScope::Reasoning
                    },
                    table: table.clone(),
                },
            ),
            ThinkingConversion::EffortToBudget => (
                "configured.effort-to-budget.v1",
                ReasoningConversion::EffortToBudget {
                    scope: EffortScope::Reasoning,
                    table: table.clone(),
                },
            ),
        };
        ReasoningRequestPolicy {
            canonical_model: endpoint.canonical_model.as_str().to_owned().into(),
            rule,
            conversion,
        }
    });
    if matches!(conversion, Some(ThinkingConversion::EffortToBudget))
        && endpoint.protocol != ProtocolProfile::GoogleGenerateContent
    {
        return Err(invalid());
    }
    match &mut endpoint.representation {
        TargetCaps::OpenAi(caps) => {
            if policy.is_some() && caps.adaptation.effort_mapping.is_some() {
                return Err(invalid());
            }
            caps.reasoning_as_text = as_text;
            caps.adaptation.reasoning_policy = policy;
        }
        TargetCaps::AnthropicMessages(caps) => {
            caps.reasoning_as_text = as_text;
            caps.reasoning_policy = policy;
        }
        TargetCaps::GoogleGenerateContent(caps) => {
            caps.reasoning_as_text = as_text;
            if policy.is_some() {
                caps.reasoning_policy = policy;
            }
        }
        TargetCaps::GoogleInteractions(caps) => {
            caps.reasoning_as_text = as_text;
            if policy.is_some() {
                caps.reasoning_policy = policy;
            }
        }
    }
    Ok(())
}
