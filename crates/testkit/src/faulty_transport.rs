use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use vc_interfaces::{SendContext, Transport, TransportError};
use vc_model::{Exchange, RenderedRequest};

/// What a [`FaultyTransport`] does to one request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Fault {
    /// Passes the request on untouched.
    Pass,
    /// Fails with this error without sending.
    Fail(TransportError),
    /// Waits this long, then passes the request on.
    Delay(Duration),
}

/// Wraps a transport and injects failures and delays (a decorator).
///
/// The `n`th request (counting from 0) gets `faults[n]`; requests past the end pass. The
/// position is an atomic, so clones share it without a lock.
#[derive(Clone)]
pub struct FaultyTransport {
    inner: Arc<dyn Transport>,
    faults: Arc<[Fault]>,
    calls: Arc<AtomicUsize>,
}

impl std::fmt::Debug for FaultyTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FaultyTransport").field("faults", &self.faults).finish_non_exhaustive()
    }
}

impl FaultyTransport {
    /// Applies `faults` in order to requests sent through `inner`.
    #[must_use]
    pub fn new(inner: Arc<dyn Transport>, faults: Vec<Fault>) -> Self {
        Self { inner, faults: faults.into(), calls: Arc::new(AtomicUsize::new(0)) }
    }
}

#[async_trait]
impl Transport for FaultyTransport {
    async fn send(&self, request: RenderedRequest, ctx: &SendContext) -> Result<Exchange, TransportError> {
        let call = self.calls.fetch_add(1, Ordering::SeqCst);
        match self.faults.get(call) {
            Some(Fault::Fail(error)) => return Err(error.clone()),
            Some(Fault::Delay(wait)) => tokio::time::sleep(*wait).await,
            Some(Fault::Pass) | None => {}
        }
        self.inner.send(request, ctx).await
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use http::{Method, Uri};
    use vc_interfaces::{SendContext, Transport, TransportError};
    use vc_model::RenderedRequest;

    use super::{Fault, FaultyTransport};
    use crate::{FakeTransport, transport_suite};

    fn get() -> RenderedRequest {
        RenderedRequest::new(Method::GET, Uri::from_static("http://fake.test/echo"))
    }

    #[tokio::test]
    async fn passes_the_transport_suite_with_no_faults() {
        let faulty = FaultyTransport::new(Arc::new(FakeTransport::echo()), Vec::new());
        transport_suite(&faulty, "http://fake.test").await;
    }

    #[tokio::test(start_paused = true)]
    async fn fails_delays_and_passes_in_script_order() {
        let fake = FakeTransport::echo();
        let refused = TransportError::Connect { host: "fake.test:80".into(), reason: "refused".into() };
        let faults = vec![Fault::Fail(refused.clone()), Fault::Delay(Duration::from_secs(5)), Fault::Pass];
        let transport = FaultyTransport::new(Arc::new(fake.clone()), faults);
        let ctx = SendContext::default();
        assert_eq!(transport.send(get(), &ctx).await.err(), Some(refused));
        assert_eq!(fake.calls(), 0, "a failure never reaches the inner transport");
        let started = tokio::time::Instant::now();
        assert!(transport.clone().send(get(), &ctx).await.is_ok());
        assert!(started.elapsed() >= Duration::from_secs(5));
        assert!(transport.send(get(), &ctx).await.is_ok());
        assert!(transport.send(get(), &ctx).await.is_ok(), "past the script every request passes");
        assert_eq!(fake.calls(), 3);
    }
}
