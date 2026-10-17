//! Fixed attempt-chain coordination. HTTP body publication and handoff stay in body.
use crate::gateway::{
    ApiError, BoundCandidate, BoundEntry, Limits, Runtime,
    body::{self, Lane, Message, Status},
    diagnostics::{Stage, Trace},
    generation::intake,
};
use crate::{adapter::Request, gateway::routing::chain::Chain};
use axum::response::Response;
use std::{
    sync::{Arc, atomic::Ordering},
    time::Duration,
};
use tokio::{
    sync::{OwnedSemaphorePermit, mpsc},
    time::Instant,
};
/// Preserve already observed upstream failures even if cancellation interrupts
/// delivery of their failure frame. Unknown cancellation remains health-neutral.
struct HealthAttempt<'a> {
    lease: Option<crate::gateway::routing::executors::Lease<'a>>,
    state: &'a Status,
}
impl HealthAttempt<'_> {
    fn failed(&self, error: ApiError) -> Option<crate::gateway::routing::executors::Observation> {
        let observed = self
            .state
            .failure_head
            .lock()
            .expect("upstream failure")
            .take();
        self.lease.as_ref()?.failed(
            if error.health {
                error
            } else {
                observed.unwrap_or(error)
            },
            Instant::now(),
        )
    }
}
impl Drop for HealthAttempt<'_> {
    fn drop(&mut self) {
        if let Some(error) = self
            .state
            .failure_head
            .lock()
            .expect("upstream failure")
            .take()
            && let Some(lease) = &self.lease
        {
            lease.failed(error, Instant::now());
        }
    }
}

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
    let mut chain = Chain::new(&candidates, &entry.policy);
    let mut blocked_delay: Option<u64> = None;
    for (index, candidate) in candidates.iter().enumerate() {
        if let Some(reason) = chain.skip(index) {
            trace.selection_skip(reason);
            continue;
        }
        if Instant::now() >= deadline {
            return Err(ApiError::timeout());
        }
        let lease = if let Some(runtime) = &runtime {
            match runtime.executors.acquire(candidate, Instant::now()) {
                Ok(lease) => Some(lease),
                Err(observation) => {
                    blocked_delay = Some(
                        blocked_delay.map_or(observation.retry_after_seconds, |delay| {
                            delay.min(observation.retry_after_seconds)
                        }),
                    );
                    trace.health(observation);
                    trace.selection_skip(observation.reason);
                    continue;
                }
            }
        } else {
            None
        };
        chain.begin(index);
        trace.begin_candidate(chain.attempts);
        trace.probe(lease.as_ref().is_some_and(|lease| lease.probing));
        trace.executor(candidate.provider.id.as_str());
        state.waiting_upstream.store(false, Ordering::Release);
        *state.failure_head.lock().expect("failure head") = None;
        let health = HealthAttempt { lease, state };
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
                    capture.upstream_request(&prepared, chain.attempts);
                }
                state.waiting_upstream.store(true, Ordering::Release);
                let response = runtime
                    .transport
                    .send(
                        prepared,
                        attempt_deadline.saturating_duration_since(Instant::now()),
                    )
                    .await
                    .map_err(ApiError::transport)?;
                state.waiting_upstream.store(false, Ordering::Release);
                trace.affinity_carriers(key_sent, session_sent);
                response
            };
            // Hold only while another candidate could still answer.
            let hold = (index + 1..candidates.len()).any(|next| {
                chain.could_follow(index, next)
                    && runtime.as_ref().is_none_or(|runtime| {
                        runtime
                            .executors
                            .unavailable(&candidates[next].health, Instant::now())
                            .is_none()
                    })
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
        .unwrap_or_else(|_| Err(state.timeout_error()));
        match result {
            Ok(()) => {
                if let Some(lease) = &health.lease {
                    let fresh = lease.completed(model, conversation, Instant::now());
                    trace.affinity_commit(fresh);
                }
                trace.end_candidate(None, false);
                return Ok(());
            }
            Err(error) => {
                if let Some(observation) = health.failed(error) {
                    trace.health(observation);
                }
                drop(health);
                if let Some(class) = error.fallback {
                    chain.failed(index, class);
                }
                let advance = !state.published.load(Ordering::Acquire)
                    && error.fallback.is_some()
                    && Instant::now() < deadline
                    && (index + 1..candidates.len()).any(|next| chain.skip(next).is_none());
                if !advance {
                    trace.selection_stop(if state.published.load(Ordering::Acquire) {
                        "published"
                    } else if error.fallback.is_none() {
                        "not_retryable"
                    } else if Instant::now() >= deadline {
                        "deadline"
                    } else if chain.attempts >= entry.policy.max_attempts {
                        "route_budget"
                    } else {
                        "no_successor"
                    });
                    trace.end_candidate(Some(error.code), false);
                    return Err(error);
                }
                trace.end_candidate(Some(error.code), true);
            }
        }
    }
    trace.selection_stop(if blocked_delay.is_some() {
        "unavailable"
    } else {
        "no_successor"
    });
    Err(blocked_delay.map_or_else(ApiError::upstream, ApiError::unavailable))
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

#[cfg(test)]
#[test]
fn interrupted_error_delivery_keeps_only_observed_upstream_failure() {
    let gate = crate::gateway::tests::support::gateway(Limits::default());
    let entry = gate.state.entries.values().next().unwrap();
    let candidate = &entry.candidates[0];
    let state = Status::default();
    let lease = gate
        .state
        .executors
        .acquire(candidate, Instant::now())
        .unwrap();
    let attempt = HealthAttempt {
        lease: Some(lease),
        state: &state,
    };
    // A normal cancellation has no failure to invent.
    drop(attempt);
    assert!(
        gate.state
            .executors
            .unavailable(&candidate.health, Instant::now())
            .is_none()
    );
    let lease = gate
        .state
        .executors
        .acquire(candidate, Instant::now())
        .unwrap();
    let attempt = HealthAttempt {
        lease: Some(lease),
        state: &state,
    };
    *state.failure_head.lock().unwrap() =
        Some(ApiError::transport(crate::provider::ErrorClass::Server));
    // A downstream cancellation after the error was observed must not erase it.
    drop(attempt);
    assert!(
        gate.state
            .executors
            .unavailable(&candidate.health, Instant::now())
            .is_some()
    );
}
