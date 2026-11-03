use std::time::Duration;

use async_trait::async_trait;
use vc_diagnostics::Code;
use vc_model::{Exchange, RenderedRequest, Size};

/// Sends one rendered request and returns the exchange with its timings.
///
/// It knows nothing of auth, retries, timeouts or per-host limits: those are layers of the
/// executor's request pipeline around it (D141). The HTTP adapter implements it; `vc-testkit`
/// has fakes, and every implementation runs `transport_suite` (D112).
#[async_trait]
pub trait Transport: Send + Sync {
    /// Sends `request` and reads the response body, decoded and capped at `ctx.body_limit`.
    ///
    /// Any status, 5xx included, is a successful exchange; checks decide what passes.
    ///
    /// # Errors
    ///
    /// A [`TransportError`] when no complete response arrived.
    async fn send(&self, request: RenderedRequest, ctx: &SendContext) -> Result<Exchange, TransportError>;
}

/// Per-request settings a transport needs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SendContext {
    /// The largest decoded body kept in memory; a larger one is [`TransportError::BodyTooLarge`].
    /// The cap applies after decoding, so a small compressed body cannot expand without bound.
    pub body_limit: Size,
}

impl SendContext {
    /// The in-memory body cap used unless configured: 64 MiB.
    pub const DEFAULT_BODY_LIMIT: Size = Size::from_bytes(64 * 1024 * 1024);
}

impl Default for SendContext {
    fn default() -> Self {
        Self { body_limit: Self::DEFAULT_BODY_LIMIT }
    }
}

/// Why no complete response arrived. Messages name hosts and reasons, never header values.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum TransportError {
    /// The request cannot be sent as given: no host, an unsupported scheme, a bad header.
    #[error("cannot send the request: {reason}")]
    InvalidRequest {
        /// What is wrong with it.
        reason: String,
    },
    /// The host name did not resolve.
    #[error("cannot resolve {host}: {reason}")]
    Dns {
        /// The host name.
        host: String,
        /// What the resolver said.
        reason: String,
    },
    /// No TCP connection could be opened (to the host or to the proxy).
    #[error("cannot connect to {host}: {reason}")]
    Connect {
        /// The host, with its port.
        host: String,
        /// What went wrong.
        reason: String,
    },
    /// The TLS handshake failed, for example an untrusted certificate.
    #[error("TLS handshake with {host} failed: {reason}")]
    Tls {
        /// The host name.
        host: String,
        /// What went wrong.
        reason: String,
    },
    /// The exchange did not finish in time.
    #[error("no response within {}ms", after.as_millis())]
    Timeout {
        /// The limit that was reached.
        after: Duration,
    },
    /// The connection broke or the peer broke the protocol (reset, closed early, bad framing).
    #[error("the exchange failed: {reason}")]
    Protocol {
        /// What went wrong.
        reason: String,
    },
    /// The decoded body is larger than the limit.
    #[error("the response body is larger than {} bytes", limit.bytes())]
    BodyTooLarge {
        /// The limit that was passed.
        limit: Size,
    },
    /// The body could not be decoded with its `content-encoding`.
    #[error("cannot decode the {encoding} body: {reason}")]
    Decode {
        /// The content encoding.
        encoding: String,
        /// What went wrong.
        reason: String,
    },
}

impl TransportError {
    /// The diagnostic code a failed step reports: V5104 for a timeout, V5105 for the rest.
    #[must_use]
    pub const fn code(&self) -> Code {
        match self {
            Self::Timeout { .. } => Code::V5104,
            _ => Code::V5105,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use vc_diagnostics::Code;
    use vc_model::Size;

    use super::{SendContext, TransportError};

    #[test]
    fn a_timeout_is_v5104_and_everything_else_v5105() {
        assert_eq!(TransportError::Timeout { after: Duration::from_millis(5) }.code(), Code::V5104);
        let others = [
            TransportError::InvalidRequest { reason: "r".into() },
            TransportError::Dns { host: "h".into(), reason: "r".into() },
            TransportError::Connect { host: "h".into(), reason: "r".into() },
            TransportError::Tls { host: "h".into(), reason: "r".into() },
            TransportError::Protocol { reason: "r".into() },
            TransportError::BodyTooLarge { limit: Size::from_bytes(1) },
            TransportError::Decode { encoding: "gzip".into(), reason: "r".into() },
        ];
        assert!(others.iter().all(|e| e.code() == Code::V5105));
    }

    #[test]
    fn messages_name_the_limits() {
        assert_eq!(
            TransportError::Timeout { after: Duration::from_millis(250) }.to_string(),
            "no response within 250ms"
        );
        let large = TransportError::BodyTooLarge { limit: Size::from_bytes(10) };
        assert_eq!(large.to_string(), "the response body is larger than 10 bytes");
    }

    #[test]
    fn the_default_body_limit_is_64_mib() {
        assert_eq!(SendContext::default().body_limit.bytes(), 64 * 1024 * 1024);
    }
}
