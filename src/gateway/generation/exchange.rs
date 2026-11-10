//! Fixed attempt-chain coordination. HTTP body publication and handoff stay in body.
use crate::{
    gateway::{
        ApiError, BoundEntry, Limits, Runtime,
        body::{self, Lane, Status},
        diagnostics::{Outcome, Stage, Trace},
        generation::intake,
        routing::executors::BindingObservation,
    },
    protocol::GenerationRequestEnvelope as Request,
};
use axum::response::Response;
use std::{
    borrow::Cow,
    sync::{Arc, atomic::Ordering},
    time::Duration,
};
use tokio::{sync::OwnedSemaphorePermit, time::Instant};
/// Preserve already observed upstream failures even if cancellation interrupts
/// delivery of their failure frame. Unknown cancellation remains health-neutral.
struct HealthAttempt<'a> {
    lease: Option<crate::gateway::routing::executors::Lease<'a>>,
    state: &'a Status,
}
impl HealthAttempt<'_> {
    fn failed(&self, error: ApiError) -> Option<crate::gateway::routing::executors::Observation> {
        let failures = self.state.take_failures(Some(error));
        self.lease
            .as_ref()?
            .resolve_failures(&failures, Instant::now())
    }
}
impl Drop for HealthAttempt<'_> {
    fn drop(&mut self) {
        let failures = self.state.take_failures(None);
        if !failures.is_empty()
            && let Some(lease) = &self.lease
        {
            lease.resolve_failures(&failures, Instant::now());
        }
    }
}

pub(in crate::gateway) enum Upstreams {
    Http {
        runtime: Arc<Runtime>,
        candidates: Vec<crate::gateway::runtime::PlannedCandidate>,
        history: Option<Box<super::history::Pending>>,
        binding: Option<BindingObservation>,
    },
    #[cfg(test)]
    Observed {
        runtime: Option<Arc<Runtime>>,
        candidate: Arc<crate::gateway::BoundCandidate>,
        response: reqwest::Response,
    },
}
fn observe_binding(
    runtime: &Runtime,
    entry: &BoundEntry,
    request: &Request,
    trace: &Trace,
) -> Option<BindingObservation> {
    runtime.executors.observe(
        trace.agent_id(),
        entry.public.id.as_str(),
        request
            .conversation
            .as_ref()
            .filter(|conversation| conversation.is_conversation_scoped())
            .map(|conversation| conversation.id()),
        Instant::now(),
    )
}
/// One candidate's prepared request on the wire, with the affinity projection it used.
struct Prepared<'r> {
    wire: crate::execution::UpstreamRequest,
    request: Cow<'r, Request>,
    affinity: Option<crate::gateway::routing::prefix_cache::Selection>,
}
/// Resolve the bound credential, apply an affinity projection when it fits,
/// encode for this candidate and send. Each candidate starts from the original
/// immutable input; affinity never chooses or advances the credential.
#[allow(clippy::too_many_arguments)]
async fn prepare_attempt<'r>(
    runtime: &Runtime,
    lease: Option<&crate::gateway::routing::executors::Lease<'_>>,
    client: &crate::adapter::ClientAdapter,
    planned: crate::gateway::runtime::PlannedCandidate,
    request: &'r Request,
    state: &Status,
    trace: &mut Trace,
) -> Result<Prepared<'r>, ApiError> {
    trace.stage(Stage::Prepare);
    let candidate = planned.candidate.clone();
    let (secret, grant) = candidate.secret.resolve_for_request().await?;
    let revision = grant.as_ref().map(|grant| grant.material_revision);
    state
        .failures
        .lock()
        .expect("failure state")
        .material_revision = revision;
    if let Some(lease) = lease {
        lease.sent_material(revision);
    }
    let report = trace.report.clone();
    let failed = |error: crate::execution::AttemptError| {
        report.failure(
            Some(crate::protocol::conversion::generation::ConversionBoundary::RequestEncode),
            &error,
        );
        ApiError::invalid()
    };
    let mut projection = planned
        .into_projection(request, &runtime.affinity, client)
        .map_err(failed)?;
    let mut local = Cow::Borrowed(request);
    let mut affinity = crate::gateway::routing::prefix_cache::choose(
        &runtime.affinity,
        &candidate,
        request,
        projection.value.prefixes.as_deref(),
    );
    if let Some(selection) = &affinity {
        match selection.project(&candidate, request, client) {
            Some((request, converted)) => {
                local = Cow::Owned(request);
                projection = converted;
            }
            None => {
                // A hint cannot invalidate the already accepted original body.
                affinity = None;
            }
        }
    }
    // Test-only intake changes invalidate the body, not downstream delivery intent.
    #[cfg(test)]
    let intake_request = {
        let mut intake = local.clone();
        if runtime.limits.force_upstream_stream {
            use crate::semantic::context::StreamOptions;
            let delivery = &mut intake.to_mut().delivery;
            delivery.stream = Some(true);
            delivery.options = Some(StreamOptions {
                include_usage: Some(true),
                ..Default::default()
            });
            let converted = crate::execution::attempt::RequestProjection::for_client(
                &candidate.endpoint,
                &intake,
                client,
            )
            .map_err(failed)?;
            projection = crate::execution::attempt::encode_projection(
                &candidate.endpoint,
                &intake,
                converted,
            )
            .map_err(failed)?;
        }
        intake
    };
    #[cfg(test)]
    let upstream_request = &intake_request;
    #[cfg(not(test))]
    let upstream_request = &local;
    let conversion = crate::execution::attempt::prepare_retained(
        &candidate.endpoint,
        &candidate.provider,
        &secret,
        upstream_request,
        projection,
    )
    .map_err(failed)?;
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

    Ok(Prepared {
        wire: prepared,
        request: local,
        affinity,
    })
}

async fn usable_candidates(
    runtime: &Runtime,
    candidates: Vec<crate::gateway::runtime::PlannedCandidate>,
) -> Result<Vec<crate::gateway::runtime::PlannedCandidate>, ApiError> {
    let mut usable = Vec::with_capacity(candidates.len());
    let mut unavailable = None;
    for planned in candidates {
        match planned.candidate.secret.availability_for_request().await {
            Ok(revision) => {
                if let Some(revision) = revision {
                    runtime
                        .executors
                        .verified_material(&planned.candidate.health, revision);
                }
                usable.push(planned);
            }
            Err(error) => {
                unavailable = Some(error);
            }
        }
    }
    if usable.is_empty() {
        Err(unavailable.unwrap_or_else(ApiError::upstream))
    } else {
        Ok(usable)
    }
}
async fn select_candidates(
    runtime: &Runtime,
    candidates: Vec<crate::gateway::runtime::PlannedCandidate>,
    binding: Option<&BindingObservation>,
    deadline: Instant,
) -> Result<Vec<crate::gateway::runtime::PlannedCandidate>, ApiError> {
    let mut candidates = usable_candidates(runtime, candidates).await?;
    if let Some(until) = runtime.executors.preferred_wait(
        binding,
        &candidates,
        Instant::now(),
        runtime.limits.max_wait,
    ) {
        tokio::time::sleep_until(until.min(deadline)).await;
        // Re-read lifecycle and material, then recheck health/probe ownership.
        // This request never waits through a second cooldown.
        candidates = usable_candidates(runtime, candidates).await?;
    }
    if Instant::now() >= deadline {
        return Err(ApiError::timeout());
    }
    runtime
        .executors
        .order(binding, &mut candidates, Instant::now());

    Ok(candidates)
}
pub(in crate::gateway) async fn produce_chain(
    source: Upstreams,
    entry: &BoundEntry,
    request: &Request,
    limits: &Limits,
    deadline: Instant,
    lane: &Lane<'_>,
    trace: &mut Trace,
) -> Result<(), ApiError> {
    let state = lane.state;
    let (runtime, candidates, mut observed, history, binding) = match source {
        Upstreams::Http {
            runtime,
            candidates,
            history,
            binding,
        } => {
            let candidates =
                select_candidates(&runtime, candidates, binding.as_ref(), deadline).await?;
            (Some(runtime), candidates, None, history, binding)
        }
        #[cfg(test)]
        Upstreams::Observed {
            runtime,
            candidate,
            response,
        } => {
            let binding = runtime
                .as_ref()
                .and_then(|runtime| observe_binding(runtime, entry, request, trace));
            (
                runtime,
                vec![crate::gateway::runtime::PlannedCandidate {
                    candidate,
                    projection: None,
                }],
                Some(response),
                None,
                binding,
            )
        }
    };
    let retry_capable = observed.is_none();
    let route_targets: Vec<_> = candidates.iter().map(|p| p.candidate.clone()).collect();
    let mut blocked_delay: Option<u64> = None;
    let mut any_blocked = false;
    let mut rate_only = true;
    for planned in candidates {
        let candidate = planned.candidate.clone();
        if Instant::now() >= deadline {
            return Err(ApiError::timeout());
        }
        let lease = if let Some(runtime) = &runtime {
            match runtime.executors.acquire(&candidate, Instant::now()) {
                Ok(lease) => Some(lease),
                Err(observation) => {
                    any_blocked = true;
                    rate_only &= observation.rate_or_quota_only;
                    if let Some(seconds) = observation.retry_after_seconds {
                        blocked_delay = Some(blocked_delay.map_or(seconds, |old| old.min(seconds)));
                    }

                    continue;
                }
            }
        } else {
            None
        };
        // Selection ends here. One lease and one immutable prepared request own
        // both sends; no account, credential material or prefix can be reselected.
        let health = HealthAttempt { lease, state };
        let recovery_budget = runtime.as_ref().map_or(Duration::ZERO, |runtime| {
            Duration::from_millis(runtime.executors.policy(&candidate).recovery_timeout_ms)
        });
        let mut prepared = None;
        let mut planned = Some(planned);
        let mut recovery_deadline = None;
        let mut initial_failure = None;
        let error = 'sends: {
            for ordinal in 1..=2 {
                let mut send_started = false;
                trace.begin_candidate(ordinal);

                trace.executor(candidate.provider.id.as_str());
                trace.named_account(candidate.account.as_deref());
                trace.models(entry.public.id.as_str(), &candidate.endpoint.upstream_model);
                state.waiting_upstream.store(false, Ordering::Release);
                state.recovery_ready.store(false, Ordering::Release);
                let attempt_deadline = deadline.min(
                    Instant::now() + Duration::from_millis(candidate.endpoint.execution.timeout_ms),
                );
                let attempt = async {
                    let upstream = if let Some(response) = observed.take() {
                        response
                    } else {
                        let runtime = runtime.as_ref().ok_or(ApiError::upstream())?;
                        if prepared.is_none() {
                            prepared = Some(
                                prepare_attempt(
                                    runtime,
                                    health.lease.as_ref(),
                                    &entry.client,
                                    planned.take().expect("prepare once"),
                                    request,
                                    state,
                                    trace,
                                )
                                .await?,
                            );
                        }
                        if ordinal == 2 {
                            let available = candidate.secret.availability_for_request().await?;
                            if available
                                != state
                                    .failures
                                    .lock()
                                    .expect("failure state")
                                    .material_revision
                            {
                                return Err(ApiError::new(
                                    axum::http::StatusCode::SERVICE_UNAVAILABLE,
                                    crate::gateway::error::ErrorKind::CredentialUnavailable,
                                ));
                            }
                        }
                        if Instant::now() >= attempt_deadline
                            || recovery_deadline.is_some_and(|until| Instant::now() >= until)
                        {
                            return Err(ApiError::timeout());
                        }
                        let ready = prepared.as_ref().expect("prepared request");

                        trace.stage(Stage::Connect);
                        if let Some(capture) = &trace.capture {
                            capture.upstream_request(&ready.wire, ordinal);
                        }
                        state.waiting_upstream.store(true, Ordering::Release);
                        send_started = true;
                        let response = runtime
                            .transport
                            .send(
                                ready.wire.clone(),
                                attempt_deadline.saturating_duration_since(Instant::now()),
                            )
                            .await
                            .map_err(|fact| {
                                trace.report.failure(
                                    None,
                                    &crate::execution::AttemptError::Transport(fact),
                                );
                                let failure = ApiError::transport_fact(fact);
                                state.observe_interruption(failure);
                                failure
                            })?;
                        state.waiting_upstream.store(false, Ordering::Release);

                        response
                    };
                    let local = prepared
                        .as_ref()
                        .map_or(request, |ready| ready.request.as_ref());
                    let produced = intake::produce_candidate(
                        entry,
                        &candidate,
                        local,
                        request,
                        upstream,
                        limits,
                        lane,
                        ordinal == 1
                            && retry_capable
                            && !recovery_budget.is_zero()
                            && runtime.is_some(),
                        history.as_deref(),
                        trace,
                    )
                    .await?;
                    if let (Some(ready), Some(successor), Some(runtime)) =
                        (&prepared, produced.successor, &runtime)
                        && let Some(selection) = &ready.affinity
                    {
                        selection.publish(
                            &runtime.affinity,
                            &candidate,
                            Some(&successor),
                            &entry.client,
                        );
                    }
                    Ok(produced.completion)
                };
                let (result, expired) = {
                    tokio::pin!(attempt);
                    let recovery = async {
                        let Some(until) = recovery_deadline else {
                            return std::future::pending::<()>().await;
                        };
                        loop {
                            let notified = state.readiness.notified();
                            if state.recovery_ready.load(Ordering::Acquire) {
                                return std::future::pending::<()>().await;
                            }
                            tokio::select! {
                                biased;
                                _ = notified => {}
                                _ = tokio::time::sleep_until(until) => return,
                            }
                        }
                    };
                    tokio::select! {
                        biased;
                        result = &mut attempt => (result, None),
                        _ = recovery => (Err(ApiError::timeout()), Some(crate::execution::attempt::DeadlineReason::Recovery)),
                        _ = tokio::time::sleep_until(attempt_deadline) => (Err(state.timeout_error()), Some(crate::execution::attempt::DeadlineReason::Attempt)),
                    }
                };
                if let Some(reason) = expired {
                    trace
                        .report
                        .failure(None, &crate::execution::AttemptError::Deadline(reason));
                }
                match result {
                    Ok(completion) => {
                        let failure = match completion {
                            intake::Completion::Eligible => {
                                state.take_failures(None);
                                if let Some(lease) = &health.lease {
                                    lease.completed(binding.as_ref(), Instant::now());
                                }
                                None
                            }
                            intake::Completion::Failed(error) => {
                                health.failed(error);
                                Some(error.diagnostic_code())
                            }
                            intake::Completion::Neutral => {
                                if let Some(lease) = &health.lease {
                                    let failures = state.take_failures(None);
                                    lease.resolve_failures(&failures, Instant::now());
                                }
                                None
                            }
                        };
                        trace.outcome(Outcome::Complete);
                        trace.end_candidate(failure, false);
                        return Ok(());
                    }
                    Err(mut error) => {
                        if ordinal == 2
                            && !send_started
                            && error.kind == crate::gateway::error::ErrorKind::CredentialUnavailable
                        {
                            error = initial_failure.expect("retained initial failure");
                        }

                        // Anchor the recovery clock to observation, not error-body cleanup.
                        let observed_at = state
                            .failures
                            .lock()
                            .expect("failure state")
                            .head
                            .as_ref()
                            .map_or_else(Instant::now, |failure| failure.at);
                        let until = (observed_at + recovery_budget).min(deadline);
                        let retry = ordinal == 1
                            && retry_capable
                            && runtime.is_some()
                            && error.recovery.retryable
                            && !recovery_budget.is_zero()
                            && !state.published.load(Ordering::Acquire)
                            && Instant::now() < until;
                        trace.end_candidate(
                            Some(error.diagnostic_code()),
                            retry
                                && error.retry_after.is_none_or(|seconds| {
                                    Duration::from_secs(seconds)
                                        < until.saturating_duration_since(observed_at)
                                }),
                        );
                        if !retry {
                            break 'sends error;
                        }
                        initial_failure = Some(error);
                        state.retain_attempt(error);
                        // The attempt future and response have been dropped before any wait/send.
                        if let Some(seconds) = error.retry_after {
                            let delay = Duration::from_secs(seconds);
                            if delay >= until.saturating_duration_since(observed_at) {
                                break 'sends error;
                            }
                            tokio::time::sleep_until(observed_at + delay).await;
                        }
                        if Instant::now() >= until {
                            break 'sends error;
                        }
                        recovery_deadline = Some(until);
                    }
                }
            }
            unreachable!("two sends are terminal");
        };
        health.failed(error);
        drop(health);
        return Err(downstream_failure(error, runtime.as_deref(), &route_targets, limits).await);
    }

    if any_blocked {
        let mut error = ApiError::new(
            if rate_only {
                axum::http::StatusCode::TOO_MANY_REQUESTS
            } else {
                axum::http::StatusCode::SERVICE_UNAVAILABLE
            },
            if rate_only {
                crate::gateway::error::ErrorKind::RateLimit
            } else {
                crate::gateway::error::ErrorKind::Unavailable
            },
        );
        error.retry_after = blocked_delay.filter(|seconds| {
            *seconds > 0 && Duration::from_secs(*seconds) <= limits.retry_hint_horizon
        });
        Err(error)
    } else {
        Err(ApiError::upstream())
    }
}
/// Health keeps upstream reset evidence; downstream hints describe the route.
async fn downstream_failure(
    mut error: ApiError,
    runtime: Option<&Runtime>,
    candidates: &[Arc<crate::gateway::BoundCandidate>],
    limits: &Limits,
) -> ApiError {
    let Some(runtime) = runtime else { return error };
    error.retry_after = None;
    let mut earliest = None;
    for candidate in candidates {
        if candidate.secret.availability_for_request().await.is_err() {
            continue;
        }
        let Some(blocked) = runtime
            .executors
            .unavailable(&candidate.health, Instant::now())
        else {
            return error;
        };
        if let Some(seconds) = blocked.retry_after_seconds {
            earliest = Some(earliest.map_or(seconds, |old: u64| old.min(seconds)));
        }
    }
    error.retry_after = earliest.filter(|seconds| {
        *seconds > 0 && Duration::from_secs(*seconds) <= limits.retry_hint_horizon
    });
    error
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
            lane,
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
    let binding = observe_binding(&runtime, &entry, &request, &trace);
    let candidates = entry
        .eligible(&request, &runtime.affinity)
        .map_err(|cause| {
            let error = ApiError::invalid();
            use crate::execution::plan::PlanError;
            let code = match cause {
                PlanError::UnknownModel => "plan.unknown_model",
                PlanError::InvalidRequest => "plan.invalid_request",
                PlanError::CandidateLimit => "plan.candidate_limit",
                PlanError::CandidateRejected(reason) => reason.code(),
                PlanError::NoCandidate { rejections } => rejections
                    .first()
                    .map_or("plan.no_candidate", |r| r.reason.code()),
            };
            trace.report.request_failure(code, error.status.as_u16());
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
            binding,
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
#[tokio::test(start_paused = true)]
async fn bound_cooldown_wait_is_single_cancellable_and_deadline_clipped() {
    use crate::gateway::{routing::executors::Scopes, runtime::PlannedCandidate};
    use std::task::Poll;
    for mode in [
        "ready",
        "unbound",
        "disabled",
        "long",
        "wait",
        "probe_race",
        "extended",
        "cancel",
        "deadline",
    ] {
        let gate = crate::gateway::tests::support::gateway(Limits {
            max_wait: if mode == "disabled" {
                Duration::ZERO
            } else {
                Duration::from_secs(10)
            },
            ..Default::default()
        });
        let runtime = &gate.state;
        let a = runtime.entries.values().next().unwrap().candidates[0].clone();
        let mut b = a.as_ref().clone();
        b.health = Scopes::bind(&[0; 32], &b.endpoint, "synthetic-secondary");
        b.endpoint.bound_replay_scope =
            Some(crate::semantic::value::ReplayOrigin::new("synthetic-secondary").unwrap());
        let b = Arc::new(b);
        let now = Instant::now();
        let initial = runtime
            .executors
            .observe(Some("agent"), "model", Some("session"), now)
            .unwrap();
        runtime
            .executors
            .acquire(&a, now)
            .unwrap()
            .completed(Some(&initial), now);
        let late = runtime.executors.acquire(&a, now).unwrap();
        if mode != "ready" {
            runtime.executors.acquire(&a, now).unwrap().failed(
                ApiError::failure(503, b"", Some(if mode == "long" { 11 } else { 5 })),
                now,
                None,
            );
        }
        let binding = (mode != "unbound").then(|| {
            runtime
                .executors
                .observe(Some("agent"), "model", Some("session"), now)
                .unwrap()
        });
        let candidates = [b.clone(), a.clone()]
            .into_iter()
            .map(|candidate| PlannedCandidate {
                candidate,
                projection: None,
            })
            .collect();
        let deadline = now + Duration::from_secs(if mode == "deadline" { 2 } else { 20 });
        {
            let selection = select_candidates(runtime, candidates, binding.as_ref(), deadline);
            tokio::pin!(selection);
            let polled = futures_util::poll!(&mut selection);
            if matches!(mode, "ready" | "unbound" | "disabled" | "long") {
                let Poll::Ready(Ok(selected)) = polled else {
                    panic!("{mode}: unexpected wait")
                };
                assert_eq!(
                    selected[0].candidate.cache_scope(),
                    if mode == "ready" {
                        a.cache_scope()
                    } else {
                        b.cache_scope()
                    }
                );
                assert_eq!(Instant::now(), now);
                continue;
            }
            assert!(polled.is_pending(), "{mode}");
            if mode == "cancel" {
                continue;
            }
            tokio::time::advance(Duration::from_secs(if mode == "deadline" { 2 } else { 5 })).await;
            let competing = (mode == "probe_race")
                .then(|| runtime.executors.acquire(&a, Instant::now()).unwrap());
            if mode == "extended" {
                late.failed(ApiError::failure(503, b"", Some(5)), Instant::now(), None);
            }
            let result = selection.await;
            if mode == "deadline" {
                assert_eq!(result.err().unwrap().diagnostic_code(), "gateway.timeout");
            } else {
                let selected = result.unwrap();
                assert_eq!(
                    selected[0].candidate.cache_scope(),
                    if matches!(mode, "probe_race" | "extended") {
                        b.cache_scope()
                    } else {
                        a.cache_scope()
                    }
                );
                assert_eq!(Instant::now() - now, Duration::from_secs(5));
            }
            drop(competing);
        }
        drop(late);
    }
}

#[cfg(test)]
#[tokio::test]
async fn response_failures_preserve_independent_health_evidence() {
    use crate::{
        gateway::routing::executors::Scopes,
        protocol::openai::{Dialect, GenerationApi, GenerationCodec, TargetContract},
    };
    use futures_util::StreamExt;
    let mut observed = Vec::new();
    for case in [
        "terminal_eof",
        "framing_eof",
        "malformed",
        "request",
        "quota",
    ] {
        let gate = crate::gateway::tests::support::gateway(Limits::default());
        let mut entry = gate.state.entries[&(
            crate::protocol::ClientProtocol::Chat,
            "fixture-model".into(),
        )]
            .as_ref()
            .clone();
        entry.client = crate::adapter::ClientAdapter::openai(
            GenerationCodec::new(GenerationApi::Responses, Dialect::MorphieCore, None),
            TargetContract::full(),
        );
        let mut candidate = entry.candidates[0].as_ref().clone();
        candidate.provider.id = crate::provider::ProviderId::new("chatgpt").unwrap();
        candidate.endpoint.protocol = crate::topology::GenerationProtocol::OpenAiResponses;
        candidate.endpoint.representation =
            GenerationCodec::new(GenerationApi::Responses, Dialect::Siwc, None)
                .contract(&TargetContract::full())
                .into();
        candidate.health = Scopes::bind(&[0; 32], &candidate.endpoint, "synthetic-account");
        let candidate = Arc::new(candidate);
        let mut sibling = candidate.as_ref().clone();
        sibling.endpoint.upstream_model = "synthetic-sibling".into();
        sibling.health = Scopes::bind(&[0; 32], &sibling.endpoint, "synthetic-account");
        let request = entry
            .client
            .decode_client_request(br#"{"model":"fixture-model","input":"hello"}"#)
            .unwrap();
        let mut response = crate::gateway::tests::responses_wire::subscription_response(true);
        let mut events = crate::gateway::tests::responses_wire::response_events(response.clone());
        let read_error = matches!(case, "request" | "quota");
        if read_error {
            response["status"] = serde_json::json!("failed");
            response["error"] = if case == "quota" {
                serde_json::json!({"code":"subscription_sharing_usage_limit_exceeded","message":"synthetic quota"})
            } else {
                serde_json::json!({"code":"invalid_prompt","type":"invalid_request_error","message":"synthetic request"})
            };
            let last = events.last_mut().unwrap();
            last["type"] = serde_json::json!("response.failed");
            last["response"] = response;
        } else {
            events.truncate(1);
        }
        let mut bytes = events
            .iter()
            .map(|event| {
                format!(
                    "event: {}\ndata: {event}\n\n",
                    event["type"].as_str().unwrap()
                )
            })
            .collect::<String>()
            .into_bytes();
        if case == "framing_eof" {
            bytes.extend_from_slice(b"data: {}");
        } else if case == "malformed" {
            // Invalid UTF-8 is rejected at finish, not while feeding this line.
            bytes.extend_from_slice(b"data: \xff");
        }
        let chunks = futures_util::stream::once(std::future::ready(Ok::<_, std::io::Error>(
            bytes::Bytes::from(bytes),
        )))
        .chain(futures_util::stream::iter(read_error.then(|| {
            Err(std::io::Error::new(
                std::io::ErrorKind::ConnectionReset,
                "synthetic read interruption",
            ))
        })));
        let upstream = reqwest::Response::from(
            axum::http::Response::builder()
                .status(200)
                .header("content-type", "text/event-stream")
                .body(reqwest::Body::wrap_stream(chunks))
                .unwrap(),
        );
        let state = Status::default();
        let (tx, _rx) = tokio::sync::mpsc::channel(1);
        let mut trace = Trace::for_request(
            None,
            &axum::extract::Request::new(axum::body::Body::empty()),
        );
        let result = produce_chain(
            Upstreams::Observed {
                runtime: Some(gate.state.clone()),
                candidate: candidate.clone(),
                response: upstream,
            },
            &entry,
            &request,
            &gate.state.limits,
            Instant::now() + Duration::from_secs(3),
            &Lane {
                tx: &tx,
                state: &state,
            },
            &mut trace,
        )
        .await;
        assert!(
            result.is_err(),
            "{case}: a failed source cannot complete delivery"
        );
        if matches!(case, "terminal_eof" | "framing_eof") {
            assert!(
                result.unwrap_err().recovery.class
                    == Some(crate::provider::RecoveryClass::Malformed),
                "EOF alone cannot grant a resend"
            );
        }
        let report = trace.report.attempt();
        assert_eq!(
            report
                .handling_failure
                .or(report.failure)
                .unwrap()
                .cause
                .code(),
            match case {
                "terminal_eof" => "event.eof_before_terminal",
                "framing_eof" => "sse.unexpected_eof",
                "malformed" => "sse.invalid_utf8",
                _ => "transport.interrupted",
            },
            "{case}: reach the intended failure boundary"
        );
        assert!(!state.published.load(Ordering::Acquire), "{case}");
        let now = Instant::now();
        let target_blocked = gate
            .state
            .executors
            .unavailable(&candidate.health, now)
            .is_some();
        let account_blocked = gate
            .state
            .executors
            .unavailable(&sibling.health, now)
            .is_some();
        // Clear an expired account constraint through a different model. An
        // independent target constraint must still require its own probe.
        let later = now + Duration::from_secs(1801);
        let account_probe = gate.state.executors.acquire(&sibling, later).unwrap();
        assert!(account_probe.completed(None, later).0);
        drop(account_probe);
        let target_probe = gate
            .state
            .executors
            .acquire(&candidate, later)
            .unwrap()
            .probing;
        observed.push((case, target_blocked, account_blocked, target_probe));
    }
    assert_eq!(
        observed,
        [
            ("terminal_eof", false, false, false),
            ("framing_eof", false, false, false),
            ("malformed", false, false, false),
            ("request", true, false, true),
            ("quota", true, true, true),
        ]
    );
}

#[cfg(test)]
#[tokio::test(start_paused = true)]
async fn recorded_interruption_survives_cleanup_without_extending_its_cooldown() {
    for cancelled in [false, true] {
        let gate = crate::gateway::tests::support::gateway(Limits::default());
        let entry = gate.state.entries.values().next().unwrap();
        let candidate = &entry.candidates[0];
        let state = Status::default();
        let attempt = HealthAttempt {
            lease: Some(
                gate.state
                    .executors
                    .acquire(candidate, Instant::now())
                    .unwrap(),
            ),
            state: &state,
        };
        state.observe_failure(ApiError::failure(400, b"", None));
        let interruption =
            ApiError::transport_fact(crate::transport::http::TransportError::Interrupted);
        state.observe_interruption(interruption);
        assert_eq!(state.timeout_error(), interruption);
        tokio::time::advance(Duration::from_secs(2)).await;
        if !cancelled {
            attempt.failed(interruption);
        }
        drop(attempt);
        let now = Instant::now();
        assert!(
            gate.state
                .executors
                .unavailable(&candidate.health, now)
                .is_none(),
            "cleanup cannot restart an expired cooldown"
        );
        let probe = gate.state.executors.acquire(candidate, now).unwrap();
        assert!(probe.probing, "the interruption still requires a probe");
        assert_eq!(
            probe
                .failed(ApiError::failure(503, b"", None), now, None)
                .unwrap()
                .retry_after_seconds,
            Some(2),
            "normal resolution and cancellation each advance backoff only once"
        );
    }
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
    state.observe_failure(ApiError::transport_fact(
        crate::transport::http::TransportError::Interrupted,
    ));
    // A downstream cancellation after the error was observed must not erase it.
    drop(attempt);
    assert!(
        gate.state
            .executors
            .unavailable(&candidate.health, Instant::now())
            .is_some()
    );
}
