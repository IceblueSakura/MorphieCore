//! Fixed attempt-chain coordination. HTTP body publication and handoff stay in body.
use crate::{
    gateway::{
        ApiError, BoundCandidate, BoundEntry, Limits, Runtime,
        body::{self, Lane, Message, Status},
        diagnostics::{Stage, Trace},
        generation::intake,
        routing::chain::Chain,
    },
    protocol::GenerationRequestEnvelope as Request,
};
use axum::response::Response;
use std::{
    borrow::Cow,
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
            if error.recovery.affects_health {
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
/// One candidate's prepared request on the wire, with the affinity projection it used.
struct Sent<'r> {
    response: reqwest::Response,
    request: Cow<'r, Request>,
    affinity: Option<crate::gateway::routing::prefix_cache::Selection>,
}
/// Resolve the bound credential, apply an affinity projection when it fits,
/// encode for this candidate and send. Each candidate starts from the original
/// immutable input; affinity never chooses or advances the credential.
async fn send_attempt<'r>(
    runtime: &Runtime,
    candidate: &BoundCandidate,
    request: &'r Request,
    ordinal: usize,
    deadline: Instant,
    state: &Status,
    trace: &mut Trace,
) -> Result<Sent<'r>, ApiError> {
    trace.stage(Stage::Prepare);
    let (secret, grant) = candidate.secret.resolve_for_request().await?;
    let mut local = Cow::Borrowed(request);
    let mut affinity =
        crate::gateway::routing::prefix_cache::choose(&runtime.affinity, candidate, request, trace);
    if let Some(selection) = &affinity {
        match selection.project(candidate, request) {
            Some(projected) => local = Cow::Owned(projected),
            None => {
                // A hint must not turn an otherwise valid request into a
                // body-budget/admission error. Reuse the original projection.
                affinity = None;
                trace.affinity("unavailable", false, false);
            }
        }
    }
    let conversion = crate::execution::attempt::prepare_with_evidence(
        &candidate.endpoint,
        &candidate.provider,
        &secret,
        &local,
    )
    .map_err(|error| {
        trace.report.failure(
            Some(crate::protocol::conversion::generation::ConversionBoundary::RequestEncode),
            &error,
        );
        ApiError::invalid()
    })?;
    let mut prepared = conversion.value;
    if let Some(grant) = grant {
        prepared.safe_headers.extend(
            crate::provider::subscription::headers(candidate.provider.auth, &grant)
                .map_err(|_| ApiError::upstream())?,
        );
    }
    trace.report.accept(
        crate::execution::report::View::UpstreamRequest,
        conversion.evidence,
    );
    trace.report.update(|a| a.prepared = true);
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
        capture.upstream_request(&prepared, ordinal);
    }
    state.waiting_upstream.store(true, Ordering::Release);
    trace.report.update(|a| a.send_started = true);
    let response = runtime
        .transport
        .send(prepared, deadline.saturating_duration_since(Instant::now()))
        .await
        .map_err(|class| {
            trace
                .report
                .failure(None, &crate::execution::AttemptError::Transport(class));
            ApiError::transport(class.recovery_class())
        })?;
    state.waiting_upstream.store(false, Ordering::Release);
    trace.affinity_carriers(key_sent, session_sent);
    Ok(Sent {
        response,
        request: local,
        affinity,
    })
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
    let mut blocked_health = None;
    let mut pending_error = None;
    for (index, candidate) in candidates.iter().enumerate() {
        if let Some(reason) = chain.skip(index) {
            trace.selection_skip(reason);
            continue;
        }
        if Instant::now() >= deadline {
            if let Some(code) = pending_error.take() {
                trace.end_candidate(Some(code), false);
            }
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
                    if pending_error.is_some() {
                        blocked_health = Some(observation);
                    } else {
                        trace.health(observation);
                    }
                    trace.selection_skip(observation.reason);
                    continue;
                }
            }
        } else {
            None
        };
        if let Some(code) = pending_error.take() {
            trace.report.update(|a| a.advanced = true);
            trace.end_candidate(Some(code), true);
        }
        blocked_health = None;
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
            let mut local = Cow::Borrowed(request);
            let mut affinity = None;
            let upstream = if let Some(response) = observed.take() {
                response
            } else {
                let runtime = runtime.as_ref().ok_or(ApiError::upstream())?;
                let sent = send_attempt(
                    runtime,
                    candidate,
                    request,
                    chain.attempts,
                    attempt_deadline,
                    state,
                    trace,
                )
                .await?;
                local = sent.request;
                affinity = sent.affinity;
                sent.response
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
                request,
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
        .unwrap_or_else(|_| {
            trace.report.failure(
                None,
                &crate::execution::AttemptError::Deadline(
                    crate::execution::attempt::DeadlineReason::Attempt,
                ),
            );
            Err(state.timeout_error())
        });
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
                trace.report.update(|a| {
                    a.failure_code = Some(error.diagnostic_code());
                    a.failure_status_hint = Some(error.status.as_u16());
                    a.policy_class = error.recovery.class;
                    a.health_failure = error.recovery.affects_health;
                });
                if let Some(observation) = health.failed(error) {
                    trace.health(observation);
                }
                drop(health);
                if let Some(class) = error.recovery.class {
                    chain.failed(index, class);
                }
                let advance = !state.published.load(Ordering::Acquire)
                    && error.recovery.class.is_some()
                    && Instant::now() < deadline
                    && (index + 1..candidates.len()).any(|next| chain.skip(next).is_none());
                if !advance {
                    trace.selection_stop(if state.published.load(Ordering::Acquire) {
                        "published"
                    } else if error.recovery.class.is_none() {
                        "not_retryable"
                    } else if Instant::now() >= deadline {
                        "deadline"
                    } else if chain.attempts >= entry.policy.max_attempts {
                        "route_budget"
                    } else {
                        "no_successor"
                    });
                    trace.end_candidate(Some(error.diagnostic_code()), false);
                    return Err(error);
                }
                // Permission to advance is not an observed successor attempt.
                trace.report.end(false);
                pending_error = Some(error.diagnostic_code());
            }
        }
    }
    if let Some(code) = pending_error {
        trace.end_candidate(Some(code), false);
    }
    if let Some(observation) = blocked_health {
        trace.health(observation);
    }
    trace.selection_stop(if blocked_delay.is_some() {
        "unavailable"
    } else {
        "no_successor"
    });
    Err(blocked_delay.map_or_else(ApiError::upstream, ApiError::unavailable))
}
/// Generation's fixed attempt chain behind one response body.
struct GenerationExchange {
    entry: Arc<BoundEntry>,
    request: Box<Request>,
    source: Upstreams,
}
impl body::Produce for GenerationExchange {
    async fn produce(
        self,
        limits: &Limits,
        deadline: Instant,
        lane: &Lane<'_>,
        trace: &mut Trace,
    ) -> Result<(), ApiError> {
        produce_chain(
            self.source,
            &self.entry,
            &self.request,
            limits,
            deadline,
            lane.tx,
            lane.state,
            trace,
        )
        .await
    }
}
#[allow(clippy::too_many_arguments)]
pub(in crate::gateway) async fn respond_source(
    entry: Arc<BoundEntry>,
    request: Request,
    source: Upstreams,
    limits: Limits,
    deadline: Instant,
    shutdown: tokio_util::sync::CancellationToken,
    permit: OwnedSemaphorePermit,
    trace: Trace,
) -> Result<Response, ApiError> {
    let stream = request.delivery.streaming();
    body::respond_producer(
        GenerationExchange {
            entry,
            request: Box::new(request),
            source,
        },
        if stream {
            body::ResponseType::EventStream
        } else {
            body::ResponseType::Json
        },
        limits,
        deadline,
        shutdown,
        permit,
        trace,
    )
    .await
}
pub(in crate::gateway) async fn run(
    runtime: Arc<Runtime>,
    entry: Arc<BoundEntry>,
    request: Request,
    history: Option<Box<super::history::Pending>>,
    deadline: Instant,
    permit: OwnedSemaphorePermit,
    mut trace: Trace,
) -> Result<Response, ApiError> {
    let candidates = entry.eligible(&request).map_err(|cause| {
        let error = ApiError::invalid();
        trace.report.planning_failure(cause);
        trace
            .report
            .request_failure(error.diagnostic_code(), error.status.as_u16());
        trace.outcome(crate::gateway::diagnostics::Outcome::Error);
        error
    })?;
    let limits = runtime.limits.clone();
    let shutdown = runtime.shutdown.clone();
    respond_source(
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
        Some(ApiError::transport(crate::provider::RecoveryClass::Server));
    // A downstream cancellation after the error was observed must not erase it.
    drop(attempt);
    assert!(
        gate.state
            .executors
            .unavailable(&candidate.health, Instant::now())
            .is_some()
    );
}
