//! The check in front of every route. A request must name this server in `Host` (no DNS
//! rebinding) and must not come from another site's page (`Origin`). Then it needs one secret,
//! and which one depends on where it goes (D239, D240):
//! - `/mcp` takes only the agent token, as a bearer token;
//! - everything else (`/api` and the pages) takes only the session secret, in the
//!   `vc_token_<port>` cookie. A bearer token there is refused.
//!
//! The printed URL's `GET /?token=…` trades the launch token for the cookie, once, and redirects
//! to a URL without it. A write the cookie signs in must also say it comes from this server's page
//! (`Origin`). There is no CORS, and every response carries the security headers.

use std::sync::Arc;

use axum::extract::{Request, State};
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode, Uri, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use crate::token::{Exchange, Token};

/// Where MCP clients connect, with the agent token.
pub const MCP: &str = "/mcp";

/// The query key of the launch URL.
const QUERY: &str = "token";

/// What the check needs: the token, the names this server answers to, the cookie's name and
/// the CSP header.
pub struct Guard {
    token: Arc<Token>,
    hosts: [String; 2],
    origins: [String; 2],
    cookie: String,
    csp: HeaderValue,
}

/// What becomes of one request.
enum Verdict {
    Pass,
    SignIn,
    Refuse(StatusCode, &'static str),
}

impl Guard {
    /// The check for a server on loopback `port`, answering with the `csp` header, which the
    /// builder made once ([`crate::csp::header`]).
    pub fn new(token: Arc<Token>, port: u16, csp: HeaderValue) -> Self {
        let hosts = [format!("127.0.0.1:{port}"), format!("localhost:{port}")];
        let origins = hosts.clone().map(|host| format!("http://{host}"));
        Self { token, hosts, origins, cookie: format!("vc_token_{port}"), csp }
    }

    fn verdict(&self, method: &Method, uri: &Uri, headers: &HeaderMap) -> Verdict {
        if !self.named(headers) {
            return Verdict::Refuse(StatusCode::FORBIDDEN, "this server answers only to 127.0.0.1 and localhost");
        }
        if !method.is_safe() && !self.same_origin(headers) {
            return Verdict::Refuse(StatusCode::FORBIDDEN, "requests from other sites are refused");
        }
        match query_token(uri) {
            Some(given) if method == Method::GET && uri.path() == "/" => self.sign_in(given),
            Some(_) => Verdict::Refuse(StatusCode::BAD_REQUEST, "the token goes only in the launch URL"),
            None if is_mcp(uri.path()) => self.agent(headers),
            None => self.person(method, headers),
        }
    }

    /// Passes the agent token as a bearer token, and nothing else: not the cookie.
    fn agent(&self, headers: &HeaderMap) -> Verdict {
        if bearer_token(headers).is_some_and(|given| self.token.agent_matches(given)) {
            Verdict::Pass
        } else {
            Verdict::Refuse(StatusCode::UNAUTHORIZED, "send the agent token `vc ui` printed as `Authorization: Bearer`")
        }
    }

    /// Passes the session secret in the cookie, and nothing else: a request with a bearer token
    /// is refused. A write must carry `Origin`, which [`Self::verdict`] has checked is ours.
    fn person(&self, method: &Method, headers: &HeaderMap) -> Verdict {
        if headers.contains_key(header::AUTHORIZATION) {
            return Verdict::Refuse(StatusCode::UNAUTHORIZED, "a bearer token works only on /mcp");
        }
        if !cookie_values(headers, &self.cookie).any(|given| self.token.session_matches(given)) {
            return Verdict::Refuse(StatusCode::UNAUTHORIZED, "open the URL `vc ui` printed to sign in");
        }
        if method.is_safe() || headers.contains_key(header::ORIGIN) {
            Verdict::Pass
        } else {
            Verdict::Refuse(StatusCode::FORBIDDEN, "a write from the browser must say where it comes from (Origin)")
        }
    }

    fn sign_in(&self, given: &str) -> Verdict {
        match self.token.exchange(given) {
            Exchange::SignedIn => Verdict::SignIn,
            Exchange::Used => Verdict::Refuse(
                StatusCode::UNAUTHORIZED,
                "this sign-in link was used already; use the open tab or restart vc ui",
            ),
            Exchange::Wrong => Verdict::Refuse(
                StatusCode::UNAUTHORIZED,
                "the token is not this server's; open the URL `vc ui` printed",
            ),
        }
    }

    fn named(&self, headers: &HeaderMap) -> bool {
        let host = headers.get(header::HOST).and_then(|value| value.to_str().ok());
        host.is_some_and(|host| self.hosts.iter().any(|name| name.eq_ignore_ascii_case(host)))
    }

    /// A request without `Origin` comes from a client that is not a browser page, such as an MCP
    /// client, and still needs the token.
    fn same_origin(&self, headers: &HeaderMap) -> bool {
        headers.get_all(header::ORIGIN).iter().all(|value| {
            value.to_str().is_ok_and(|origin| self.origins.iter().any(|ours| ours.eq_ignore_ascii_case(origin)))
        })
    }

    /// The `Set-Cookie` value that signs the browser in. `Secure` is left out on purpose: the
    /// server speaks plain HTTP on loopback, where a `Secure` cookie would never be sent back.
    fn cookie(&self) -> Option<HeaderValue> {
        let text = format!("{}={}; Path=/; HttpOnly; SameSite=Strict", self.cookie, self.token.session());
        let mut value = HeaderValue::from_str(&text).ok()?;
        value.set_sensitive(true);
        Some(value)
    }
}

/// The middleware: answers or passes the request on, then adds the security headers
/// (`no-store`, `nosniff`, `no-referrer` and the CSP) to whatever goes back.
pub async fn check(State(guard): State<Arc<Guard>>, request: Request, next: Next) -> Response {
    let verdict = guard.verdict(request.method(), request.uri(), request.headers());
    let mut response = match verdict {
        Verdict::Pass => next.run(request).await,
        Verdict::SignIn => signed_in(&guard),
        Verdict::Refuse(status, why) => (status, why).into_response(),
    };
    let headers = response.headers_mut();
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    headers.insert(header::REFERRER_POLICY, HeaderValue::from_static("no-referrer"));
    headers.insert(header::CONTENT_SECURITY_POLICY, guard.csp.clone());
    response
}

/// 303 to `/`, so the token leaves the address bar and the history, with the cookie set.
fn signed_in(guard: &Guard) -> Response {
    let Some(cookie) = guard.cookie() else { return StatusCode::INTERNAL_SERVER_ERROR.into_response() };
    let headers = [(header::LOCATION, HeaderValue::from_static("/")), (header::SET_COOKIE, cookie)];
    (StatusCode::SEE_OTHER, headers).into_response()
}

fn query_token(uri: &Uri) -> Option<&str> {
    let query = uri.query()?;
    query.split('&').find_map(|pair| pair.split_once('=').filter(|(key, _)| *key == QUERY).map(|(_, value)| value))
}

/// Whether `path` is under [`MCP`].
fn is_mcp(path: &str) -> bool {
    path.strip_prefix(MCP).is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
}

/// The values of every cookie called `cookie`.
fn cookie_values<'h>(headers: &'h HeaderMap, cookie: &'h str) -> impl Iterator<Item = &'h str> {
    let lines = headers.get_all(header::COOKIE).into_iter().filter_map(|value| value.to_str().ok());
    lines
        .flat_map(|line| line.split(';'))
        .filter_map(move |pair| pair.trim().split_once('=').filter(|(name, _)| *name == cookie).map(|(_, value)| value))
}

fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    let value = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let (scheme, token) = value.split_once(' ')?;
    scheme.eq_ignore_ascii_case("bearer").then(|| token.trim())
}

#[cfg(test)]
#[path = "guard_tests.rs"]
mod tests;
