use std::sync::Arc;

use rustls::pki_types::{CertificateDer, ServerName};
use rustls::{ClientConfig, RootCertStore};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;
use tokio_rustls::client::TlsStream;

use crate::HttpAdapterError;

/// The ALPN protocols offered, best first: HTTP/2, then HTTP/1.1.
const ALPN: [&[u8]; 2] = [b"h2", b"http/1.1"];

/// The client TLS setup: ring as the crypto provider, the OS certificate store (so corporate CAs
/// work) plus `extra_roots` (DER), the webpki verifier, and ALPN for h2 and HTTP/1.1.
/// Verification is always on.
pub fn client_config(extra_roots: &[Vec<u8>]) -> Result<Arc<ClientConfig>, HttpAdapterError> {
    let mut roots = RootCertStore::empty();
    roots.add_parsable_certificates(rustls_native_certs::load_native_certs().certs);
    for (index, der) in extra_roots.iter().enumerate() {
        let added = roots.add(CertificateDer::from(der.as_slice()));
        added.map_err(|error| HttpAdapterError::Certificate { index, reason: error.to_string() })?;
    }
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let builder = ClientConfig::builder_with_provider(provider).with_safe_default_protocol_versions();
    let builder = builder.map_err(|error| HttpAdapterError::Certificate { index: 0, reason: error.to_string() })?;
    let mut config = builder.with_root_certificates(roots).with_no_client_auth();
    config.alpn_protocols = ALPN.iter().map(|protocol| protocol.to_vec()).collect();
    Ok(Arc::new(config))
}

/// Runs the TLS handshake for `host` over `stream`.
pub async fn handshake(
    config: Arc<ClientConfig>,
    stream: TcpStream,
    host: &str,
) -> Result<TlsStream<TcpStream>, HttpAdapterError> {
    let fail = |reason: String| HttpAdapterError::Tls { host: host.to_owned(), reason };
    let name = ServerName::try_from(host.to_owned()).map_err(|error| fail(error.to_string()))?;
    TlsConnector::from(config).connect(name, stream).await.map_err(|error| fail(error.to_string()))
}

/// `true` when the server picked HTTP/2 by ALPN.
pub fn negotiated_h2(stream: &TlsStream<TcpStream>) -> bool {
    stream.get_ref().1.alpn_protocol() == Some(ALPN[0])
}

#[cfg(test)]
mod tests {
    use super::{ALPN, client_config};
    use crate::HttpAdapterError;

    #[test]
    fn offers_h2_then_http1_and_loads_the_os_roots() {
        let config = client_config(&[]).unwrap();
        assert_eq!(config.alpn_protocols, ALPN.map(<[u8]>::to_vec));
    }

    #[test]
    fn rejects_an_extra_root_that_is_not_a_certificate() {
        let error = client_config(&[b"not a certificate".to_vec()]).err();
        assert!(matches!(error, Some(HttpAdapterError::Certificate { index: 0, .. })), "{error:?}");
    }
}
