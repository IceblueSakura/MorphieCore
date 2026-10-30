//! Bound entries, request-local projections and shared gateway state.
use super::{
    Limits, diagnostics,
    ingress::{auth, capabilities, discovery, models},
    routing::{executors, prefix_cache as affinity},
    startup::credentials,
    tasks::{embeddings, images, speech, transcription},
};
use crate::{
    adapter::ClientAdapter,
    protocol::ClientProtocol,
    provider::ProviderDefinition,
    topology::{Endpoint, PublicModel},
    transport::http::HttpTransport,
};
use std::{collections::BTreeMap, sync::Arc};
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;
#[derive(Clone)]
pub(super) struct BoundCandidate {
    pub(super) endpoint: Endpoint,
    pub(super) provider: ProviderDefinition,
    pub(super) secret: credentials::Source,
    pub(super) credential_fallback: bool,
    pub(super) credential_attempts: usize,
    pub(super) health: executors::Scopes,
    pub(super) cache_scope: crate::semantic::value::ReplayOrigin,
}
/// Bound identity and its optional, request-local preflight conversion move together.
pub(super) struct PlannedCandidate {
    pub(super) candidate: Arc<BoundCandidate>,
    pub(super) projection: Option<
        crate::protocol::conversion::generation::Conversion<
            crate::execution::attempt::EncodedRequest,
        >,
    >,
}
impl PlannedCandidate {
    pub(super) fn into_projection(
        self,
        request: &crate::protocol::GenerationRequestEnvelope,
        affinity: &affinity::Index,
    ) -> Result<
        crate::protocol::conversion::generation::Conversion<
            crate::execution::attempt::EncodedRequest,
        >,
        crate::execution::AttemptError,
    > {
        match self.projection {
            Some(projection) => Ok(projection),
            None => affinity.materialize(
                &self.candidate.endpoint,
                request,
                crate::execution::attempt::RequestProjection::encode(
                    &self.candidate.endpoint,
                    request,
                )?,
            ),
        }
    }
}
impl AsRef<BoundCandidate> for PlannedCandidate {
    fn as_ref(&self) -> &BoundCandidate {
        &self.candidate
    }
}

/// Cache misses recompute the same pure conversion, never reject or remove a candidate.
const MAX_RETAINED_REQUEST_BYTES: usize = 1 << 20;

fn retain(
    endpoint: &Endpoint,
    request: &crate::protocol::GenerationRequestEnvelope,
    projection: crate::execution::attempt::RequestProjection,
    remaining: &mut usize,
    affinity: &affinity::Index,
) -> Option<
    crate::protocol::conversion::generation::Conversion<crate::execution::attempt::EncodedRequest>,
> {
    let evidence_bytes = projection.evidence().retained_bytes();
    let limit = remaining.checked_sub(evidence_bytes)?;
    if projection.encoded_len() > limit {
        return None;
    }
    let projection = affinity
        .materialize(endpoint, request, projection)
        .expect("bounded codec-produced JSON serializes");
    let charged = projection
        .value
        .retained_bytes()
        .checked_add(evidence_bytes)?;
    *remaining = remaining.checked_sub(charged)?;
    Some(projection)
}

#[derive(Clone)]
pub(super) struct BoundEntry {
    pub(super) public: PublicModel,
    pub(super) client: ClientAdapter,
    pub(super) policy: crate::topology::RoutePolicy,
    pub(super) candidates: Vec<Arc<BoundCandidate>>,
}
impl BoundEntry {
    pub(super) fn eligible(
        &self,
        request: &crate::protocol::GenerationRequestEnvelope,
        affinity: &affinity::Index,
    ) -> Result<Vec<PlannedCandidate>, crate::execution::plan::PlanError> {
        let mut remaining = MAX_RETAINED_REQUEST_BYTES;
        let (candidates, _) = crate::execution::plan::select_candidates_with(
            request,
            self.policy.candidates,
            self.candidates.iter().map(|c| &c.endpoint),
            Some(&self.client),
            |ordinal, endpoint, projection| PlannedCandidate {
                candidate: self.candidates[ordinal].clone(),
                projection: retain(endpoint, request, projection, &mut remaining, affinity),
            },
        )?;
        Ok(candidates)
    }
}
pub(super) struct Runtime {
    pub(super) history: Arc<crate::gateway::generation::history::Store>,
    pub(super) diagnostics: Option<diagnostics::Sink>,
    pub(super) capture: Option<super::capture::Sink>,
    pub(super) affinity: affinity::Index,
    pub(super) executors: executors::Executors,
    pub(super) auth: auth::Auth,
    pub(super) request_ids: super::ingress::correlation::RequestIds,
    pub(super) entries: BTreeMap<(ClientProtocol, String), Arc<BoundEntry>>,
    pub(super) images: BTreeMap<String, Arc<images::BoundImage>>,
    pub(super) speech: BTreeMap<String, Arc<speech::BoundSpeech>>,
    pub(super) transcriptions: BTreeMap<String, Arc<transcription::BoundTranscription>>,
    pub(super) embeddings: BTreeMap<String, Arc<embeddings::BoundEmbedding>>,
    pub(super) models: models::ModelView,
    pub(super) text_models: discovery::TextModelView,
    pub(super) capabilities: capabilities::CapabilityView,
    pub(super) limits: Limits,
    pub(super) permits: Arc<Semaphore>,
    pub(super) transport: HttpTransport,
    pub(super) shutdown: CancellationToken,
    pub(super) serving: tokio::sync::Mutex<()>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::conversion::generation::{
        Change, Conversion, ConversionBoundary, Effect, EvidenceDelta, Field,
    };
    use serde_json::json;

    #[test]
    fn retained_preflight_preserves_reserved_schema_keys() {
        let gate = crate::gateway::tests::support::gateway(Limits::default());
        let entry = &gate.state.entries[&(ClientProtocol::Chat, "fixture-model".into())];
        for annotation in [
            json!({"$serde_json::private::Number": "not-a-number"}),
            json!({"$serde_json::private::Number": "1"}),
        ] {
            let request = entry
                .client
                .decode_client_request(
                    json!({"model":"fixture-model","messages":[{"role":"user","content":"hello"}],
                    "tools":[{"type":"function","function":{"name":"lookup","strict":false,
                        "parameters":{"type":"object","properties":{},"default":annotation}}}]})
                    .to_string()
                    .as_bytes(),
                )
                .unwrap();
            for hit in [true, false] {
                let mut planned = entry
                    .eligible(&request, &gate.state.affinity)
                    .unwrap()
                    .remove(0);
                assert!(planned.projection.is_some());
                if !hit {
                    planned.projection = None;
                }
                let candidate = planned.candidate.clone();
                let conversion = planned
                    .into_projection(&request, &gate.state.affinity)
                    .unwrap();
                let secret = candidate.secret.resolve().unwrap().0;
                let prepared = crate::execution::attempt::prepare_retained(
                    &candidate.endpoint,
                    &candidate.provider,
                    &secret,
                    &request,
                    conversion,
                )
                .unwrap();
                let wire = crate::protocol::json::decode(&prepared.value.body).unwrap();
                assert_eq!(
                    wire["tools"][0]["function"]["parameters"]["default"],
                    annotation
                );
            }
        }
    }

    #[test]
    fn retention_charges_spare_evidence_capacity() {
        let gate = crate::gateway::tests::support::gateway(Limits::default());
        let entry = &gate.state.entries[&(ClientProtocol::Chat, "fixture-model".into())];
        let candidate = &entry.candidates[0];
        let request = entry
            .client
            .decode_client_request(
                br#"{"model":"fixture-model","messages":[{"role":"user","content":"hello"}]}"#,
            )
            .unwrap();
        let value = candidate.endpoint.encode_request(&request).unwrap();
        let encoded = crate::execution::attempt::encode_projection(
            &candidate.endpoint,
            &request,
            crate::execution::attempt::RequestProjection::new(
                Conversion {
                    value: value.clone(),
                    evidence: EvidenceDelta::default(),
                },
                candidate.endpoint.execution.request_body_limit,
            )
            .unwrap(),
        )
        .unwrap();
        let mut evidence = EvidenceDelta::default();
        evidence.push(Effect::new(
            "test.preflight.v1",
            Field::Prediction,
            ConversionBoundary::RequestEncode,
            Change::Omit,
        ));
        evidence.retain(|_| false);
        let mut remaining = encoded.value.retained_bytes() + std::mem::size_of::<EvidenceDelta>();
        let before = remaining;
        assert!(
            retain(
                &candidate.endpoint,
                &request,
                crate::execution::attempt::RequestProjection::new(
                    Conversion { value, evidence },
                    candidate.endpoint.execution.request_body_limit,
                )
                .unwrap(),
                &mut remaining,
                &gate.state.affinity,
            )
            .is_none()
        );
        assert_eq!(remaining, before);
    }

    #[test]
    fn exhausted_retention_keeps_candidates_and_recomputes_the_same_conversion() {
        let gate = crate::gateway::tests::support::gateway(Limits::default());
        let mut entry = gate.state.entries[&(ClientProtocol::Chat, "fixture-model".into())]
            .as_ref()
            .clone();
        entry.candidates =
            vec![entry.candidates[0].clone(); crate::execution::plan::MAX_CANDIDATES];
        let request = entry.client.decode_client_request(
            json!({"model":"fixture-model","messages":[{"role":"user","content":"x".repeat(32 << 10)}]})
                .to_string().as_bytes(),
        ).unwrap();
        let original = request.clone();
        let mut selected = entry.eligible(&request, &gate.state.affinity).unwrap();
        assert_eq!(selected.len(), entry.candidates.len());
        let hits = selected
            .iter()
            .filter_map(|candidate| candidate.projection.as_ref());
        let mut retained_count = 0;
        let charged: usize = hits
            .map(|projection| {
                retained_count += 1;
                projection.value.retained_bytes() + projection.evidence.retained_bytes()
            })
            .sum();
        assert!(charged <= MAX_RETAINED_REQUEST_BYTES);
        assert!(retained_count > 0 && retained_count < selected.len());
        let miss = selected.pop().unwrap();
        assert!(miss.projection.is_none());
        let hit = selected.remove(0);
        assert!(hit.projection.is_some());
        let candidate = hit.candidate.clone();
        let secret = candidate.secret.resolve().unwrap().0;
        let prepare = |planned: PlannedCandidate| {
            crate::execution::attempt::prepare_retained(
                &candidate.endpoint,
                &candidate.provider,
                &secret,
                &request,
                planned
                    .into_projection(&request, &gate.state.affinity)
                    .unwrap(),
            )
            .unwrap()
        };
        let retained = prepare(hit);
        let recomputed = prepare(miss);
        assert_eq!(retained.value, recomputed.value);
        assert_eq!(retained.evidence, recomputed.evidence);
        assert_eq!(request, original);
    }

    #[test]
    fn body_evidence_and_credentials_follow_reordered_bound_candidates() {
        let gate = crate::gateway::tests::support::gateway(Limits::default());
        let mut entry = gate.state.entries[&(ClientProtocol::Chat, "fixture-model".into())]
            .as_ref()
            .clone();
        let mut first = entry.candidates[0].as_ref().clone();
        first
            .endpoint
            .representation
            .openai_mut()
            .unwrap()
            .prediction = false;
        let mut second = first.clone();
        second.endpoint.id = crate::topology::EndpointId::new("fixture-second").unwrap();
        second.endpoint.upstream_model = "private-second".into();
        second
            .endpoint
            .representation
            .openai_mut()
            .unwrap()
            .prediction = true;
        second.secret = credentials::Source::ApiKey(Arc::new(
            crate::provider::SecretMaterial::new("synthetic-second-credential").unwrap(),
        ));
        second.cache_scope = crate::semantic::value::ReplayOrigin::new("synthetic-second").unwrap();
        entry.candidates = vec![Arc::new(first), Arc::new(second)];
        let request = entry.client.decode_client_request(
            br#"{"model":"fixture-model","messages":[{"role":"user","content":"hello"}],"prediction":{"type":"content","content":"hint"}}"#,
        ).unwrap();
        let original = request.clone();
        let mut selected = entry.eligible(&request, &gate.state.affinity).unwrap();
        assert!(selected.iter().all(|planned| planned.projection.is_some()));
        let now = tokio::time::Instant::now();
        let lease = gate
            .state
            .executors
            .acquire(&selected[1].candidate, now)
            .unwrap();
        assert!(lease.completed("fixture-model", Some("session"), now));
        drop(lease);
        assert_eq!(
            gate.state
                .executors
                .order("fixture-model", Some("session"), &mut selected),
            "affinity"
        );
        assert!(Arc::ptr_eq(&selected[0].candidate, &entry.candidates[1]));
        for planned in selected {
            let candidate = planned.candidate.clone();
            let secret = candidate.secret.resolve().unwrap().0;
            let prepared = crate::execution::attempt::prepare_retained(
                &candidate.endpoint,
                &candidate.provider,
                &secret,
                &request,
                planned
                    .into_projection(&request, &gate.state.affinity)
                    .unwrap(),
            )
            .unwrap();
            let direct = crate::execution::attempt::prepare_with_evidence(
                &candidate.endpoint,
                &candidate.provider,
                &secret,
                &request,
            )
            .unwrap();
            assert_eq!(prepared.value, direct.value);
            assert_eq!(prepared.evidence, direct.evidence);
            let wire = crate::protocol::json::decode(&prepared.value.body).unwrap();
            let supports_prediction = candidate
                .endpoint
                .representation
                .openai()
                .unwrap()
                .prediction;
            assert_eq!(wire["model"], candidate.endpoint.upstream_model);
            assert_eq!(wire.get("prediction").is_some(), supports_prediction);
            assert_eq!(
                prepared
                    .evidence
                    .contains_rule("generation.omit-prediction.v1"),
                !supports_prediction
            );
        }
        assert_eq!(request, original);
    }
}
