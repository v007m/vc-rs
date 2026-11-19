//! The https mode: a rustls acceptor in front of the same router, with optional client
//! certificate checks (D352).

use std::collections::BTreeMap;
use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::connect_info::Connected;
use axum::serve::{IncomingStream, Listener};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::server::WebPkiClientVerifier;
use rustls::server::danger::ClientCertVerifier;
use rustls::{RootCertStore, ServerConfig};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio_rustls::TlsAcceptor;
use tokio_rustls::server::TlsStream;

use crate::error::MockError;

/// The names `/whoami` gives client certificates, by certificate DER.
pub type Names = Arc<BTreeMap<Vec<u8>, String>>;

/// How long a client has to finish the TLS handshake.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);
/// How long, and how many bytes, a refused connection is read after its alert, so the client
/// gets the alert rather than a reset.
const LINGER: Duration = Duration::from_millis(500);
const LINGER_BYTES: usize = 64 * 1024;
/// The pause after an accept error that is not about one connection (out of file descriptors).
const ACCEPT_BACKOFF: Duration = Duration::from_secs(1);

type Ready = mpsc::Sender<(TlsStream<TcpStream>, SocketAddr)>;

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
    h2: bool,
    tls12: bool,
}

impl MockTls {
    /// Serves `chain` (DER, leaf first) with its PKCS#8 DER `key`, asking for no client certificate.
    #[must_use]
    pub fn new(chain: Vec<Vec<u8>>, key: Vec<u8>) -> Self {
        let chain = chain.into_iter().map(CertificateDer::from).collect();
        Self {
            chain,
            key: PrivatePkcs8KeyDer::from(key),
            clients: None,
            names: BTreeMap::new(),
            h2: false,
            tls12: false,
        }
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

    /// Offers HTTP/2 by ALPN as well as HTTP/1.1.
    #[must_use]
    pub const fn h2(mut self) -> Self {
        self.h2 = true;
        self
    }

    /// Speaks TLS 1.2 only, so a refused client certificate is reported during the handshake
    /// rather than on the client's first read, as under TLS 1.3.
    #[must_use]
    pub const fn tls12(mut self) -> Self {
        self.tls12 = true;
        self
    }

    /// The rustls server setup (ring, HTTP/1.1 and, if asked, HTTP/2 by ALPN, and the client checks asked for) and the
    /// names `/whoami` gives. Consumes the setup, so the key is never copied.
    pub(crate) fn into_server(self) -> Result<(Arc<ServerConfig>, Names), MockError> {
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let builder = ServerConfig::builder_with_provider(provider.clone());
        let versions: &[_] = if self.tls12 { &[&rustls::version::TLS12] } else { rustls::DEFAULT_VERSIONS };
        let builder = builder.with_protocol_versions(versions).map_err(tls_error)?;
        let builder = match self.clients {
            None => builder.with_no_client_auth(),
            Some((auth, roots)) => builder.with_client_cert_verifier(verifier(auth, roots, provider)?),
        };
        let key = PrivateKeyDer::Pkcs8(self.key);
        let mut config = builder.with_single_cert(self.chain, key).map_err(tls_error)?;
        config.alpn_protocols =
            if self.h2 { vec![b"h2".to_vec(), b"http/1.1".to_vec()] } else { vec![b"http/1.1".to_vec()] };
        Ok((Arc::new(config), Arc::new(self.names)))
    }
}

fn verifier(
    auth: ClientAuth,
    roots: Vec<CertificateDer<'static>>,
    provider: Arc<rustls::crypto::CryptoProvider>,
) -> Result<Arc<dyn ClientCertVerifier>, MockError> {
    let mut store = RootCertStore::empty();
    for root in roots {
        store.add(root).map_err(tls_error)?;
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

async fn accept_all(tcp: TcpListener, acceptor: TlsAcceptor, done: Ready) {
    loop {
        let accepted = tokio::select! {
            () = done.closed() => return,
            accepted = tcp.accept() => accepted,
        };
        match accepted {
            Ok(connection) => drop(tokio::spawn(handshake(acceptor.clone(), connection, done.clone()))),
            Err(error) if is_connection_error(&error) => {}
            Err(_) => tokio::time::sleep(ACCEPT_BACKOFF).await,
        }
    }
}

/// Errors about one connection, after which the next accept can go ahead at once.
fn is_connection_error(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::ConnectionRefused | io::ErrorKind::ConnectionAborted | io::ErrorKind::ConnectionReset
    )
}

/// Hands a connection that finished the handshake to the listener. A refused one lingers so its
/// alert arrives; one that takes too long is dropped.
async fn handshake(acceptor: TlsAcceptor, (tcp, remote): (TcpStream, SocketAddr), done: Ready) {
    match tokio::time::timeout(HANDSHAKE_TIMEOUT, acceptor.accept(tcp).into_fallible()).await {
        Ok(Ok(tls)) => drop(done.send((tls, remote)).await),
        Ok(Err((_, tcp))) => linger(tcp).await,
        Err(_) => {}
    }
}

/// Closes the write side after the alert, then reads what the client still sends for a short,
/// bounded while: closing with unread data would send a reset that can overtake the alert.
async fn linger(mut tcp: TcpStream) {
    let _ = tcp.shutdown().await;
    let drain = async {
        let (mut buffer, mut left) = ([0_u8; 4096], LINGER_BYTES);
        while left > 0 {
            match tcp.read(&mut buffer).await {
                Ok(0) | Err(_) => break,
                Ok(read) => left = left.saturating_sub(read),
            }
        }
    };
    let _ = tokio::time::timeout(LINGER, drain).await;
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
