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
    delivery_modes: [&'static str; 2],
    request: Vec<Case>,
    delivery: Vec<Case>,
    history: Vec<Case>,
}
impl<'a> Entry<'a> {
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
            let admitted = entry.public.contract.explicit_cache;
            if !admitted
                || entry.candidates.iter().all(|candidate| {
                    candidate.endpoint.protocol
                        == crate::topology::ProtocolProfile::AnthropicMessages
                        && candidate.endpoint.representation.semantics().explicit_cache
                })
            {
                request.push(Case::new(
                    "generation.cache",
                    "valid_owner_local_breakpoints_5m_or_1h",
                    if admitted {
                        Treatment::Faithful
                    } else {
                        Treatment::Rejected
                    },
                ));
            }
            if !entry.public.contract.automatic_prefix_cache {
                request.push(Case::new(
                    "generation.cache",
                    "request_wide_automatic_policy",
                    Treatment::Rejected,
                ));
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
        Self {
            model: entry.public.id.as_str(),
            protocol: match protocol {
                ClientProtocol::Chat => "chat_completions",
                ClientProtocol::Responses => "responses",
                ClientProtocol::Messages => "messages",
            },
            delivery_modes: ["json", "sse"],
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
        response_bytes: usize,
    ) -> Result<Self, StartupError> {
        if entries.len() > 64 * 3 {
            return Err(StartupError::Binding);
        }
        let data = entries
            .values()
            .map(|entry| Entry::from_bound(entry))
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
