//! Puts the server together: the Connect services, MCP over HTTP at `/mcp`, the web UI's files
//! and the guard in front of them all, listening on loopback only.

use std::convert::Infallible;
use std::fmt;
use std::future::IntoFuture as _;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::extract::Request;
use axum::http::HeaderValue;
use axum::middleware::from_fn_with_state;
use axum::response::IntoResponse;
use tokio::net::TcpListener;
use tokio::sync::watch;
use tower::Service;

use crate::guard::{self, Guard, MCP};
use crate::site::{self, Files, INDEX};
use crate::token::Token;
use crate::{ConnectService, ServerError, csp, engine_api};

/// How long the requests in flight at shutdown get to finish, once their streams were told to
/// stop, before the server returns without them.
const GRACE: Duration = Duration::from_secs(5);

/// The parts of a server, before it listens.
pub struct ServerBuilder {
    port: u16,
    services: Vec<ConnectService>,
    /// What answers at [`MCP`], if anything does.
    mcp: Option<Router>,
    files: Option<Arc<dyn Files>>,
    /// Turns true at shutdown, which ends every open stream.
    stop: watch::Sender<bool>,
    grace: Duration,
}

impl ServerBuilder {
    /// A server that will listen on `127.0.0.1:port`; port 0 picks a free one.
    #[must_use]
    pub fn new(port: u16) -> Self {
        Self { port, services: Vec::new(), mcp: None, files: None, stop: watch::channel(false).0, grace: GRACE }
    }

    /// Gives the requests in flight at shutdown `grace` instead of [`GRACE`].
    #[cfg(test)]
    #[must_use]
    pub(crate) const fn grace(mut self, grace: Duration) -> Self {
        self.grace = grace;
        self
    }

    /// Serves a Connect service's methods.
    #[must_use]
    pub fn service(mut self, service: ConnectService) -> Self {
        self.services.push(service);
        self
    }

    /// Serves everything under `/mcp` with `service`, for MCP clients that send the agent token.
    /// The engine service then also answers `Connection`, with where they connect and that token.
    #[must_use]
    pub fn mcp<S>(mut self, service: S) -> Self
    where
        S: Service<Request, Error = Infallible> + Clone + Send + Sync + 'static,
        S::Response: IntoResponse,
        S::Future: Send + 'static,
    {
        self.mcp = Some(Router::new().nest_service(MCP, service));
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
        let (stop, grace) = (self.stop.clone(), self.grace);
        let Assembled { app, mcp, csp_problem } = self.assemble(token, address.port());
        Ok(Server { listener, app, launch, mcp, stop, grace, csp_problem })
    }

    /// The routes behind a guard for `port` with `token`, and the CSP's problem, if any.
    fn assemble(self, token: Arc<Token>, port: u16) -> Assembled {
        let mcp = self.mcp.is_some().then(|| McpAccess::new(port, token.agent()));
        let (csp, csp_problem) = self.csp();
        let guard = Arc::new(Guard::new(token, port, csp));
        let Self { services, mcp: mounted, files, stop, .. } = self;
        let services = services.into_iter().map(|service| engine_api::connected(service, mcp.as_ref()));
        let routes = services.fold(mounted.unwrap_or_default(), |routes, service| {
            routes.merge(service.stopped_by(stop.subscribe()).into_router())
        });
        let site = Router::new().fallback(site::serve).with_state(files);
        let app = routes.fallback_service(site).layer(from_fn_with_state(guard, guard::check));
        Assembled { app, mcp, csp_problem }
    }

    /// The CSP header for the web UI's `index.html`, and why it is [`csp::FALLBACK`] when its
    /// script hashes cannot be read ([`csp::header`]).
    fn csp(&self) -> (HeaderValue, Option<&'static str>) {
        let index = self.files.as_ref().and_then(|files| files.get(INDEX));
        csp::header(index.as_deref())
    }

    /// The routes behind a guard for `port` with `token`, as [`ServerBuilder::bind`] puts them.
    #[cfg(test)]
    pub(crate) fn guarded(self, token: Arc<Token>, port: u16) -> Router {
        self.assemble(token, port).app
    }
}

/// What [`ServerBuilder::assemble`] puts together.
struct Assembled {
    app: Router,
    mcp: Option<McpAccess>,
    csp_problem: Option<&'static str>,
}

/// A server listening on loopback, ready to serve.
pub struct Server {
    listener: TcpListener,
    app: Router,
    launch: LaunchUrl,
    mcp: Option<McpAccess>,
    stop: watch::Sender<bool>,
    grace: Duration,
    csp_problem: Option<&'static str>,
}

impl Server {
    /// Why the web UI's page gave no CSP script hashes, if it did not. The header then lets no
    /// inline script run, so the page will not load; the caller reports this as an error.
    #[must_use]
    pub const fn csp_problem(&self) -> Option<&'static str> {
        self.csp_problem
    }

    /// The URL that signs a browser in: the only place the token is shown.
    #[must_use]
    pub const fn launch_url(&self) -> &LaunchUrl {
        &self.launch
    }

    /// Where MCP clients connect and the agent token they send, when the server serves `/mcp`.
    #[must_use]
    pub const fn mcp(&self) -> Option<&McpAccess> {
        self.mcp.as_ref()
    }

    /// Serves until `shutdown` resolves. Then every open stream is told to stop (a run is
    /// interrupted, so its cleanups run) and the requests in flight get a few seconds to finish
    /// before the server returns without them.
    ///
    /// # Errors
    ///
    /// [`ServerError::Serve`] when the listener fails.
    pub async fn serve(self, shutdown: impl Future<Output = ()> + Send + 'static) -> Result<(), ServerError> {
        let Self { listener, app, stop, grace, .. } = self;
        let mut stopped = stop.subscribe();
        let signal = async move {
            shutdown.await;
            stop.send_replace(true);
        };
        let serving = axum::serve(listener, app).with_graceful_shutdown(signal).into_future();
        let expired = async move {
            if stopped.wait_for(|stop| *stop).await.is_ok() {
                tokio::time::sleep(grace).await;
            } else {
                std::future::pending::<()>().await;
            }
        };
        tokio::select! {
            served = serving => served.map_err(ServerError::Serve),
            () = expired => Ok(()),
        }
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

/// `http://127.0.0.1:<port>/mcp` and the agent token, the only secret `/mcp` takes (D239).
///
/// `Debug` shows the URL and hides the token, which is read only through [`McpAccess::token`],
/// to print it once for the user and to show it in the Connect panel.
#[derive(Clone)]
pub struct McpAccess {
    url: String,
    token: String,
}

impl McpAccess {
    fn new(port: u16, token: &str) -> Self {
        Self { url: format!("http://{}:{port}{MCP}", Ipv4Addr::LOCALHOST), token: token.to_owned() }
    }

    /// The URL MCP clients connect to.
    #[must_use]
    pub fn url(&self) -> &str {
        &self.url
    }

    /// The agent token, for `Authorization: Bearer`.
    #[must_use]
    pub fn token(&self) -> &str {
        &self.token
    }
}

impl fmt::Debug for McpAccess {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("McpAccess").field("url", &self.url).finish_non_exhaustive()
    }
}

#[cfg(test)]
#[path = "builder_tests.rs"]
mod tests;
