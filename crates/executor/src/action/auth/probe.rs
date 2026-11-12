//! The auth test of the web UI's settings: the credentials an auth gives, obtained through the
//! run's token cache like a call's, reported by name and lifetime only, never by value.

use std::sync::Arc;
use std::time::Duration;

use http::Uri;
use vc_interfaces::{Credentials, Secret, TransportError};
use vc_model::{Action, Api, Failure};

use super::stage::AuthStage;
use super::{Lease, Prepared, prepare};
use crate::action::{called, request};
use crate::{ActionRequest, Render};

/// What an auth puts on a request, by name, and how long it holds; the values stay hidden.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct ProbedAuth {
    /// The header names.
    pub headers: Vec<String>,
    /// The query parameter names.
    pub query: Vec<String>,
    /// The cookie names.
    pub cookies: Vec<String>,
    /// How long the credentials hold from now; `None` when they never expire.
    pub expires_in: Option<Duration>,
}

/// The credentials the auth of `request`'s call gives, or `None` when the call has no auth.
/// Whatever is fetched is masked by the run's redactor from then on, as a call's would be.
///
/// # Errors
///
/// A failure when the call's auth cannot render, or a V5114 failure, masked by the run's
/// redactor, when its credentials cannot be obtained.
pub async fn probe_auth(request: &ActionRequest) -> Result<Option<ProbedAuth>, Failure> {
    let Some(prepared) = prepared(request)? else { return Ok(None) };
    let cx = &request.cx;
    let lease = Arc::new(Lease::new(prepared.key));
    let stage =
        AuthStage::new(prepared, (Arc::clone(&cx.tokens), lease), (Arc::clone(&cx.auth), Arc::clone(&cx.redactor)));
    match stage.credentials().await {
        Ok(credentials) => Ok(Some(named(&credentials))),
        Err(error) => {
            let error = TransportError::Credentials(error);
            let message = cx.redactor.masked_text(&error.to_string()).await;
            Err(Failure::new(message).with_code(error.code().to_string()))
        }
    }
}

/// The origin `request`'s call sends to, `scheme://host:port` in lower case with the scheme's
/// port when the URL leaves it out; nothing is sent and no auth is applied.
///
/// # Errors
///
/// A failure when the call's URL cannot render or names no host.
pub fn probe_origin(request: &ActionRequest) -> Result<String, Failure> {
    let (action, api) = call_of(request)?;
    let render = Render::new(request).map_err(request::failed)?;
    origin(&request::uri(&render, api, action)?).ok_or_else(|| Failure::new("the call's URL names no host"))
}

/// The auth of `request`'s call, rendered.
fn prepared(request: &ActionRequest) -> Result<Option<Prepared>, Failure> {
    let (action, api) = call_of(request)?;
    let mut render = Render::new(request).map_err(request::failed)?;
    prepare(request, &mut render, (api, action))
}

/// The action of `request` and the api it calls.
fn call_of(request: &ActionRequest) -> Result<(&Action, &Api), Failure> {
    let cx = &request.cx;
    let action = cx.action(request.action).ok_or_else(|| Failure::new("the action is not in the program"))?;
    let api = called(&cx.program, action).ok_or_else(|| Failure::new("the action calls no api in the program"))?;
    Ok((action, api))
}

/// `uri`'s scheme, host and port, the port given or the scheme's own.
fn origin(uri: &Uri) -> Option<String> {
    let scheme = uri.scheme_str()?.to_ascii_lowercase();
    let host = uri.host()?.to_ascii_lowercase();
    let default = match scheme.as_str() {
        "https" => Some(443),
        "http" => Some(80),
        _ => None,
    };
    let port = uri.port_u16().or(default);
    Some(port.map_or_else(|| format!("{scheme}://{host}"), |port| format!("{scheme}://{host}:{port}")))
}

fn named(credentials: &Credentials) -> ProbedAuth {
    let names = |pairs: &[(String, Secret)]| pairs.iter().map(|(name, _)| name.clone()).collect();
    ProbedAuth {
        headers: names(&credentials.headers),
        query: names(&credentials.query),
        cookies: names(&credentials.cookies),
        expires_in: credentials.expires_in,
    }
}
