//! Fixed attempt-chain coordination. HTTP body publication and handoff stay in body.
use super::{
    ApiError, BoundCandidate, BoundEntry, Limits, Runtime,
    body::{self, Lane, Message, Status},
    diagnostics::{Stage, Trace},
    intake,
};
use crate::{
    adapter::Request,
    execution::{DeliveryState, fallback::may_advance},
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
pub(super) enum Upstreams {
    Http {
        runtime: Arc<Runtime>,
        candidates: Vec<Arc<BoundCandidate>>,
    },
    #[cfg(test)]
    Observed {
        candidate: Arc<BoundCandidate>,
        response: reqwest::Response,
    },
}
#[allow(clippy::too_many_arguments)]
pub(super) async fn produce_chain(
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
    let (runtime, candidates, mut observed) = match source {
        Upstreams::Http {
            runtime,
            candidates,
        } => (Some(runtime), candidates, None),
        #[cfg(test)]
        Upstreams::Observed {
            candidate,
            response,
        } => (None, vec![candidate], Some(response)),
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
        let attempt_deadline = deadline
            .min(Instant::now() + Duration::from_millis(candidate.endpoint.execution.timeout_ms));
        let result = tokio::time::timeout_at(attempt_deadline, async {
            let upstream = if let Some(response) = observed.take() {
                response
            } else {
                let runtime = runtime.as_ref().ok_or(ApiError::upstream())?;
                trace.stage(Stage::Prepare);
                let (secret, grant) = candidate.secret.resolve()?;
                let mut prepared = crate::execution::prepare(
                    &candidate.endpoint,
                    &candidate.provider,
                    &secret,
                    request,
                )
                .map_err(|_| ApiError::invalid())?;
                if let Some(grant) = grant {
                    prepared.safe_headers.extend(
                        crate::provider::subscription::headers(candidate.provider.auth, &grant)
                            .map_err(|_| ApiError::upstream())?,
                    );
                }
                trace.stage(Stage::Connect);
                runtime
                    .transport
                    .send(
                        prepared,
                        attempt_deadline.saturating_duration_since(Instant::now()),
                    )
                    .await
                    .map_err(ApiError::transport)?
            };
            intake::produce_candidate(entry, candidate, request, upstream, limits, &lane, trace)
                .await
        })
        .await
        .unwrap_or_else(|_| Err(ApiError::timeout()));
        match result {
            Ok(()) => {
                trace.end_candidate(None, false);
                return Ok(());
            }
            Err(error) => {
                let visibility = if state.published.load(Ordering::Acquire) {
                    DeliveryState::Committed
                } else {
                    DeliveryState::Uncommitted
                };
                let advance = index + 1 < candidates.len()
                    && error.fallback.is_some_and(|class| {
                        let next = &candidates[index + 1];
                        let remaining =
                            index + 1 < entry.policy.max_attempts && Instant::now() < deadline;
                        if candidate.endpoint.id == next.endpoint.id {
                            crate::execution::fallback::may_advance_credential(
                                candidate.credential_fallback,
                                visibility,
                                class,
                                remaining,
                            )
                        } else if candidate.provider.id == next.provider.id
                            && class == crate::provider::ErrorClass::RateLimit
                        {
                            false
                        } else {
                            may_advance(&entry.policy, visibility, class, index + 1, remaining)
                        }
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
pub(super) async fn run(
    runtime: Arc<Runtime>,
    entry: Arc<BoundEntry>,
    request: Request,
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
        },
        limits,
        deadline,
        shutdown,
        permit,
        trace,
    )
    .await
}
