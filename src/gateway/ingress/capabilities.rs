//! Partial gateway-boundary declarations, not model-effectiveness or full-loop promises.
use crate::{
    adapter::{ClientAdapter, ClientProtocol},
    gateway::{ApiError, BoundEntry, Runtime, StartupError},
    lowering::generation::ReportedFactPolicy,
    protocol::openai::lowering::projection::LossRule,
};
use axum::{body::Body, extract::State, http::HeaderValue, response::Response};
use bytes::Bytes;
use serde::Serialize;
use std::{collections::BTreeMap, sync::Arc};

const MAX_BYTES: usize = 256 << 10;
const LINK: &str = "</v1/morphiecore/capabilities>; rel=\"describedby\"";

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
enum Treatment {
    Faithful,
    ApprovedAdaptation,
    Rejected,
}
#[derive(Serialize)]
struct Case {
    feature: &'static str,
    domain: &'static str,
    treatment: Treatment,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    rules: Vec<&'static str>,
}
impl Case {
    fn new(feature: &'static str, domain: &'static str, treatment: Treatment) -> Self {
        Self {
            feature,
            domain,
            treatment,
            rules: vec![],
        }
    }
}
#[derive(Serialize)]
struct Entry<'a> {
    model: &'a str,
    protocol: &'static str,
    delivery_modes: Vec<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    embedding: Option<EmbeddingContract<'a>>,
    request: Vec<Case>,
    delivery: Vec<Case>,
    history: Vec<Case>,
}
#[derive(Serialize)]
struct EmbeddingContract<'a> {
    input_kinds: Vec<&'static str>,
    tokenizer: Option<crate::semantic::task::embedding::TokenizerScope>,
    max_input_tokens: Option<usize>,
    max_token_ids: usize,
    usage_may_be_estimated: bool,
    default_dimensions: usize,
    dimensions: &'a crate::lowering::embeddings::DimensionSupport,
    max_inputs: usize,
    max_inputs_at_default_dimensions: usize,
    max_vector_values: usize,
    max_text_bytes: usize,
    upstream_encodings: crate::lowering::embeddings::EncodingSupport,
    output_encodings: [&'static str; 2],
}
impl<'a> Entry<'a> {
    fn from_embedding(
        model: &'a str,
        entry: &'a crate::gateway::tasks::embeddings::BoundEmbedding,
    ) -> Self {
        use crate::{
            adapter::embeddings::*,
            lowering::embeddings::{ReportPolicy, Rule, UserPolicy},
        };
        let contract = &entry.route.endpoint.contract;
        let mut rounding = Case::new(
            "embedding.encoding",
            "float_json_to_binary32",
            Treatment::ApprovedAdaptation,
        );
        rounding.rules.push(Rule::RoundBinary32.name());
        let mut identifier = Case::new(
            "embedding.response_id",
            "ancillary_report",
            Treatment::ApprovedAdaptation,
        );
        identifier.rules.push(Rule::OmitResponseId.name());
        let compatible = contract.reports == ReportPolicy::Compatible;
        let mut user = Case::new(
            "embedding.user",
            "upstream_identity_hint",
            match contract.user {
                UserPolicy::Forward => Treatment::Faithful,
                UserPolicy::Omit => Treatment::ApprovedAdaptation,
                UserPolicy::Reject => Treatment::Rejected,
            },
        );
        if contract.user == UserPolicy::Omit {
            user.rules.push(Rule::OmitUser.name());
        }
        let mut reports = Case::new(
            "embedding.reports",
            "source_accounting_and_model_presence",
            if compatible {
                Treatment::ApprovedAdaptation
            } else {
                Treatment::Faithful
            },
        );
        if compatible {
            reports.rules.extend(
                [
                    Rule::BoundModel,
                    Rule::DeriveTotal,
                    Rule::InputFromTotal,
                    Rule::EstimateUsage,
                    Rule::OmitDetails,
                ]
                .map(Rule::name),
            );
        }
        Self {
            model,
            protocol: "embeddings",
            delivery_modes: vec!["json"],
            embedding: Some(EmbeddingContract {
                input_kinds: if contract.tokenizer.is_some() {
                    vec!["text", "token_ids"]
                } else {
                    vec!["text"]
                },
                tokenizer: contract.tokenizer,
                max_input_tokens: contract.max_input_tokens,
                max_token_ids: MAX_TOKEN_IDS,
                usage_may_be_estimated: compatible,
                default_dimensions: contract.default_dimensions,
                dimensions: &contract.dimensions,
                max_inputs: contract.max_inputs,
                max_inputs_at_default_dimensions: contract
                    .max_inputs
                    .min(MAX_VECTOR_VALUES / contract.default_dimensions),
                max_vector_values: MAX_VECTOR_VALUES,
                max_text_bytes: MAX_INPUT_BYTES,
                upstream_encodings: contract.encodings,
                output_encodings: ["float", "base64"],
            }),
            request: vec![
                Case::new(
                    "embedding.input",
                    "token_ids",
                    if contract.tokenizer.is_some() {
                        Treatment::Faithful
                    } else {
                        Treatment::Rejected
                    },
                ),
                user,
            ],
            delivery: vec![rounding, identifier, reports],
            history: vec![],
        }
    }
    fn from_bound(entry: &'a BoundEntry) -> Self {
        let protocol = entry.client.protocol();
        let mut request = vec![];
        // These are explicit public rejections, not an inventory of codec carriers.
        for (admitted, feature, domain) in [
            (
                entry.public.contract.safety,
                "generation.safety",
                "explicit_policy",
            ),
            (
                entry.public.contract.cached_context,
                "generation.cached_context",
                "existing_reference",
            ),
            (
                entry.public.contract.normalization_default_tools,
                "generation.tools",
                "normalization_default_strictness",
            ),
        ] {
            if !admitted {
                request.push(Case::new(feature, domain, Treatment::Rejected));
            }
        }
        if protocol == ClientProtocol::Messages {
            let native_all = |supports: fn(
                &crate::semantic::task::generation::GenerationSemanticContract,
            ) -> bool| {
                entry.candidates.iter().all(|candidate| {
                    candidate.endpoint.protocol
                        == crate::topology::ProtocolProfile::AnthropicMessages
                        && supports(candidate.endpoint.representation.semantics())
                })
            };
            for (admitted, supported, feature, domain) in [
                (
                    entry.public.contract.tool_examples,
                    native_all(|c| c.tool_examples),
                    "generation.tools",
                    "bounded_ordered_input_examples",
                ),
                (
                    entry.public.contract.document_context,
                    native_all(|c| c.document_context),
                    "generation.document_context",
                    "per_use_presence_in_documents_and_tool_results",
                ),
                (
                    entry.public.contract.explicit_cache,
                    native_all(|c| c.explicit_cache),
                    "generation.cache",
                    "valid_owner_local_breakpoints_5m_or_1h",
                ),
                (
                    entry.public.contract.automatic_prefix_cache,
                    native_all(|c| c.automatic_prefix_cache),
                    "generation.cache",
                    "request_wide_automatic_policy",
                ),
            ] {
                if !admitted || supported {
                    request.push(Case::new(
                        feature,
                        domain,
                        if admitted {
                            Treatment::Faithful
                        } else {
                            Treatment::Rejected
                        },
                    ));
                }
            }
        }
        let (settings_domain, settings_rule, settings_admitted) = match &entry.client {
            ClientAdapter::Messages => (
                "incidental_settings_without_safety",
                "messages.omit-incidental-settings.v1",
                true,
            ),
            ClientAdapter::OpenAi { adapter, contract } => (
                if protocol == ClientProtocol::Chat {
                    "incidental_settings_without_safety"
                } else {
                    "independent_reasoning_controls_without_carrier"
                },
                if protocol == ClientProtocol::Chat {
                    LossRule::OmitChatReportedSettings
                } else {
                    LossRule::OmitReportedReasoningControls
                }
                .name(),
                adapter.contract(contract).reported_facts != ReportedFactPolicy::StrictComplete,
            ),
        };
        let mut settings = Case::new(
            "response.reported.settings",
            settings_domain,
            if settings_admitted {
                Treatment::ApprovedAdaptation
            } else {
                Treatment::Rejected
            },
        );
        if settings_admitted {
            settings.rules.push(settings_rule);
        }
        let mut assessments = Case::new(
            "generation.safety",
            "assessment_only_reports",
            Treatment::ApprovedAdaptation,
        );
        assessments
            .rules
            .push(crate::adapter::SAFETY_ASSESSMENT_RULE);
        let mut delivery = vec![
            settings,
            assessments,
            Case::new(
                "generation.safety",
                "blocking_reports_or_settings_echoes",
                Treatment::Rejected,
            ),
        ];
        let mut usage = Case::new(
            "response.delivery.usage",
            "reported_then_derived_then_estimated",
            Treatment::ApprovedAdaptation,
        );
        usage.rules.push("generation.complete-delivery-usage.v1");
        delivery.push(usage);
        if protocol == ClientProtocol::Responses {
            let mut required = Case::new(
                "response.delivery.settings",
                "missing_required_tool_settings",
                Treatment::ApprovedAdaptation,
            );
            required.rules.push("responses.infer-required-settings.v1");
            delivery.push(required);
        }
        use crate::protocol::anthropic::messages::client::GoogleTerminalPolicy as GoogleTerminal;
        let selected_google = |profile| {
            protocol == ClientProtocol::Messages
                && entry.candidates.iter().any(|c| {
                    c.endpoint.representation.google_messages_terminal_profile() == Some(profile)
                })
        };
        let generate_content = selected_google(GoogleTerminal::GenerateContentWithoutStops);
        let interactions = selected_google(GoogleTerminal::InteractionsWithoutStops);
        // General ambiguity remains rejected. Selected Google delivery additionally
        // needs request context, so expose that smaller domain rather than broad support.
        for (domain, exact) in [
            (
                "natural_or_matched_stop_without_tool_continuation",
                protocol == ClientProtocol::Messages,
            ),
            (
                if generate_content || interactions {
                    "ambiguous_stop_outside_selected_google_no_stop_profile"
                } else {
                    "ambiguous_stop_without_tool_continuation"
                },
                protocol == ClientProtocol::Chat,
            ),
        ] {
            let mut case = Case::new(
                "generation.completion_cause",
                domain,
                match (exact, protocol) {
                    (true, _) => Treatment::Faithful,
                    (false, ClientProtocol::Messages) => Treatment::Rejected,
                    (false, _) => Treatment::ApprovedAdaptation,
                },
            );
            match (exact, protocol) {
                (false, ClientProtocol::Chat) => {
                    case.rules.push(LossRule::CoarsenChatCompletionCause.name());
                }
                (false, ClientProtocol::Responses) => {
                    case.rules
                        .push(LossRule::OmitResponsesCompletionCause.name());
                }
                _ => {}
            }
            delivery.push(case);
        }
        for (selected, domain, rules) in [
            (
                generate_content,
                "generate_content_stop_without_configured_sequences",
                &[
                    "messages.google-tool-stop.v1",
                    "messages.google-stop-without-sequences.v1",
                ][..],
            ),
            (
                interactions,
                "interactions_completed_without_configured_sequences",
                &["messages.interactions-completed-as-end-turn.v1"][..],
            ),
        ] {
            if selected {
                let mut case = Case::new(
                    "generation.completion_cause",
                    domain,
                    Treatment::ApprovedAdaptation,
                );
                case.rules.extend_from_slice(rules);
                delivery.push(case);
            }
        }
        let mut history = if protocol == ClientProtocol::Responses {
            vec![Case::new(
                "generation.delivered_projection",
                "completed_stored_response_within_ttl",
                Treatment::Faithful,
            )]
        } else {
            vec![]
        };
        let native_phase = protocol == ClientProtocol::Responses;
        let phase_carrier = entry.client.check_phase_carrier().is_ok();
        if protocol == ClientProtocol::Messages {
            let mut case = Case::new(
                "generation.message.phase",
                "assistant_commentary_or_final_answer",
                Treatment::ApprovedAdaptation,
            );
            case.rules.push("generation.omit-delivery-message-phase.v1");
            delivery.push(case);
            history.push(Case::new(
                "generation.message.phase",
                "returned_assistant_phase",
                Treatment::Rejected,
            ));
        } else {
            delivery.push(Case::new(
                "generation.message.phase",
                if native_phase {
                    "assistant_commentary_or_final_answer"
                } else {
                    "assistant_text_phase_via_bounded_manifest"
                },
                if phase_carrier {
                    Treatment::Faithful
                } else {
                    Treatment::Rejected
                },
            ));
            history.push(Case::new(
                "generation.message.phase",
                if native_phase {
                    "returned_native_assistant_phase"
                } else {
                    "unchanged_returned_phase_manifest"
                },
                if phase_carrier {
                    Treatment::Faithful
                } else {
                    Treatment::Rejected
                },
            ));
        }
        Self {
            model: entry.public.id.as_str(),
            protocol: match protocol {
                ClientProtocol::Chat => "chat_completions",
                ClientProtocol::Responses => "responses",
                ClientProtocol::Messages => "messages",
            },
            delivery_modes: vec!["json", "sse"],
            embedding: None,
            request,
            delivery,
            history,
        }
    }
}

#[derive(Serialize)]
struct Snapshot<'a> {
    object: &'static str,
    version: u32,
    scope: &'static str,
    coverage: &'static str,
    unlisted: &'static str,
    provider_effectiveness: &'static str,
    full_loop: &'static str,
    data: Vec<Entry<'a>>,
}
pub(in crate::gateway) struct CapabilityView(pub(in crate::gateway) Bytes);
impl CapabilityView {
    pub(in crate::gateway) fn bind(
        entries: &BTreeMap<(ClientProtocol, String), Arc<BoundEntry>>,
        embeddings: &BTreeMap<String, Arc<crate::gateway::tasks::embeddings::BoundEmbedding>>,
        response_bytes: usize,
    ) -> Result<Self, StartupError> {
        if entries.len() > 64 * 3 || embeddings.len() > 64 {
            return Err(StartupError::Binding);
        }
        let data = entries
            .values()
            .map(|entry| Entry::from_bound(entry))
            .chain(
                embeddings
                    .iter()
                    .map(|(model, entry)| Entry::from_embedding(model, entry)),
            )
            .collect();
        let value = Snapshot {
            object: "morphiecore.capabilities",
            version: 1,
            scope: "gateway_contract",
            coverage: "partial",
            unlisted: "not_assessed",
            provider_effectiveness: "not_asserted",
            full_loop: "not_asserted",
            data,
        };
        let bytes = serde_json::to_vec(&value).map_err(|_| StartupError::Binding)?;
        if bytes.len() > response_bytes.min(MAX_BYTES) {
            return Err(StartupError::Limits);
        }
        Ok(Self(Bytes::from(bytes)))
    }
}
pub(in crate::gateway) async fn retrieve(
    State(state): State<Arc<Runtime>>,
) -> Result<Response, ApiError> {
    if state.shutdown.is_cancelled() {
        return Err(ApiError::shutdown());
    }
    Ok(Response::builder()
        .header("content-type", "application/json")
        .header("cache-control", "no-store")
        .body(Body::from(state.capabilities.0.clone()))
        .expect("static capability headers"))
}
pub(in crate::gateway) fn notice(mut response: Response) -> Response {
    response.headers_mut().insert(
        "x-morphiecore-adaptation",
        HeaderValue::from_static("possible"),
    );
    response
        .headers_mut()
        .append("link", HeaderValue::from_static(LINK));
    response
}
