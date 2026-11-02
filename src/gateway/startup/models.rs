//! Explicit model contracts over account-bound catalog targets.
use super::{
    entries::{
        ChatProfile, EmbeddingEntry, Entry, ImageEntry, SpeechEntry, TaskEntries,
        TranscriptionEntry,
    },
    error::StartupError,
};
use crate::{
    account::{AccountCredential, BoundAccounts},
    gateway::Credentials,
    lowering::target::TargetContract,
    protocol::{
        ClientProtocol,
        reasoning::{ReasoningConversion, ReasoningRequestPolicy, SHARED_BUDGETS, ThinkingBudgets},
    },
    semantic::task::generation::{EffortLevel, EffortScope},
    topology::{self, CompiledTopology, Endpoint, EndpointId, GenerationProtocol},
};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Models {
    version: u32,
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
    account: Option<String>,
    #[serde(default)]
    candidates: Vec<Candidate>,
    total_timeout_ms: Option<u64>,
    thinking_tokens: Option<ThinkingTokens>,
    chat_profile: Option<ChatProfile>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Candidate {
    binding: String,
    allowed_effort: Option<Vec<String>>,
    context_window_tokens: Option<u64>,
    maximum_input_tokens: Option<u64>,
    maximum_output_tokens: Option<u64>,
    #[serde(default)]
    default_output_tokens: crate::semantic::value::Presence<u64>,
    upstream_model: Option<String>,
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

impl Models {
    pub fn bind(self, accounts: &BoundAccounts) -> Result<Plan, StartupError> {
        if self.version != 1 || self.models.is_empty() || self.models.len() > 64 {
            return Err(invalid());
        }
        let catalog = topology::catalog::default_topology().map_err(|_| StartupError::Binding)?;
        let mut credentials = Credentials::from(accounts);
        let mut entries = TaskEntries::default();
        let mut endpoints = Vec::new();
        let mut routes = Vec::new();
        let mut public_models = Vec::new();
        for (model_index, (label, config)) in self.models.into_iter().enumerate() {
            let Some(public) = catalog.model(&label) else {
                if !config.candidates.is_empty()
                    || config.total_timeout_ms.is_some()
                    || config.thinking_tokens.is_some()
                    || config.chat_profile.is_some()
                {
                    return Err(invalid());
                }
                let name = config.account.as_deref().ok_or_else(invalid)?;
                let provider = if let Some(route) = catalog.image_route(&label) {
                    if config.binding.is_some() {
                        return Err(invalid());
                    }
                    entries.images.push(ImageEntry {
                        model: label.clone(),
                    });
                    &route.endpoint.provider
                } else if let Some(route) = catalog.speech_route(&label) {
                    if config.binding.is_some() {
                        return Err(invalid());
                    }
                    entries.speech.push(SpeechEntry {
                        model: label.clone(),
                    });
                    &route.endpoint.provider
                } else if catalog.transcription_model(&label).is_some() {
                    let route = match config.binding.as_deref() {
                        Some(binding) => catalog.transcription_binding(
                            &label,
                            &EndpointId::new(binding).map_err(|_| invalid())?,
                        ),
                        None => catalog.transcription_route(&label),
                    }
                    .ok_or(StartupError::Binding)?;
                    entries.transcriptions.push(TranscriptionEntry {
                        model: label.clone(),
                        binding: Some(route.endpoint.id.clone()),
                    });
                    &route.endpoint.provider
                } else if let Some(route) = catalog.embedding_route(&label) {
                    if config.binding.is_some() {
                        return Err(invalid());
                    }
                    entries.embeddings.push(EmbeddingEntry {
                        model: label.clone(),
                    });
                    &route.endpoint.provider
                } else {
                    return Err(StartupError::Binding);
                };
                let account = accounts
                    .account(provider.as_str(), name)
                    .ok_or(StartupError::Credentials)?;
                if !account.enabled || !matches!(account.credential, AccountCredential::ApiKey(_)) {
                    return Err(StartupError::Credentials);
                }
                credentials.select_task(label, account.binding_id());
                continue;
            };
            if config.binding.is_some()
                || config.account.is_some()
                || config.candidates.is_empty()
                || config.candidates.len() > 64
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
            route.policy.candidates = topology::CandidatePolicy::SkipUnrepresentable;
            // The account cutover never dispatches another target after failure.
            // The bounded same-target recovery executor is integrated separately.
            route.policy.fallback = topology::FallbackPolicy::Disabled;
            route.policy.max_attempts = 1;
            route.policy.total_timeout_ms = config.total_timeout_ms;
            let table = config
                .thinking_tokens
                .as_ref()
                .map_or(SHARED_BUDGETS, ThinkingTokens::table);
            table.validate().map_err(|_| invalid())?;
            let mut deployments = BTreeSet::new();
            for (candidate_index, candidate) in config.candidates.into_iter().enumerate() {
                let binding = EndpointId::new(&candidate.binding).map_err(|_| invalid())?;
                if !template_route.endpoints.contains(&binding) {
                    return Err(StartupError::Binding);
                }
                let template = catalog.endpoint(&binding).ok_or(StartupError::Binding)?;
                let configured = accounts.provider(template.provider.as_str());
                if configured.is_empty() {
                    return Err(StartupError::Credentials);
                }
                for (account_index, account) in
                    configured.iter().enumerate().filter(|(_, a)| a.enabled)
                {
                    if route.endpoints.len() >= crate::execution::plan::MAX_CANDIDATES {
                        return Err(StartupError::Binding);
                    }
                    let mut endpoint = template.clone();
                    let provider = account.provider.clone();
                    let operation = provider
                        .generation_entry(endpoint.protocol.operation())
                        .ok_or(StartupError::Binding)?;
                    endpoint.credential = account.binding_id();
                    endpoint.id = EndpointId::new(&format!(
                        "configured-{model_index}-{candidate_index}-{account_index}"
                    ))
                    .map_err(|_| invalid())?;
                    endpoint.target.origin = provider.origin.clone();
                    if let Some(model) = &candidate.upstream_model {
                        endpoint.upstream_model = model.clone();
                    }
                    endpoint.target.path = endpoint
                        .declared_path(operation.path)
                        .map_err(|_| invalid())?;
                    if endpoint.target != template.target
                        || endpoint.upstream_model != template.upstream_model
                    {
                        let limits = endpoint.representation.model_limits_mut();
                        limits.context_window_tokens = None;
                        limits.maximum_input_tokens = None;
                    }
                    if !deployments.insert((
                        provider.id.as_str().to_owned(),
                        endpoint.target.origin.as_str().to_owned(),
                        endpoint.target.path.as_str().to_owned(),
                        endpoint.upstream_model.clone(),
                        credentials
                            .get(&endpoint.credential)
                            .expect("enabled account binding")
                            .source
                            .provenance(),
                    )) {
                        return Err(StartupError::Binding);
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
                            .as_ref()
                            .map(|levels| {
                                if levels.is_empty() || levels.len() > 6 {
                                    return Err(invalid());
                                }
                                levels
                                    .iter()
                                    .map(|level| level.parse().map_err(|_| invalid()))
                                    .collect::<Result<Box<[_]>, _>>()
                            })
                            .transpose()?,
                        context_window_tokens: candidate.context_window_tokens,
                        maximum_input_tokens: candidate.maximum_input_tokens,
                        maximum_output_tokens: candidate.maximum_output_tokens,
                    };
                    *endpoint.representation.model_limits_mut() = endpoint
                        .representation
                        .model_limits()
                        .narrowed(limits)
                        .map_err(|_| invalid())?;
                    match (
                        &mut endpoint.representation,
                        &candidate.default_output_tokens,
                    ) {
                        (TargetContract::AnthropicMessages(caps), configured) => {
                            use crate::semantic::value::Presence;
                            match configured {
                                Presence::Absent => {}
                                Presence::Null => caps.default_output_tokens = None,
                                Presence::Value(value) => caps.default_output_tokens = Some(*value),
                            }
                            caps.validate().map_err(|_| invalid())?;
                        }
                        (_, crate::semantic::value::Presence::Absent) => {}
                        _ => return Err(invalid()),
                    }
                    route.endpoints.push(endpoint.id.clone());
                    entries
                        .generation
                        .extend(ClientProtocol::ALL.map(|protocol| Entry {
                            model: label.clone(),
                            protocol,
                            chat_profile: if protocol == ClientProtocol::Chat {
                                config.chat_profile.unwrap_or_default()
                            } else {
                                ChatProfile::Standard
                            },
                            endpoint: endpoint.id.clone(),
                        }));
                    endpoints.push((endpoint, provider));
                }
            }
            if route.endpoints.is_empty() {
                return Err(StartupError::Credentials);
            }
            routes.push(route);
            public_models.push(public.clone());
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
                    scope: if endpoint.protocol == GenerationProtocol::AnthropicMessages {
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
        && endpoint.protocol != GenerationProtocol::GoogleGenerateContent
    {
        return Err(invalid());
    }
    match &mut endpoint.representation {
        TargetContract::OpenAi(caps) => {
            if policy.is_some() && caps.adaptation.effort_mapping.is_some() {
                return Err(invalid());
            }
            caps.reasoning_as_text = as_text;
            caps.adaptation.reasoning_policy = policy;
        }
        TargetContract::AnthropicMessages(caps) => {
            caps.reasoning_as_text = as_text;
            caps.reasoning_policy = policy;
        }
        TargetContract::GoogleGenerateContent(caps) => {
            caps.reasoning_as_text = as_text;
            if policy.is_some() {
                caps.reasoning_policy = policy;
            }
        }
        TargetContract::GoogleInteractions(caps) => {
            caps.reasoning_as_text = as_text;
            if policy.is_some() {
                caps.reasoning_policy = policy;
            }
        }
    }
    Ok(())
}
