use vc_interfaces::TransportError;

/// Why the HTTP transport could not be built or a connection could not be opened.
///
/// The connect variants travel through hyper as its connector error and come back out as a
/// [`TransportError`] with the same meaning.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum HttpAdapterError {
    /// An extra root certificate is not a valid DER certificate.
    #[error("extra root certificate {index} is invalid: {reason}")]
    Certificate {
        /// Its position in the list.
        index: usize,
        /// What rustls said.
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
            HttpAdapterError::Certificate { .. } | HttpAdapterError::NoHost(_) => {
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

#[cfg(test)]
mod tests {
    use vc_interfaces::TransportError;

    use super::HttpAdapterError;

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
}
