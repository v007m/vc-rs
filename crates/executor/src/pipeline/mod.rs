//! The request pipeline (D141): `tower` services around the transport, outermost first:
//! timing → retry → auth → `beforeRequest` hooks → per-host limit → sign → timeout → transport.
//!
//! Auth and hooks sit inside retry, so a retried request is prepared again. The timeout sits
//! inside retry and inside the per-host limit, so it limits each attempt and waiting for a
//! permit does not count. The sign stage sits inside the per-host limit too (D443): a request
//! is signed as it leaves, after every header is on it, so its date is fresh. Without auth,
//! the auth and sign stages are [`Unchanged`] null objects; the call executor sets them, and
//! the hooks stage, for each call.
//!
//! A stream call opens through the same layers with [`OpenStream`] in place of [`Transmit`]
//! (D420): each layer is generic over its [`HasHead`] reply, retry and auth refresh read the
//! opening head, and the per-host permit lives with the open stream.

/// The associated types and `poll_ready` every pipeline service shares: it is always ready and
/// returns a boxed future.
macro_rules! ready_service {
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
mod open_stream;
mod retry;
mod stage;
mod timeout;
mod timing;
mod transmit;

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::OwnedSemaphorePermit;
use tower::Service;
use vc_interfaces::{Clock, HookRunner, SendContext, StreamMode, StreamTransport, Transport, TransportError};
use vc_model::{Exchange, RenderedRequest};

pub use host_limit::{HostLimit, HostLimits};
pub use open_stream::{OpenStream, Opening};
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

/// What one attempt answers: an [`Exchange`], or an [`Opening`] stream with its head.
pub trait HasHead: Send + 'static {
    /// The response head retry and auth refresh read.
    fn head(&self) -> &Exchange;

    /// Keeps the per-host `permit` for as long as the reply is alive; an exchange is done, so
    /// it lets the permit go.
    fn hold(&mut self, permit: OwnedSemaphorePermit) {
        drop(permit);
    }
}

impl HasHead for Exchange {
    fn head(&self) -> &Exchange {
        self
    }
}

/// A service inside retry: it sends one attempt and returns its reply or its error.
pub trait Hop: Service<Call, Response: HasHead, Error = TransportError, Future: Send> + Clone + Send + 'static {}

impl<S> Hop for S where
    S: Service<Call, Response: HasHead, Error = TransportError, Future: Send> + Clone + Send + 'static
{
}

/// The whole pipeline, as [`pipeline`] builds it; [`StreamPipeline`] opens streams instead.
pub type Pipeline<B = Transmit> = Timing<Retry<Stage<Stage<HostLimit<Stage<Timeout<B>>>>>>>;

/// The pipeline a stream call opens through, as [`stream_pipeline`] builds it.
pub type StreamPipeline = Pipeline<OpenStream>;

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
    /// Signs each attempt as it leaves (D443); the call executor sets it per call when its auth
    /// signs requests.
    pub sign: Arc<dyn RequestStage>,
    /// Tells the time a request is signed at. With none, an auth that signs fails with V5114,
    /// as an auth type this host cannot run (D455).
    pub clock: Option<Arc<dyn Clock>>,
    /// Runs hooks (D172): request hooks, custom checks and `function:` actions. With none, any
    /// hook that runs fails with V5115.
    pub hook_runner: Option<Arc<dyn HookRunner>>,
    /// Opens `sse` and `websocket` streams. With none, a stream call fails with a plain message.
    pub streams: Option<Arc<dyn StreamTransport>>,
}

impl PipelineParts {
    /// Parts with no retry, no auth, no hooks, no signing, no clock and no hook runner.
    #[must_use]
    pub fn new(transport: Arc<dyn Transport>, limits: HostLimits) -> Self {
        let (retry, auth, hooks, sign) =
            (Arc::new(NoRetry), Arc::new(Unchanged), Arc::new(Unchanged), Arc::new(Unchanged));
        Self { transport, limits, retry, auth, hooks, sign, clock: None, hook_runner: None, streams: None }
    }

    /// These parts, signing requests at the time `clock` tells.
    #[must_use]
    pub fn with_clock(self, clock: Arc<dyn Clock>) -> Self {
        Self { clock: Some(clock), ..self }
    }

    /// These parts, opening streams through `streams`.
    #[must_use]
    pub fn with_streams(self, streams: Arc<dyn StreamTransport>) -> Self {
        Self { streams: Some(streams), ..self }
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
    let transmit = Transmit::new(Arc::clone(&parts.transport));
    layered(transmit, parts)
}

/// Builds the pipeline in D141 order around `transport`, opening streams in `mode` (D420).
#[must_use]
pub fn stream_pipeline(
    parts: PipelineParts,
    (transport, mode): (Arc<dyn StreamTransport>, StreamMode),
) -> StreamPipeline {
    layered(OpenStream::new(transport, mode), parts)
}

/// The D141 layers around `bottom`, the service that makes each attempt.
fn layered<B: Hop>(bottom: B, parts: PipelineParts) -> Pipeline<B> {
    let attempt = HostLimit::new(Stage::new(Timeout::new(bottom), parts.sign), parts.limits);
    let prepared = Stage::new(Stage::new(attempt, parts.hooks), parts.auth);
    Timing::new(Retry::new(prepared, parts.retry))
}

#[cfg(test)]
#[path = "pipeline_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "stream_pipeline_tests.rs"]
mod stream_tests;
