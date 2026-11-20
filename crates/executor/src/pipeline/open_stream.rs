use std::fmt;
use std::sync::Arc;

use tokio::sync::OwnedSemaphorePermit;
use tower::Service;
use vc_interfaces::{MessageStream, Opened, StreamMode, StreamTransport, TransportError};
use vc_model::Exchange;

use super::{Call, HasHead};

/// The innermost service of a stream call: opens the stream through the [`StreamTransport`]
/// (D420).
#[derive(Clone)]
pub struct OpenStream {
    transport: Arc<dyn StreamTransport>,
    mode: StreamMode,
}

impl OpenStream {
    /// Opens streams read in `mode` through `transport`.
    #[must_use]
    pub fn new(transport: Arc<dyn StreamTransport>, mode: StreamMode) -> Self {
        Self { transport, mode }
    }
}

impl fmt::Debug for OpenStream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OpenStream").field("mode", &self.mode).finish_non_exhaustive()
    }
}

impl Service<Call> for OpenStream {
    ready_service!(Opening, TransportError);

    fn call(&mut self, call: Call) -> Self::Future {
        let (transport, mode) = (Arc::clone(&self.transport), self.mode);
        Box::pin(async move {
            let Opened { head, stream } = transport.open(call.request, mode, &call.ctx).await?;
            Ok(Opening { head, stream, permit: None })
        })
    }
}

/// A stream attempt that answered: its opening head and, when it opened, the stream. It holds
/// the host's permit, so an open stream counts against `limits.perHost` until it is dropped.
#[non_exhaustive]
pub struct Opening {
    /// The opening response.
    pub head: Exchange,
    /// The stream, or `None` when the answer opened none.
    pub stream: Option<Box<dyn MessageStream>>,
    permit: Option<OwnedSemaphorePermit>,
}

impl Opening {
    /// Whether this opening holds a per-host permit.
    #[must_use]
    pub const fn holds_permit(&self) -> bool {
        self.permit.is_some()
    }
}

impl HasHead for Opening {
    fn head(&self) -> &Exchange {
        &self.head
    }

    fn hold(&mut self, permit: OwnedSemaphorePermit) {
        self.permit = Some(permit);
    }
}

impl fmt::Debug for Opening {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let opened = self.stream.is_some();
        f.debug_struct("Opening").field("head", &self.head).field("stream", &opened).finish_non_exhaustive()
    }
}
