//! Admission, permit lifetime and the single authorized standalone attempt.
use super::{BoundTask, TaskRoute, exchange};
use crate::{
    ApiError, Limits, Runtime, body, diagnostics::Trace, execution::task::PreparedTaskRequest,
    ingress::admission,
};
use axum::{extract::Request, http::HeaderMap, response::Response};
use std::{future::Future, sync::Arc, time::Duration};
use tokio::{sync::OwnedSemaphorePermit, time::Instant};

pub(super) struct Admission {
    started: Instant,
    permit: OwnedSemaphorePermit,
    trace: Trace,
}

impl Admission {
    pub(super) async fn upload<H>(
        runtime: &Runtime,
        request: Request,
        limit: usize,
        check_head: impl FnOnce(&HeaderMap, usize) -> Result<H, ApiError>,
    ) -> Result<(Self, H, Vec<u8>), ApiError> {
        let started = Instant::now();
        let mut trace = Trace::for_request(runtime.diagnostics.as_ref(), &request);
        let permit = admission::acquire_permit(runtime)?;
        let head = check_head(request.headers(), limit)?;
        let bytes = crate::ingress::http::read_body(
            runtime,
            request.into_body(),
            limit,
            started + runtime.limits.exchange_timeout,
            &mut trace,
        )
        .await?;
        Ok((
            Self {
                started,
                permit,
                trace,
            },
            head,
            bytes,
        ))
    }

    pub(super) async fn respond<T: TaskOutput>(
        self,
        runtime: Arc<Runtime>,
        entry: Arc<BoundTask<T::Route>>,
        prepared: PreparedTaskRequest,
        output: T,
    ) -> Result<Response, ApiError> {
        let (_, _, execution) = entry.route.delivery();
        let deadline = self.started
            + runtime
                .limits
                .exchange_timeout
                .min(Duration::from_millis(execution.timeout_ms));
        if Instant::now() >= deadline {
            return Err(ApiError::timeout());
        }
        let limits = runtime.limits.clone();
        let shutdown = runtime.shutdown.clone();
        body::respond_producer(
            Exchange {
                runtime,
                entry,
                prepared,
                output,
            },
            T::RESPONSE,
            limits,
            deadline,
            shutdown,
            self.permit,
            self.trace,
        )
        .await
    }
}

pub(super) trait TaskOutput: Send + 'static {
    type Route: TaskRoute + Send + Sync + 'static;
    const RESPONSE: body::ResponseType;

    fn receive_limit(
        &self,
        entry: &BoundTask<Self::Route>,
        limits: &Limits,
        wire_limit: usize,
    ) -> usize;
    fn publish(
        self,
        receiver: exchange::Receiver,
        entry: &BoundTask<Self::Route>,
        limits: &Limits,
        lane: &body::Lane<'_>,
        trace: &mut Trace,
    ) -> impl Future<Output = Result<(), ApiError>> + Send;
}

struct Exchange<T: TaskOutput> {
    runtime: Arc<Runtime>,
    entry: Arc<BoundTask<T::Route>>,
    prepared: PreparedTaskRequest,
    output: T,
}

impl<T: TaskOutput> body::Produce for Exchange<T> {
    async fn produce(
        self,
        limits: &Limits,
        deadline: Instant,
        lane: &body::Lane<'_>,
        trace: &mut Trace,
    ) -> Result<(), ApiError> {
        let Self {
            runtime,
            entry,
            prepared,
            output,
        } = self;
        let (model, upstream, execution) = entry.route.delivery();
        trace.begin_candidate(1);
        trace.models(model, upstream);
        let result = async {
            let limit = output.receive_limit(
                &entry,
                limits,
                limits.response_bytes.min(execution.response_body_limit),
            );
            let receiver =
                exchange::send(&runtime, &entry, prepared, deadline, limit, trace).await?;
            output.publish(receiver, &entry, limits, lane, trace).await
        }
        .await;
        trace.end_candidate(result.as_ref().err().map(|e| e.diagnostic_code()), false);
        result
    }
}
