use std::net::{Ipv4Addr, SocketAddr};

use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

use crate::error::MockError;
use crate::routes::router;
use crate::stats::Stats;
use crate::store::Store;

/// Entry point: starts a mock server on the current tokio runtime.
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
        let bind = |source| MockError::Bind { addr, source };
        let listener = TcpListener::bind(addr).await.map_err(bind)?;
        let addr = listener.local_addr().map_err(bind)?;
        let store = Store::spawn();
        let app = router(&store);
        let (stop, stopped) = oneshot::channel::<()>();
        let task = tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(async move {
                    let _ = stopped.await;
                })
                .await
        });
        Ok(MockHandle { addr, store, stop, task })
    }
}

/// A running mock server. Dropping it starts a graceful shutdown; [`MockHandle::shutdown`]
/// also waits for it to finish.
pub struct MockHandle {
    addr: SocketAddr,
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

    /// `http://<addr>`, without a trailing slash.
    #[must_use]
    pub fn base_url(&self) -> String {
        format!("http://{}", self.addr)
    }

    /// The current counters, as served by `GET /__stats`.
    ///
    /// # Errors
    /// [`MockError::Stopped`] when the server has stopped.
    pub async fn stats(&self) -> Result<Stats, MockError> {
        self.store.call(|state| state.stats()).await
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
