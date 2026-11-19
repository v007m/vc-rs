use std::error::Error;
use std::io;

use vc_interfaces::TransportError;

/// Why the HTTP transport could not be built or a connection could not be opened.
///
/// The connect variants travel through hyper wrapped in a [`ConnectError`], the connector's
/// error, and come back out as a [`TransportError`] with the same meaning.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum HttpAdapterError {
    /// An extra root certificate is not a valid DER certificate.
    #[error("extra root certificate {index} is invalid: {reason}")]
    Certificate {
        /// Its position in the list.
        index: usize,
        /// What rustls said.
        reason: String,
    },
    /// A client identity cannot be used. The reason never quotes the PEM text.
    #[error("client identity {id}: {reason}")]
    Identity {
        /// The identity's ID.
        id: String,
        /// What is wrong with it.
        reason: String,
    },
    /// The URL has no host.
    #[error("the URL {0} has no host")]
    NoHost(String),
    /// The host name did not resolve.
    #[error("cannot resolve {host}: {reason}")]
    Dns {
        /// The host name.
        host: String,
        /// What the resolver said.
        reason: String,
    },
    /// No TCP connection could be opened.
    #[error("cannot connect to {host}: {reason}")]
    Connect {
        /// The host and port.
        host: String,
        /// What went wrong.
        reason: String,
    },
    /// The proxy refused or broke the CONNECT tunnel.
    #[error("the proxy refused a tunnel to {host}: {reason}")]
    Tunnel {
        /// The target host and port.
        host: String,
        /// What went wrong.
        reason: String,
    },
    /// The TLS handshake failed.
    #[error("TLS handshake with {host} failed: {reason}")]
    Tls {
        /// The host name.
        host: String,
        /// What went wrong.
        reason: String,
    },
}

impl From<HttpAdapterError> for TransportError {
    fn from(error: HttpAdapterError) -> Self {
        match error {
            HttpAdapterError::Certificate { .. } | HttpAdapterError::Identity { .. } | HttpAdapterError::NoHost(_) => {
                Self::InvalidRequest { reason: error.to_string() }
            }
            HttpAdapterError::Dns { host, reason } => Self::Dns { host, reason },
            HttpAdapterError::Connect { host, reason } | HttpAdapterError::Tunnel { host, reason } => {
                Self::Connect { host, reason }
            }
            HttpAdapterError::Tls { host, reason } => Self::Tls { host, reason },
        }
    }
}

/// Why the connector could not open a connection. A failed TLS handshake keeps the I/O error
/// tokio-rustls returns, so the rustls error inside still tells which alert the server sent.
#[derive(Debug, thiserror::Error)]
pub enum ConnectError {
    /// Every failure but the handshake's own.
    #[error(transparent)]
    Adapter(#[from] HttpAdapterError),
    /// The TLS handshake failed.
    #[error("TLS handshake failed: {0}")]
    Handshake(#[source] io::Error),
}

/// `error` and its causes, outermost first.
pub fn causes<'a>(error: &'a (dyn Error + 'static)) -> impl Iterator<Item = &'a (dyn Error + 'static)> {
    std::iter::successors(Some(error), |&cause| cause.source())
}

/// `error` and its causes, joined with `: `.
pub fn chain(error: &(dyn Error + 'static)) -> String {
    causes(error).map(ToString::to_string).collect::<Vec<_>>().join(": ")
}

#[cfg(test)]
mod tests {
    use std::io;

    use vc_interfaces::TransportError;

    use super::{ConnectError, HttpAdapterError, chain};

    fn pair(host: &str) -> (String, String) {
        (host.to_owned(), "why".to_owned())
    }

    #[test]
    fn connect_errors_keep_their_meaning_as_transport_errors() {
        let (host, reason) = pair("h");
        let dns = HttpAdapterError::Dns { host: host.clone(), reason: reason.clone() };
        assert_eq!(TransportError::from(dns), TransportError::Dns { host: host.clone(), reason: reason.clone() });
        let tls = HttpAdapterError::Tls { host: host.clone(), reason: reason.clone() };
        assert_eq!(TransportError::from(tls), TransportError::Tls { host: host.clone(), reason: reason.clone() });
        let tunnel = HttpAdapterError::Tunnel { host: host.clone(), reason: reason.clone() };
        assert_eq!(
            TransportError::from(tunnel),
            TransportError::Connect { host: host.clone(), reason: reason.clone() }
        );
        let connect = HttpAdapterError::Connect { host: host.clone(), reason: reason.clone() };
        assert_eq!(TransportError::from(connect), TransportError::Connect { host, reason });
    }

    #[test]
    fn a_missing_host_is_an_invalid_request() {
        let error = TransportError::from(HttpAdapterError::NoHost("/x".into()));
        assert_eq!(error, TransportError::InvalidRequest { reason: "the URL /x has no host".into() });
    }

    #[test]
    fn a_chain_joins_the_error_and_each_cause_outermost_first() {
        let failed = ConnectError::Handshake(io::Error::other("reset"));
        assert_eq!(chain(&failed), "TLS handshake failed: reset: reset");
        assert_eq!(chain(&io::Error::other("alone")), "alone");
    }
}
