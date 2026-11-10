//! Puts the server together: the Connect services, the mounts (MCP over HTTP), the web UI's
//! files and the guard in front of them all, listening on loopback only.

use std::convert::Infallible;
use std::fmt;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;

use axum::Router;
use axum::extract::Request;
use axum::middleware::from_fn_with_state;
use axum::response::IntoResponse;
use tokio::net::TcpListener;
use tower::Service;

use crate::guard::{self, Guard};
use crate::site::{self, Files, INDEX};
use crate::token::Token;
use crate::{ConnectService, ServerError, csp};

/// The parts of a server, before it listens.
pub struct ServerBuilder {
    port: u16,
    routes: Router,
    files: Option<Arc<dyn Files>>,
}

impl ServerBuilder {
    /// A server that will listen on `127.0.0.1:port`; port 0 picks a free one.
    #[must_use]
    pub fn new(port: u16) -> Self {
        Self { port, routes: Router::new(), files: None }
    }

    /// Serves a Connect service's methods.
    #[must_use]
    pub fn service(mut self, service: ConnectService) -> Self {
        self.routes = self.routes.merge(service.into_router());
        self
    }

    /// Serves everything under `path` (such as `/mcp`) with `service`, behind the same guard.
    /// `path` starts with `/` and is not `/` itself.
    #[must_use]
    pub fn mount<S>(mut self, path: &'static str, service: S) -> Self
    where
        S: Service<Request, Error = Infallible> + Clone + Send + Sync + 'static,
        S::Response: IntoResponse,
        S::Future: Send + 'static,
    {
        self.routes = self.routes.nest_service(path, service);
        self
    }

    /// Serves the web UI from `files`; its `index.html` gives the CSP its script hashes.
    #[must_use]
    pub fn files(mut self, files: Arc<dyn Files>) -> Self {
        self.files = Some(files);
        self
    }

    /// Listens on loopback with a new token.
    ///
    /// # Errors
    ///
    /// [`ServerError::Random`] when no token can be made, [`ServerError::Bind`] when the port
    /// cannot be listened on.
    pub async fn bind(self) -> Result<Server, ServerError> {
        let token = Arc::new(Token::generate()?);
        let address = SocketAddr::from((Ipv4Addr::LOCALHOST, self.port));
        let listener = TcpListener::bind(address).await.map_err(|source| ServerError::Bind { address, source })?;
        let address = listener.local_addr().map_err(|source| ServerError::Bind { address, source })?;
        let launch = LaunchUrl(format!("http://{address}/?token={}", token.expose()));
        Ok(Server { listener, app: self.app(token, address.port()), launch })
    }

    /// The routes behind the guard, for a server on `port` with `token`.
    pub(crate) fn app(self, token: Arc<Token>, port: u16) -> Router {
        let index = self.files.as_ref().and_then(|files| files.get(INDEX));
        let policy = csp::policy(index.as_deref().and_then(|bytes| std::str::from_utf8(bytes).ok()));
        let guard = Arc::new(Guard::new(token, port, &policy));
        let site = Router::new().fallback(site::serve).with_state(self.files);
        self.routes.fallback_service(site).layer(from_fn_with_state(guard, guard::check))
    }
}

/// A server listening on loopback, ready to serve.
pub struct Server {
    listener: TcpListener,
    app: Router,
    launch: LaunchUrl,
}

impl Server {
    /// The URL that signs a browser in: the only place the token is shown.
    #[must_use]
    pub const fn launch_url(&self) -> &LaunchUrl {
        &self.launch
    }

    /// Serves until `shutdown` resolves, then lets the requests in flight finish.
    ///
    /// # Errors
    ///
    /// [`ServerError::Serve`] when the listener fails.
    pub async fn serve(self, shutdown: impl Future<Output = ()> + Send + 'static) -> Result<(), ServerError> {
        axum::serve(self.listener, self.app).with_graceful_shutdown(shutdown).await.map_err(ServerError::Serve)
    }
}

/// `http://127.0.0.1:<port>/?token=<token>`. It holds the token, so `Debug` hides it and the text
/// is read only through [`LaunchUrl::expose`], to print it once for the user.
pub struct LaunchUrl(String);

impl LaunchUrl {
    /// The URL with its token.
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for LaunchUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("LaunchUrl(<redacted>)")
    }
}

#[cfg(test)]
#[path = "builder_tests.rs"]
mod tests;
