use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};

use async_trait::async_trait;
use vc_interfaces::{SendContext, Transport, TransportError};
use vc_model::{Exchange, RenderedRequest};

/// One request a [`RecordingTransport`] passed on, and what came back.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Recorded {
    /// The request as given.
    pub request: RenderedRequest,
    /// The inner transport's answer.
    pub outcome: Result<Exchange, TransportError>,
}

/// Wraps a transport and records every request and outcome (a decorator).
///
/// Records go down a channel to the [`Recordings`] handed out by [`RecordingTransport::new`],
/// so the transport is shared without a lock.
#[derive(Clone)]
pub struct RecordingTransport {
    inner: Arc<dyn Transport>,
    records: Sender<Recorded>,
}

/// The receiving end of a [`RecordingTransport`].
#[derive(Debug)]
pub struct Recordings(Receiver<Recorded>);

impl Recordings {
    /// Everything recorded since the last call, oldest first.
    #[must_use]
    pub fn take(&self) -> Vec<Recorded> {
        self.0.try_iter().collect()
    }
}

impl std::fmt::Debug for RecordingTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RecordingTransport").finish_non_exhaustive()
    }
}

impl RecordingTransport {
    /// Records what passes through `inner`.
    #[must_use]
    pub fn new(inner: Arc<dyn Transport>) -> (Self, Recordings) {
        let (records, inbox) = mpsc::channel();
        (Self { inner, records }, Recordings(inbox))
    }
}

#[async_trait]
impl Transport for RecordingTransport {
    async fn send(&self, request: RenderedRequest, ctx: &SendContext) -> Result<Exchange, TransportError> {
        let outcome = self.inner.send(request.clone(), ctx).await;
        self.records.send(Recorded { request, outcome: outcome.clone() }).ok();
        outcome
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use http::{Method, StatusCode, Uri};
    use vc_interfaces::{SendContext, Transport, TransportError};
    use vc_model::RenderedRequest;

    use super::RecordingTransport;
    use crate::{FakeTransport, status, transport_suite};

    #[tokio::test]
    async fn passes_the_transport_suite_around_the_echo_fake() {
        let (recording, _recordings) = RecordingTransport::new(Arc::new(FakeTransport::echo()));
        transport_suite(&recording, "http://fake.test").await;
    }

    #[tokio::test]
    async fn records_requests_and_outcomes_in_order() {
        let reset = TransportError::Protocol { reason: "reset".into() };
        let fake = FakeTransport::scripted(vec![Ok(status(204)), Err(reset.clone())]);
        let (recording, recordings) = RecordingTransport::new(Arc::new(fake));
        let request = RenderedRequest::new(Method::DELETE, Uri::from_static("http://h/users/1"));
        let ctx = SendContext::default();
        recording.send(request.clone(), &ctx).await.ok();
        recording.clone().send(request.clone(), &ctx).await.ok();
        let records = recordings.take();
        assert_eq!(records.len(), 2);
        assert!(records.iter().all(|r| r.request == request));
        assert_eq!(records[0].outcome.as_ref().map(|e| e.response.status).ok(), Some(StatusCode::NO_CONTENT));
        assert_eq!(records[1].outcome.as_ref().err(), Some(&reset));
        assert!(recordings.take().is_empty(), "take drains");
    }
}
