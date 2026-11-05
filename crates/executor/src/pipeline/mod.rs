//! The request pipeline (D141): `tower` services around the transport, outermost first:
//! timing → retry → auth → `beforeRequest` hooks → per-host limit → timeout → transport.
//!
//! Auth and hooks sit inside retry, so a retried request is prepared again. The timeout sits
//! inside retry and inside the per-host limit, so it limits each attempt and waiting for a
//! permit does not count. Until M3 auth is an [`Unchanged`] null object; the call executor sets
//! the hooks stage of each call from its api's `beforeRequest` hooks.

/// The associated types and `poll_ready` every pipeline service shares: it is always ready and
/// returns a boxed future. With no arguments, the types of a [`Hop`]: one attempt, ending in an
/// exchange or a transport error.
macro_rules! ready_service {
    () => {
        ready_service!(vc_model::Exchange, vc_interfaces::TransportError);
    };
    ($response:ty, $error:ty) => {
        type Response = $response;
        type Error = $error;
        type Future = $crate::BoxFuture<Result<$response, $error>>;

        fn poll_ready(&mut self, _: &mut std::task::Context<'_>) -> std::task::Poll<Result<(), $error>> {
            std::task::Poll::Ready(Ok(()))
        }
    };
}

mod host_limit;
mod retry;
mod stage;
mod timeout;
mod timing;
mod transmit;

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use tower::Service;
use vc_interfaces::{HookRunner, SendContext, Transport, TransportError};
use vc_model::{Exchange, RenderedRequest};

pub use host_limit::{HostLimit, HostLimits};
pub use retry::{Attempted, HttpRetryPolicy, Jittered, NoRetry, Retried, Retry, RetryOn, RetryPolicy};
pub use stage::{RequestStage, Stage, Unchanged};
pub use timeout::Timeout;
pub use timing::{Delivery, Timing};
pub use transmit::Transmit;

/// One request through the pipeline, with its per-attempt timeout and transport settings.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Call {
    /// The request as rendered from its template, before auth and hooks.
    pub request: RenderedRequest,
    /// The limit on each attempt (`timeout`, default 30 s).
    pub timeout: Duration,
    /// What the transport needs, such as the body cap.
    pub ctx: SendContext,
}

/// A boxed future, as every pipeline service returns.
pub type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send>>;

/// A service inside retry: it sends one attempt and returns the exchange or its error.
pub trait Hop:
    Service<Call, Response = Exchange, Error = TransportError, Future: Send> + Clone + Send + 'static
{
}

impl<S> Hop for S where
    S: Service<Call, Response = Exchange, Error = TransportError, Future: Send> + Clone + Send + 'static
{
}

/// The whole pipeline, as [`pipeline`] builds it.
pub type Pipeline = Timing<Retry<Stage<Stage<HostLimit<Timeout<Transmit>>>>>>;

/// What the pipeline is built from. [`PipelineParts::new`] fills in the null objects.
#[derive(Clone)]
#[non_exhaustive]
pub struct PipelineParts {
    /// Sends each attempt.
    pub transport: Arc<dyn Transport>,
    /// The per-host semaphores, shared by every pipeline of a run.
    pub limits: HostLimits,
    /// Decides whether a failed attempt is retried; the call executor sets it per api from
    /// `http.retry`.
    pub retry: Arc<dyn RetryPolicy>,
    /// Adds credentials (M3).
    pub auth: Arc<dyn RequestStage>,
    /// Runs `beforeRequest` hooks; the call executor sets it per call from the api's hooks.
    pub hooks: Arc<dyn RequestStage>,
    /// Runs hooks (D172): request hooks, custom checks and `function:` actions. With none, any
    /// hook that runs fails with V5115.
    pub hook_runner: Option<Arc<dyn HookRunner>>,
}

impl PipelineParts {
    /// Parts with no retry, no auth, no hooks and no hook runner.
    #[must_use]
    pub fn new(transport: Arc<dyn Transport>, limits: HostLimits) -> Self {
        let (retry, auth, hooks) = (Arc::new(NoRetry), Arc::new(Unchanged), Arc::new(Unchanged));
        Self { transport, limits, retry, auth, hooks, hook_runner: None }
    }

    /// These parts, running hooks with `runner`.
    #[must_use]
    pub fn with_hook_runner(self, runner: Arc<dyn HookRunner>) -> Self {
        Self { hook_runner: Some(runner), ..self }
    }
}

/// Builds the pipeline in D141 order.
#[must_use]
pub fn pipeline(parts: PipelineParts) -> Pipeline {
    let attempt = HostLimit::new(Timeout::new(Transmit::new(parts.transport)), parts.limits);
    let prepared = Stage::new(Stage::new(attempt, parts.hooks), parts.auth);
    Timing::new(Retry::new(prepared, parts.retry))
}

#[cfg(test)]
#[path = "pipeline_tests.rs"]
mod tests;
