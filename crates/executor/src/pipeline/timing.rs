use std::convert::Infallible;
use std::time::Duration;

use tokio::time::Instant;
use tower::{Service, ServiceExt};
use vc_interfaces::TransportError;
use vc_model::Exchange;

use super::{Call, Retried};

/// What the pipeline returns for one call.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Delivery {
    /// How the last attempt ended.
    pub result: Result<Exchange, TransportError>,
    /// Attempts made, at least one.
    pub attempts: u32,
    /// Wall time for the whole call: every attempt, the waits between them and for permits.
    pub elapsed: Duration,
}

/// The outermost service: times the whole call, retries included.
#[derive(Clone, Debug)]
pub struct Timing<S> {
    inner: S,
}

impl<S> Timing<S> {
    /// Times calls sent through `inner`.
    pub const fn new(inner: S) -> Self {
        Self { inner }
    }
}

impl<S> Service<Call> for Timing<S>
where
    S: Service<Call, Response = Retried, Error = Infallible, Future: Send> + Clone + Send + 'static,
{
    ready_service!(Delivery, Infallible);

    fn call(&mut self, call: Call) -> Self::Future {
        let inner = self.inner.clone();
        Box::pin(async move {
            let started = Instant::now();
            let Retried { result, attempts } = inner.oneshot(call).await?;
            Ok(Delivery { result, attempts, elapsed: started.elapsed() })
        })
    }
}
