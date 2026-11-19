use std::sync::Arc;

use rustls::pki_types::CertificateDer;
use rustls::pki_types::ServerName;
use rustls::pki_types::pem::PemObject;
use rustls::{ClientConfig, RootCertStore};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;
use tokio_rustls::client::TlsStream;

use crate::{ClientIdentity, HttpAdapterError};

/// The ALPN protocols offered, best first: HTTP/2, then HTTP/1.1.
const ALPN: [&[u8]; 2] = [b"h2", b"http/1.1"];

/// Marks a `ca` entry as PEM, wherever it starts: bundles may open with comments (curl's
/// `cacert.pem`) or `Bag Attributes` (`openssl pkcs12 -nokeys`).
const PEM_CERTIFICATE: &[u8] = b"-----BEGIN CERTIFICATE-----";

/// The trusted roots: the OS certificate store (so corporate CAs work) plus each `ca` entry, a
/// PEM bundle or one DER certificate. Like the OS store, a bundle skips certificates that do
/// not parse; an entry fails only when none of its certificates do.
pub fn roots(ca: &[Vec<u8>]) -> Result<Arc<RootCertStore>, HttpAdapterError> {
    let mut roots = RootCertStore::empty();
    roots.add_parsable_certificates(rustls_native_certs::load_native_certs().certs);
    for (index, entry) in ca.iter().enumerate() {
        let fail = |reason: &str| HttpAdapterError::Certificate { index, reason: reason.to_owned() };
        let certs = if entry.windows(PEM_CERTIFICATE.len()).any(|window| window == PEM_CERTIFICATE) {
            let certs = CertificateDer::pem_slice_iter(entry).collect::<Result<Vec<_>, _>>();
            certs.map_err(|_| fail("the PEM bundle is not valid PEM"))?
        } else {
            vec![CertificateDer::from(entry.clone())]
        };
        if roots.add_parsable_certificates(certs).0 == 0 {
            return Err(fail("no certificate in it parses"));
        }
    }
    Ok(Arc::new(roots))
}

/// The client TLS setup: ring as the crypto provider, `roots` with the webpki verifier, ALPN for
/// h2 and HTTP/1.1, and `identity` as the client certificate if given. Verification is always on.
pub fn client_config(
    roots: &Arc<RootCertStore>,
    identity: Option<&ClientIdentity>,
) -> Result<Arc<ClientConfig>, HttpAdapterError> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let builder = ClientConfig::builder_with_provider(provider).with_safe_default_protocol_versions();
    let builder = builder.map_err(|error| HttpAdapterError::Certificate { index: 0, reason: error.to_string() })?;
    let builder = builder.with_root_certificates(Arc::clone(roots));
    let mut config = match identity {
        None => builder.with_no_client_auth(),
        Some(identity) => {
            let (chain, key) = identity.parse()?;
            builder.with_client_auth_cert(chain, key).map_err(|error| identity.error(error.to_string()))?
        }
    };
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
    use super::{ALPN, client_config, roots};
    use crate::HttpAdapterError;

    const EC: &[u8] = include_bytes!("../tests/fixtures/ec.pem");

    #[test]
    fn offers_h2_then_http1_and_loads_the_os_roots() -> Result<(), HttpAdapterError> {
        let native = roots(&[])?;
        assert!(!native.is_empty());
        assert_eq!(client_config(&native, None)?.alpn_protocols, ALPN.map(<[u8]>::to_vec));
        Ok(())
    }

    #[test]
    fn adds_every_certificate_of_a_pem_bundle() -> Result<(), HttpAdapterError> {
        let native = roots(&[])?.len();
        let bundle = [b"\n".as_slice(), EC, include_bytes!("../tests/fixtures/rsa.pem")].concat();
        assert_eq!(roots(&[bundle])?.len(), native + 2);
        Ok(())
    }

    #[test]
    fn reads_bundles_with_comments_and_bag_attributes_and_skips_bad_certificates() -> Result<(), HttpAdapterError> {
        let native = roots(&[])?.len();
        let curl = b"##\n## Bundle of CA Root Certificates\n##\n\nEC client\n==========\n";
        let bag = b"Bag Attributes\n    friendlyName: rsa-client\nsubject=CN=rsa-client\nissuer=CN=rsa-client\n";
        let junk = b"-----BEGIN CERTIFICATE-----\nAAAA\n-----END CERTIFICATE-----\n";
        let bundle = [curl.as_slice(), EC, junk, bag, include_bytes!("../tests/fixtures/rsa.pem")].concat();
        assert_eq!(roots(&[bundle])?.len(), native + 2);
        Ok(())
    }

    #[test]
    fn rejects_a_ca_entry_that_is_not_a_certificate() {
        let cases: [&[u8]; 4] = [
            b"not a certificate",
            b"-----BEGIN PRIVATE KEY-----\n-----END PRIVATE KEY-----\n",
            b"-----BEGIN CERTIFICATE-----\nAAAA\n-----END CERTIFICATE-----\n",
            b"-----BEGIN CERTIFICATE-----\n",
        ];
        let parses = "no certificate in it parses";
        for (case, reason) in cases.into_iter().zip([parses, parses, parses, "the PEM bundle is not valid PEM"]) {
            let error = roots(&[EC.to_vec(), case.to_vec()]).err();
            let Some(HttpAdapterError::Certificate { index: 1, reason: found }) = error else { panic!("{error:?}") };
            assert_eq!(found, reason);
        }
    }
}
