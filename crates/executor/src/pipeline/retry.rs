use std::convert::Infallible;
use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use http::Method;
use tower::{Service, ServiceExt};
use vc_interfaces::{Random, TransportError};
use vc_model::{Exchange, RenderedRequest, StatusPattern};

use super::{Call, Hop};

/// One finished attempt, as a [`RetryPolicy`] sees it.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct Attempted<'a> {
    /// The request as rendered (before auth and hooks).
    pub request: &'a RenderedRequest,
    /// How the attempt ended.
    pub result: &'a Result<Exchange, TransportError>,
    /// Retries already made before this attempt.
    pub retries: u32,
}

/// Decides whether a finished attempt is tried again (`http.retry`, D143).
pub trait RetryPolicy: Send + Sync {
    /// How long to wait before the next attempt, or `None` to keep this result.
    fn delay(&self, attempt: &Attempted<'_>) -> Option<Duration>;
}

/// The null policy: every result is final.
#[derive(Clone, Copy, Debug, Default)]
#[non_exhaustive]
pub struct NoRetry;

impl RetryPolicy for NoRetry {
    fn delay(&self, _: &Attempted<'_>) -> Option<Duration> {
        None
    }
}

/// What makes an attempt worth retrying.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum RetryOn {
    /// The connection could not be opened (V5105 `Connect`).
    Connect,
    /// The attempt timed out (V5104).
    Timeout,
    /// The response status matched this code or class (`503`, `5xx`).
    Status(StatusPattern),
}

impl RetryOn {
    fn matches(self, result: &Result<Exchange, TransportError>) -> bool {
        match (self, result) {
            (Self::Connect, Err(TransportError::Connect { .. }))
            | (Self::Timeout, Err(TransportError::Timeout { .. })) => true,
            (Self::Status(pattern), Ok(exchange)) => pattern.matches(exchange.response.status.as_u16()),
            _ => false,
        }
    }
}

/// The `http.retry` policy (D143): up to `attempts` retries of an idempotent request on a
/// connect failure, a timeout or a listed status, waiting `base`, `2 × base`, `4 × base`, …
/// up to `max` between them.
#[derive(Clone, Debug, PartialEq, Eq)]
#[expect(clippy::exhaustive_structs, reason = "options struct filled with `..Default::default()` (D160)")]
pub struct HttpRetryPolicy {
    /// Retries after the first attempt; 0 turns retry off.
    pub attempts: u32,
    /// What is retried.
    pub on: Vec<RetryOn>,
    /// The methods that may be retried.
    pub methods: Vec<Method>,
    /// The first wait.
    pub base: Duration,
    /// The longest wait.
    pub max: Duration,
}

impl Default for HttpRetryPolicy {
    fn default() -> Self {
        let statuses = [502, 503, 504].map(StatusPattern::Code);
        Self {
            attempts: 0,
            on: [RetryOn::Connect, RetryOn::Timeout].into_iter().chain(statuses.map(RetryOn::Status)).collect(),
            methods: vec![Method::GET, Method::HEAD, Method::OPTIONS, Method::PUT, Method::DELETE],
            base: Duration::from_millis(100),
            max: Duration::from_secs(2),
        }
    }
}

impl HttpRetryPolicy {
    fn backoff(&self, retries: u32) -> Duration {
        let factor = 1_u32.checked_shl(retries).unwrap_or(u32::MAX);
        self.base.saturating_mul(factor).min(self.max)
    }
}

impl RetryPolicy for HttpRetryPolicy {
    fn delay(&self, attempt: &Attempted<'_>) -> Option<Duration> {
        let wanted = attempt.retries < self.attempts
            && self.methods.contains(&attempt.request.method)
            && self.on.iter().any(|on| on.matches(attempt.result));
        wanted.then(|| self.backoff(attempt.retries))
    }
}

/// Spreads another policy's waits (`backoff.jitter`, D143).
///
/// Each wait is drawn evenly between half of it and all of it from the run's [`Random`], so
/// calls that failed together do not all retry at the same moment, and `--seed` repeats the
/// draws.
#[derive(Clone)]
pub struct Jittered {
    inner: Arc<dyn RetryPolicy>,
    random: Arc<dyn Random>,
}

impl Jittered {
    /// `inner`'s waits, spread with draws from `random`.
    #[must_use]
    pub fn new(inner: Arc<dyn RetryPolicy>, random: Arc<dyn Random>) -> Self {
        Self { inner, random }
    }
}

impl fmt::Debug for Jittered {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Jittered").finish_non_exhaustive()
    }
}

impl RetryPolicy for Jittered {
    fn delay(&self, attempt: &Attempted<'_>) -> Option<Duration> {
        let wait = self.inner.delay(attempt)?;
        Some(spread(wait, self.random.next_u64()))
    }
}

/// A wait between half of `wait` and all of it, picked by `draw`.
fn spread(wait: Duration, draw: u64) -> Duration {
    let half = wait / 2;
    let rest = u64::try_from(wait.saturating_sub(half).as_nanos()).unwrap_or(u64::MAX);
    let extra = draw.checked_rem(rest.saturating_add(1)).unwrap_or_default();
    half.saturating_add(Duration::from_nanos(extra))
}

/// The last attempt's result and how many attempts were made.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Retried {
    /// How the last attempt ended.
    pub result: Result<Exchange, TransportError>,
    /// Attempts made, at least one.
    pub attempts: u32,
}

/// Sends each call through `inner` until the policy keeps a result.
#[derive(Clone)]
pub struct Retry<S> {
    inner: S,
    policy: Arc<dyn RetryPolicy>,
}

impl<S> Retry<S> {
    /// Retries attempts sent through `inner` as `policy` says.
    pub fn new(inner: S, policy: Arc<dyn RetryPolicy>) -> Self {
        Self { inner, policy }
    }
}

impl<S: fmt::Debug> fmt::Debug for Retry<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Retry").field("inner", &self.inner).finish_non_exhaustive()
    }
}

impl<S: Hop> Service<Call> for Retry<S> {
    ready_service!(Retried, Infallible);

    fn call(&mut self, call: Call) -> Self::Future {
        let (inner, policy) = (self.inner.clone(), Arc::clone(&self.policy));
        Box::pin(async move { Ok(attempt_until_kept(inner, policy.as_ref(), call).await) })
    }
}

async fn attempt_until_kept<S: Hop>(inner: S, policy: &dyn RetryPolicy, call: Call) -> Retried {
    let mut retries = 0;
    loop {
        let result = inner.clone().oneshot(call.clone()).await;
        match policy.delay(&Attempted { request: &call.request, result: &result, retries }) {
            Some(wait) => tokio::time::sleep(wait).await,
            None => return Retried { result, attempts: retries + 1 },
        }
        retries += 1;
    }
}

#[cfg(test)]
#[path = "retry_tests.rs"]
mod tests;
