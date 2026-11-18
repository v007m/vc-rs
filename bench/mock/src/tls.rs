//! The https mode: a rustls acceptor in front of the same router, with optional client
//! certificate checks (D352).

use std::collections::BTreeMap;
use std::io;
use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::connect_info::Connected;
use axum::serve::{IncomingStream, Listener};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::server::WebPkiClientVerifier;
use rustls::server::danger::ClientCertVerifier;
use rustls::{RootCertStore, ServerConfig};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio_rustls::TlsAcceptor;
use tokio_rustls::server::TlsStream;

use crate::error::MockError;

/// Whether the https mock asks clients for a certificate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ClientAuth {
    /// The handshake fails without a certificate the roots trust.
    Required,
    /// A certificate is checked when sent; without one, `/whoami` answers 403.
    Optional,
}

/// The https mode's setup: the server's certificate chain and key, which roots client
/// certificates must chain to, and the names `/whoami` gives known client certificates.
pub struct MockTls {
    chain: Vec<CertificateDer<'static>>,
    key: PrivatePkcs8KeyDer<'static>,
    clients: Option<(ClientAuth, Vec<CertificateDer<'static>>)>,
    names: BTreeMap<Vec<u8>, String>,
}

impl MockTls {
    /// Serves `chain` (DER, leaf first) with its PKCS#8 DER `key`, asking for no client certificate.
    #[must_use]
    pub fn new(chain: Vec<Vec<u8>>, key: Vec<u8>) -> Self {
        let chain = chain.into_iter().map(CertificateDer::from).collect();
        Self { chain, key: PrivatePkcs8KeyDer::from(key), clients: None, names: BTreeMap::new() }
    }

    /// Checks client certificates against `roots` (DER).
    #[must_use]
    pub fn client_auth(mut self, auth: ClientAuth, roots: Vec<Vec<u8>>) -> Self {
        self.clients = Some((auth, roots.into_iter().map(CertificateDer::from).collect()));
        self
    }

    /// `/whoami` answers `name` for the client certificate `cert` (DER).
    #[must_use]
    pub fn name_client(mut self, name: &str, cert: Vec<u8>) -> Self {
        self.names.insert(cert, name.to_owned());
        self
    }

    /// The names `/whoami` gives, by certificate DER.
    pub(crate) fn names(&self) -> Arc<BTreeMap<Vec<u8>, String>> {
        Arc::new(self.names.clone())
    }

    /// The rustls server setup: ring, HTTP/1.1 by ALPN, and the client checks asked for.
    pub(crate) fn server_config(&self) -> Result<Arc<ServerConfig>, MockError> {
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let builder = ServerConfig::builder_with_provider(provider.clone());
        let builder = builder.with_safe_default_protocol_versions().map_err(tls_error)?;
        let builder = match &self.clients {
            None => builder.with_no_client_auth(),
            Some((auth, roots)) => builder.with_client_cert_verifier(verifier(*auth, roots, provider)?),
        };
        let key = PrivateKeyDer::Pkcs8(self.key.clone_key());
        let mut config = builder.with_single_cert(self.chain.clone(), key).map_err(tls_error)?;
        config.alpn_protocols = vec![b"http/1.1".to_vec()];
        Ok(Arc::new(config))
    }
}

fn verifier(
    auth: ClientAuth,
    roots: &[CertificateDer<'static>],
    provider: Arc<rustls::crypto::CryptoProvider>,
) -> Result<Arc<dyn ClientCertVerifier>, MockError> {
    let mut store = RootCertStore::empty();
    for root in roots {
        store.add(root.clone()).map_err(tls_error)?;
    }
    let builder = WebPkiClientVerifier::builder_with_provider(Arc::new(store), provider);
    let builder = if auth == ClientAuth::Optional { builder.allow_unauthenticated() } else { builder };
    builder.build().map_err(tls_error)
}

fn tls_error(error: impl std::fmt::Display) -> MockError {
    MockError::Tls(error.to_string())
}

/// Connections that finished the TLS handshake. Handshakes run on tasks of their own, so a slow
/// or failing client never holds up the others; a failed one is dropped after its alert is sent.
pub struct TlsListener {
    ready: mpsc::Receiver<(TlsStream<TcpStream>, SocketAddr)>,
    addr: SocketAddr,
}

impl TlsListener {
    /// Accepts on `tcp` with `config` until the listener is dropped.
    pub fn new(tcp: TcpListener, addr: SocketAddr, config: Arc<ServerConfig>) -> Self {
        let (done, ready) = mpsc::channel(64);
        tokio::spawn(accept_all(tcp, TlsAcceptor::from(config), done));
        Self { ready, addr }
    }
}

async fn accept_all(tcp: TcpListener, acceptor: TlsAcceptor, done: mpsc::Sender<(TlsStream<TcpStream>, SocketAddr)>) {
    loop {
        let accepted = tokio::select! {
            () = done.closed() => return,
            accepted = tcp.accept() => accepted,
        };
        let Ok((stream, remote)) = accepted else { continue };
        let (acceptor, done) = (acceptor.clone(), done.clone());
        tokio::spawn(async move {
            if let Ok(tls) = acceptor.accept(stream).await {
                let _ = done.send((tls, remote)).await;
            }
        });
    }
}

impl Listener for TlsListener {
    type Io = TlsStream<TcpStream>;
    type Addr = SocketAddr;

    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        match self.ready.recv().await {
            Some(connection) => connection,
            None => std::future::pending().await,
        }
    }

    fn local_addr(&self) -> io::Result<Self::Addr> {
        Ok(self.addr)
    }
}

/// The leaf certificate the client sent (DER), if any.
#[derive(Clone, Debug)]
pub struct PeerCert(pub Option<Vec<u8>>);

impl Connected<IncomingStream<'_, TlsListener>> for PeerCert {
    fn connect_info(stream: IncomingStream<'_, TlsListener>) -> Self {
        let certs = stream.io().get_ref().1.peer_certificates();
        Self(certs.and_then(<[CertificateDer<'_>]>::first).map(|cert| cert.to_vec()))
    }
}
