use std::fmt;
use std::sync::Arc;
use std::task::{Context, Poll};

use tower::Service;
use vc_interfaces::{Transport, TransportError};
use vc_model::Exchange;

use super::{BoxFuture, Call};

/// The innermost service: hands the attempt to the [`Transport`].
#[derive(Clone)]
pub struct Transmit {
    transport: Arc<dyn Transport>,
}

impl Transmit {
    /// Sends through `transport`.
    #[must_use]
    pub fn new(transport: Arc<dyn Transport>) -> Self {
        Self { transport }
    }
}

impl fmt::Debug for Transmit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Transmit").finish_non_exhaustive()
    }
}

impl Service<Call> for Transmit {
    type Response = Exchange;
    type Error = TransportError;
    type Future = BoxFuture<Result<Exchange, TransportError>>;

    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), TransportError>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, call: Call) -> Self::Future {
        let transport = Arc::clone(&self.transport);
        Box::pin(async move { transport.send(call.request, &call.ctx).await })
    }
}
