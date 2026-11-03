use std::task::{Context, Poll};

use tower::{Service, ServiceExt};
use vc_interfaces::TransportError;
use vc_model::Exchange;

use super::{BoxFuture, Call, Hop};

/// Cuts an attempt off after [`Call::timeout`] with [`TransportError::Timeout`] (V5104).
#[derive(Clone, Debug)]
pub struct Timeout<S> {
    inner: S,
}

impl<S> Timeout<S> {
    /// Limits each attempt sent through `inner`.
    pub const fn new(inner: S) -> Self {
        Self { inner }
    }
}

impl<S: Hop> Service<Call> for Timeout<S> {
    type Response = Exchange;
    type Error = TransportError;
    type Future = BoxFuture<Result<Exchange, TransportError>>;

    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), TransportError>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, call: Call) -> Self::Future {
        let inner = self.inner.clone();
        Box::pin(async move {
            let after = call.timeout;
            let attempt = tokio::time::timeout(after, inner.oneshot(call)).await;
            attempt.unwrap_or(Err(TransportError::Timeout { after }))
        })
    }
}
