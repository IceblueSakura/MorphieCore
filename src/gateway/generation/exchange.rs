//! Fixed attempt-chain coordination. HTTP body publication and handoff stay in body.
use crate::gateway::{
    ApiError, BoundCandidate, BoundEntry, Limits, Runtime,
    body::{self, Lane, Message, Status},
    diagnostics::{Stage, Trace},
    generation::intake,
};
use crate::{
    adapter::Request,
    execution::{DeliveryState, fallback::Transition},
};
use axum::response::Response;
use std::{
    sync::{Arc, atomic::Ordering},
    time::Duration,
};
use tokio::{
    sync::{OwnedSemaphorePermit, mpsc},
    time::Instant,
};
pub(in crate::gateway) enum Upstreams {
    Http {
        runtime: Arc<Runtime>,
        candidates: Vec<Arc<BoundCandidate>>,
        history: Option<Box<super::history::Pending>>,
    },
    #[cfg(test)]
    Observed {
        candidate: Arc<BoundCandidate>,
        response: reqwest::Response,
    },
}
#[allow(clippy::too_many_arguments)]
pub(in crate::gateway) async fn produce_chain(
    source: Upstreams,
    entry: &BoundEntry,
    request: &Request,
    limits: &Limits,
    deadline: Instant,
    tx: &mpsc::Sender<Message>,
    state: &Status,
    trace: &mut Trace,
) -> Result<(), ApiError> {
    let lane = Lane { tx, state };
    let model = entry.public.id.as_str();
    let conversation = request
        .conversation
        .as_ref()
        .filter(|c| c.is_conversation_scoped())
        .map(|c| c.id());
    let (runtime, candidates, mut observed, history) = match source {
        Upstreams::Http {
            runtime,
            mut candidates,
            history,
        } => {
            let order = runtime
                .executors
                .order(model, conversation, &mut candidates);
            trace.executor_order(order);
            (Some(runtime), candidates, None, history)
        }
        #[cfg(test)]
        Upstreams::Observed {
            candidate,
            response,
        } => (None, vec![candidate], Some(response), None),
    };
    for (index, candidate) in candidates
        .iter()
        .enumerate()
        .take(entry.policy.max_attempts)
    {
        if Instant::now() >= deadline {
            return Err(ApiError::timeout());
        }
        trace.begin_candidate(index + 1);
        trace.executor(candidate.provider.id.as_str());
        let attempt_deadline = deadline
            .min(Instant::now() + Duration::from_millis(candidate.endpoint.execution.timeout_ms));
        let result = tokio::time::timeout_at(attempt_deadline, async {
            // Each candidate starts from the original immutable input. Only the fixed
            // bound credential is validated; affinity never chooses or advances it.
            let mut local = std::borrow::Cow::Borrowed(request);
            let mut affinity = None;
            let upstream = if let Some(response) = observed.take() {
                response
            } else {
                let runtime = runtime.as_ref().ok_or(ApiError::upstream())?;
                trace.stage(Stage::Prepare);
                let (secret, grant) = candidate.secret.resolve()?;
                affinity = crate::gateway::routing::prefix_cache::choose(
                    &runtime.affinity,
                    candidate,
                    request,
                    trace,
                );
                if let Some(selection) = &affinity {
                    if let Some(projected) = selection.project(candidate, request) {
                        local = std::borrow::Cow::Owned(projected);
                    } else {
                        // A hint must not turn an otherwise valid request into a
                        // body-budget/admission error. Reuse the original projection.
                        local = std::borrow::Cow::Borrowed(request);
                        affinity = None;
                        trace.affinity("unavailable", false, false);
                    }
                }
                let (mut prepared, rules, controls) =
                    crate::execution::attempt::prepare_with_evidence(
                        &candidate.endpoint,
                        &candidate.provider,
                        &secret,
                        &local,
                    )
                    .map_err(|_| ApiError::invalid())?;
                trace.request_controls(&controls);
                trace.request_rules(rules);
                if let Some(grant) = grant {
                    prepared.safe_headers.extend(
                        crate::provider::subscription::headers(candidate.provider.auth, &grant)
                            .map_err(|_| ApiError::upstream())?,
                    );
                }
                let key_sent = serde_json::from_slice::<serde_json::Value>(&prepared.body)
                    .ok()
                    .is_some_and(|v| {
                        v.get("prompt_cache_key")
                            .is_some_and(serde_json::Value::is_string)
                    });
                let session_sent = prepared
                    .safe_headers
                    .iter()
                    .any(|(name, _)| name == "x-opencode-session");
                trace.stage(Stage::Connect);
                if let Some(capture) = &trace.capture {
                    capture.upstream_request(&prepared, index + 1);
                }
                let response = runtime
                    .transport
                    .send(
                        prepared,
                        attempt_deadline.saturating_duration_since(Instant::now()),
                    )
                    .await
                    .map_err(ApiError::transport)?;
                trace.affinity_carriers(key_sent, session_sent);
                response
            };
            // Hold only while another candidate could still answer.
            let hold = candidates
                .get(index + 1)
                .filter(|_| index + 1 < entry.policy.max_attempts)
                .is_some_and(|next| {
                    entry.policy.fallback == crate::topology::FallbackPolicy::BeforeCommit
                        || next.endpoint.id == candidate.endpoint.id
                            && candidate.credential_fallback
                });
            let successor = intake::produce_candidate(
                entry,
                candidate,
                &local,
                upstream,
                limits,
                &lane,
                hold,
                history.as_deref(),
                trace,
            )
            .await?;
            if let (Some(selection), Some(successor), Some(runtime)) =
                (affinity, successor, &runtime)
            {
                selection.publish(&runtime.affinity, candidate, Some(&successor));
            }
            Ok(())
        })
        .await
        .unwrap_or_else(|_| Err(ApiError::timeout()));
        match result {
            Ok(()) => {
                if let Some(runtime) = &runtime {
                    runtime.executors.completed(model, conversation, candidate);
                }
                trace.end_candidate(None, false);
                return Ok(());
            }
            Err(error) => {
                if let (Some(runtime), Some(class)) = (&runtime, error.fallback) {
                    runtime
                        .executors
                        .failed(candidate, class, error.retry_after);
                }
                let visibility = if state.published.load(Ordering::Acquire) {
                    DeliveryState::Committed
                } else {
                    DeliveryState::Uncommitted
                };
                let advance = index + 1 < candidates.len()
                    && error.fallback.is_some_and(|class| {
                        let next = &candidates[index + 1];
                        Transition::between(
                            &candidate.endpoint,
                            &next.endpoint,
                            candidate.credential_fallback,
                        )
                        .may_advance(
                            &entry.policy,
                            visibility,
                            class,
                            index + 1,
                            Instant::now() < deadline,
                        )
                    });
                trace.end_candidate(Some(error.code), advance);
                if !advance {
                    return Err(error);
                }
            }
        }
    }
    Err(ApiError::upstream())
}
pub(in crate::gateway) async fn run(
    runtime: Arc<Runtime>,
    entry: Arc<BoundEntry>,
    request: Request,
    history: Option<Box<super::history::Pending>>,
    deadline: Instant,
    permit: OwnedSemaphorePermit,
    trace: Trace,
) -> Result<Response, ApiError> {
    let candidates = entry.eligible(&request)?;
    let limits = runtime.limits.clone();
    let shutdown = runtime.shutdown.clone();
    body::respond_source(
        entry,
        request,
        Upstreams::Http {
            runtime,
            candidates,
            history,
        },
        limits,
        deadline,
        shutdown,
        permit,
        trace,
    )
    .await
}
