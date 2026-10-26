//! Finite source-domain declarations checked by the real admission and target projectors.
//! A declaration covers the documented simple domain, not arbitrary schemas or combinations.
use crate::{
    execution,
    gateway::BoundEntry,
    protocol::{
        ClientProtocol, GenerationRequestEnvelope,
        conversion::generation::{Change, Field},
    },
};
use serde::Serialize;
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum State {
    Faithful,
    ApprovedAdaptation,
    Unavailable,
    NonUniform,
    NotAssessed,
}
#[derive(Clone, Eq, PartialEq, Serialize)]
pub(super) struct Capability {
    state: State,
    #[serde(skip_serializing_if = "BTreeSet::is_empty")]
    rules: BTreeSet<&'static str>,
}
impl Capability {
    fn new(state: State) -> Self {
        Self {
            state,
            rules: BTreeSet::new(),
        }
    }
    pub(super) fn positive(&self) -> bool {
        matches!(self.state, State::Faithful | State::ApprovedAdaptation)
    }
    fn common(values: impl IntoIterator<Item = Self>) -> Self {
        let values: Vec<_> = values.into_iter().collect();
        if values.is_empty() || values.iter().any(|v| v.state == State::NotAssessed) {
            return Self::new(State::NotAssessed);
        }
        if values.iter().all(Self::positive) {
            let rules = values
                .into_iter()
                .flat_map(|v| v.rules)
                .collect::<BTreeSet<_>>();
            Self {
                state: if rules.is_empty() {
                    State::Faithful
                } else {
                    State::ApprovedAdaptation
                },
                rules,
            }
        } else if values.iter().all(|v| v.state == State::Unavailable) {
            Self::new(State::Unavailable)
        } else {
            Self::new(State::NonUniform)
        }
    }
}
#[derive(Serialize)]
struct Choices {
    #[serde(flatten)]
    capability: Capability,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    values: Vec<&'static str>,
}
impl Choices {
    fn new(choices: impl IntoIterator<Item = (&'static str, Capability)>) -> Self {
        let (positive, negative): (Vec<_>, Vec<_>) =
            choices.into_iter().partition(|(_, c)| c.positive());
        if positive.is_empty() {
            Self {
                capability: Capability::common(negative.into_iter().map(|(_, c)| c)),
                values: vec![],
            }
        } else {
            let (values, capabilities): (Vec<_>, Vec<_>) = positive.into_iter().unzip();
            Self {
                capability: Capability::common(capabilities),
                values,
            }
        }
    }
}
#[derive(Serialize)]
pub(super) struct Interface {
    task: &'static str,
    protocol: &'static str,
    delivery_modes: Vec<&'static str>,
    generation: Generation,
}
#[derive(Serialize)]
struct Generation {
    scope: &'static str,
    composition: &'static str,
    input: super::media::Input,
    output: super::media::Output,
    token_limits: TokenLimits,
    features: BTreeMap<&'static str, Capability>,
    reasoning: Reasoning,
}
#[derive(Serialize)]
struct TokenLimits {
    purpose: &'static str,
    context_window_tokens: Option<u64>,
    max_input_tokens: Option<u64>,
    max_output_tokens: Option<u64>,
    output_token_accounting: &'static str,
}
#[derive(Serialize)]
struct Reasoning {
    activation: Choices,
    effort: BTreeMap<&'static str, Choices>,
}
impl Interface {
    pub(super) fn from_bound(entry: &BoundEntry) -> Self {
        let streaming = entry
            .candidates
            .iter()
            .all(|c| c.endpoint.execution.streaming);
        let delivery_modes = if streaming {
            vec!["json", "sse"]
        } else {
            vec!["json"]
        };
        let assess = |case| assess(entry, &delivery_modes, case);
        let mut features = BTreeMap::new();
        for (name, case) in [
            ("function_tools", Case::Tools(false)),
            ("strict_tool_schema", Case::Tools(true)),
            ("parallel_tool_calls", Case::Parallel),
            ("json_object_output", Case::JsonObject),
            ("json_schema_output", Case::JsonSchema),
            ("max_output_tokens", Case::MaximumOutput),
        ] {
            features.insert(name, assess(case));
        }
        let effort_scope = if entry.client.protocol() == ClientProtocol::Messages {
            "response"
        } else {
            "reasoning"
        };
        let effort = ["response", "reasoning"]
            .into_iter()
            .map(|scope| {
                let values = Choices::new(
                    ["minimal", "low", "medium", "high", "xhigh", "max"].map(|level| {
                        (
                            level,
                            if scope == effort_scope {
                                assess(Case::Effort(level))
                            } else {
                                Capability::new(State::Unavailable)
                            },
                        )
                    }),
                );
                (scope, values)
            })
            .collect();
        let activation = Choices::new(
            ["enabled", "disabled", "adaptive"].map(|mode| (mode, assess(Case::Activation(mode)))),
        );
        // Only complete comparable declarations produce a route-common planning ceiling.
        let common_limit =
            |select: fn(&crate::protocol::model_limits::ModelLimits) -> Option<u64>| {
                entry
                    .candidates
                    .iter()
                    .try_fold(u64::MAX, |minimum, candidate| {
                        select(candidate.endpoint.representation.model_limits())
                            .filter(|n| (1..=9_007_199_254_740_991).contains(n))
                            .map(|n| minimum.min(n))
                    })
            };
        let token_limits = TokenLimits {
            purpose: "planning",
            context_window_tokens: common_limit(|limits| limits.context_window_tokens),
            max_input_tokens: common_limit(|limits| limits.maximum_input_tokens),
            max_output_tokens: common_limit(|limits| limits.maximum_output_tokens),
            output_token_accounting: "not_assessed",
        };
        let input = super::media::input(entry, &delivery_modes);
        let output = super::media::Output {
            text: assess(Case::Text),
        };
        Self {
            task: "generation",
            protocol: match entry.client.protocol() {
                ClientProtocol::Chat => "chat_completions",
                ClientProtocol::Responses => "responses",
                ClientProtocol::Messages => "messages",
            },
            delivery_modes,
            generation: Generation {
                scope: "route_common",
                composition: "not_asserted",
                input,
                output,
                token_limits,
                features,
                reasoning: Reasoning { activation, effort },
            },
        }
    }
}

#[derive(Clone, Copy)]
pub(super) enum Case {
    Text,
    Image(super::media::ImageCase),
    Tools(bool),
    Parallel,
    JsonObject,
    JsonSchema,
    MaximumOutput,
    Activation(&'static str),
    Effort(&'static str),
}
impl Case {
    fn protected_field(self) -> Option<Field> {
        match self {
            Self::MaximumOutput => Some(Field::MaximumOutput),
            Self::Activation(_) | Self::Effort(_) => Some(Field::Effort),
            Self::Tools(_) => Some(Field::Tools),
            _ => None,
        }
    }
    fn request(self, entry: &BoundEntry, stream: bool) -> Option<GenerationRequestEnvelope> {
        let protocol = entry.client.protocol();
        // Messages requires an output cap even for a plain text request.
        let cap = entry
            .candidates
            .iter()
            .filter_map(|c| {
                c.endpoint
                    .representation
                    .model_limits()
                    .maximum_output_tokens
            })
            .min()
            .unwrap_or(4096)
            .min(4096);
        let mut value = match protocol {
            ClientProtocol::Chat => json!({"messages":[{"role":"user","content":"text"}]}),
            ClientProtocol::Responses => json!({"input":"text"}),
            ClientProtocol::Messages => {
                json!({"messages":[{"role":"user","content":"text"}],"max_tokens":cap})
            }
        };
        value["model"] = json!(entry.public.id.as_str());
        value["stream"] = json!(stream);
        // A small closed object is a common declaration domain, not all JSON Schema.
        let schema = json!({"type":"object","properties":{"value":{"type":"string"}},"required":["value"],"additionalProperties":false});
        match self {
            Self::Text => {}
            Self::Image(image) => image.apply(&mut value, protocol)?,
            Self::Tools(_) | Self::Parallel => {
                let strict = matches!(self, Self::Tools(true));
                let function = json!({"name":"lookup","description":"Look up a value","parameters":schema,"strict":strict});
                value["tools"] = match protocol {
                    ClientProtocol::Chat => json!([{"type":"function","function":function}]),
                    ClientProtocol::Responses => {
                        json!([{"type":"function","name":"lookup","description":"Look up a value","parameters":schema,"strict":strict}])
                    }
                    ClientProtocol::Messages => {
                        json!([{"name":"lookup","description":"Look up a value","input_schema":schema,"strict":strict}])
                    }
                };
                if matches!(self, Self::Parallel) {
                    match protocol {
                        ClientProtocol::Messages => {
                            value["tool_choice"] =
                                json!({"type":"auto","disable_parallel_tool_use":false})
                        }
                        _ => value["parallel_tool_calls"] = json!(true),
                    }
                }
            }
            Self::JsonObject | Self::JsonSchema => {
                let format = if matches!(self, Self::JsonObject) {
                    json!({"type":"json_object"})
                } else {
                    match protocol {
                        ClientProtocol::Chat => {
                            json!({"type":"json_schema","json_schema":{"name":"result","schema":schema,"strict":true}})
                        }
                        ClientProtocol::Responses => {
                            json!({"type":"json_schema","name":"result","schema":schema,"strict":true})
                        }
                        ClientProtocol::Messages => json!({"type":"json_schema","schema":schema}),
                    }
                };
                match protocol {
                    ClientProtocol::Chat => value["response_format"] = format,
                    ClientProtocol::Responses => value["text"] = json!({"format":format}),
                    ClientProtocol::Messages if matches!(self, Self::JsonSchema) => {
                        value["output_config"] = json!({"format":format})
                    }
                    ClientProtocol::Messages => return None,
                }
            }
            Self::MaximumOutput => {
                let field = match protocol {
                    ClientProtocol::Chat => "max_completion_tokens",
                    ClientProtocol::Responses => "max_output_tokens",
                    ClientProtocol::Messages => "max_tokens",
                };
                value[field] = json!(cap);
            }
            Self::Effort(level) => match protocol {
                ClientProtocol::Chat => value["reasoning_effort"] = json!(level),
                ClientProtocol::Responses => value["reasoning"] = json!({"effort":level}),
                ClientProtocol::Messages => value["output_config"] = json!({"effort":level}),
            },
            Self::Activation(mode) => match protocol {
                ClientProtocol::Messages => {
                    value["thinking"] = if mode == "enabled" {
                        json!({"type":"enabled","budget_tokens":1024})
                    } else {
                        json!({"type":mode})
                    }
                }
                ClientProtocol::Chat if mode == "disabled" => {
                    value["reasoning_effort"] = json!("none")
                }
                ClientProtocol::Responses if mode == "disabled" => {
                    value["reasoning"] = json!({"effort":"none"})
                }
                _ => return None,
            },
        }
        let mut request = entry
            .client
            .decode_client_request(&serde_json::to_vec(&value).ok()?)
            .ok()?;
        // Normal ingress always supplies request identity before candidate preflight.
        // This local placeholder creates neither a runtime session nor an affinity entry.
        request.conversation = Some(
            crate::semantic::context::ConversationContext::independent_request("text-discovery")
                .ok()?,
        );
        Some(request)
    }
}

pub(super) fn assess(entry: &BoundEntry, modes: &[&str], case: Case) -> Capability {
    Capability::common(modes.iter().flat_map(|mode| {
        let stream = *mode == "sse";
        let base = Case::Text.request(entry, stream);
        let request = case.request(entry, stream);
        entry.candidates.iter().map(move |candidate| {
            let Some(base) = &base else {
                return Capability::new(State::NotAssessed);
            };
            // A blocked plain request must not make every unrelated feature look unavailable.
            if execution::admit(&entry.public, base).is_err()
                || execution::plan::representable_for_client(
                    &candidate.endpoint,
                    base,
                    Some(&entry.client),
                )
                .is_err()
            {
                return Capability::new(State::NotAssessed);
            }
            let Some(request) = &request else {
                return Capability::new(State::Unavailable);
            };
            if execution::admit(&entry.public, request).is_err()
                || execution::plan::representable_for_client(
                    &candidate.endpoint,
                    request,
                    Some(&entry.client),
                )
                .is_err()
            {
                return Capability::new(State::Unavailable);
            }
            let Ok(projection) = candidate.endpoint.encode_request_with_evidence(request) else {
                return Capability::new(State::NotAssessed);
            };
            if matches!(case, Case::Activation(_) | Case::Effort(_)) {
                let Ok(baseline) = candidate.endpoint.encode_request(base) else {
                    return Capability::new(State::NotAssessed);
                };
                if baseline == projection.value {
                    return Capability::new(State::Unavailable);
                }
            }
            let mut rules = BTreeSet::new();
            for effect in projection.evidence.iter().filter(|e| e.change().is_loss()) {
                // Dropped control intent is not effective control support. Parallel permission
                // is different: the existing named omission preserves always-enabled permission.
                if effect.change() == Change::Omit
                    && (Some(effect.semantic_field()) == case.protected_field()
                        || matches!(case, Case::Activation(_) | Case::Effort(_))
                            && effect.semantic_field() == Field::ReasoningAllocation)
                {
                    return Capability::new(State::Unavailable);
                }
                rules.insert(effect.rule());
            }
            Capability {
                state: if rules.is_empty() {
                    State::Faithful
                } else {
                    State::ApprovedAdaptation
                },
                rules,
            }
        })
    }))
}
