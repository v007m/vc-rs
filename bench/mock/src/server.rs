use std::net::{Ipv4Addr, SocketAddr};

use axum::Router;
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

use crate::error::MockError;
use crate::routes;
use crate::stats::Stats;
use crate::store::Store;
use crate::tls::{MockTls, PeerCert, TlsListener};

/// Entry point: starts a mock server on the current tokio runtime.
#[non_exhaustive]
pub struct MockServer;

impl MockServer {
    /// Starts a server on `127.0.0.1` with a free port, so every test can have its own.
    ///
    /// # Errors
    /// [`MockError::Bind`] when no port can be bound.
    pub async fn start() -> Result<MockHandle, MockError> {
        Self::start_on(SocketAddr::from((Ipv4Addr::LOCALHOST, 0))).await
    }

    /// Starts a server on `addr`.
    ///
    /// # Errors
    /// [`MockError::Bind`] when `addr` cannot be bound.
    pub async fn start_on(addr: SocketAddr) -> Result<MockHandle, MockError> {
        let (listener, addr) = bind(addr).await?;
        let store = Store::spawn();
        let app = routes::router(&store, Router::new());
        let (stop, stopped) = oneshot::channel::<()>();
        let task = tokio::spawn(async move { axum::serve(listener, app).with_graceful_shutdown(until(stopped)).await });
        Ok(MockHandle { addr, scheme: "http", store, stop, task })
    }

    /// Starts an https server on `127.0.0.1` with a free port: the same routes behind a rustls
    /// acceptor set up by `tls`, plus `GET /whoami`.
    ///
    /// # Errors
    /// [`MockError::Bind`] when no port can be bound, [`MockError::Tls`] when rustls refuses `tls`.
    pub async fn start_tls(tls: MockTls) -> Result<MockHandle, MockError> {
        let (config, names) = tls.into_server()?;
        let (listener, addr) = bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0))).await?;
        let store = Store::spawn();
        let app = routes::router(&store, routes::whoami(names));
        let service = app.into_make_service_with_connect_info::<PeerCert>();
        let listener = TlsListener::new(listener, addr, config);
        let (stop, stopped) = oneshot::channel::<()>();
        let task =
            tokio::spawn(async move { axum::serve(listener, service).with_graceful_shutdown(until(stopped)).await });
        Ok(MockHandle { addr, scheme: "https", store, stop, task })
    }
}

async fn bind(addr: SocketAddr) -> Result<(TcpListener, SocketAddr), MockError> {
    let bind = |source| MockError::Bind { addr, source };
    let listener = TcpListener::bind(addr).await.map_err(bind)?;
    let addr = listener.local_addr().map_err(bind)?;
    Ok((listener, addr))
}

async fn until(stopped: oneshot::Receiver<()>) {
    let _ = stopped.await;
}

/// A running mock server. Dropping it starts a graceful shutdown; [`MockHandle::shutdown`]
/// also waits for it to finish.
pub struct MockHandle {
    addr: SocketAddr,
    scheme: &'static str,
    store: Store,
    stop: oneshot::Sender<()>,
    task: JoinHandle<std::io::Result<()>>,
}

impl MockHandle {
    /// The bound address.
    #[must_use]
    pub const fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// `http://<addr>`, or `https://<addr>` for [`MockServer::start_tls`], without a trailing slash.
    #[must_use]
    pub fn base_url(&self) -> String {
        format!("{}://{}", self.scheme, self.addr)
    }

    /// The current counters, as served by `GET /__stats`.
    ///
    /// # Errors
    /// [`MockError::Stopped`] when the server has stopped.
    pub async fn stats(&self) -> Result<Stats, MockError> {
        self.store.call(|state| state.stats()).await
    }

    /// Issues a refresh token that `POST /oauth/token` takes once, as a long-lived token kept
    /// for CI would be.
    ///
    /// # Errors
    /// [`MockError::Stopped`] when the server has stopped.
    pub async fn refresh_token(&self) -> Result<String, MockError> {
        self.store.call(|state| state.sessions.refresh_token()).await
    }

    /// Clears all state and counters, like `POST /__reset`.
    ///
    /// # Errors
    /// [`MockError::Stopped`] when the server has stopped.
    pub async fn reset(&self) -> Result<(), MockError> {
        self.store.call(crate::state::State::reset).await
    }

    /// Stops accepting connections and waits for open ones to finish.
    ///
    /// # Errors
    /// [`MockError::Io`] when serving failed, [`MockError::Stopped`] when the server task died.
    pub async fn shutdown(self) -> Result<(), MockError> {
        let _ = self.stop.send(());
        self.task.await.map_err(|_| MockError::Stopped)?.map_err(MockError::Io)
    }
}
