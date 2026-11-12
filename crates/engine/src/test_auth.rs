//! The web UI's auth test (LLD `TestAuth`): obtains the credentials of one auth as a call of an
//! api would, through the run's token cache, without sending the call. Only what the credentials
//! are called and how long they hold leave; a value never does, and a failure is masked.

use std::sync::Arc;
use std::time::Duration;

use vc_executor::{ActionRequest, Frame, chosen_auth, probe_auth};
use vc_model::{Action, ActionId, Api, AuthId, AuthRef, OwnerKey, Program, VarDefs};

use crate::call::{called, named_api, pushed};
use crate::explain::did_you_mean;
use crate::{CallRequest, Engine, EngineError};

/// What the auth test asks for.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "built by the apps with `..Default::default()`; every user is in this repo (D160)"
)]
pub struct TestAuthRequest {
    /// The auth's ID, `svc`.
    pub auth: String,
    /// The api to obtain them for, `users#me`; by default the first api, by ID, that uses it.
    pub api: Option<String>,
    /// `--profile`, in order; later ones win. Each may be a comma list.
    pub profiles: Vec<String>,
    /// `--set key=value`, in order; the top config layer (D129).
    pub sets: Vec<String>,
}

/// What [`Engine::test_auth`] obtained: names and a lifetime, never a value.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct AuthTested {
    /// The api the credentials were obtained for.
    pub api: String,
    /// The header names the auth sets.
    pub headers: Vec<String>,
    /// The query parameter names it adds.
    pub query: Vec<String>,
    /// The cookie names it sends.
    pub cookies: Vec<String>,
    /// How long the credentials hold; `None` when they never expire.
    pub expires_in: Option<Duration>,
    /// Why they could not be obtained (V5114), masked; then there are no names.
    pub error: Option<String>,
    /// Notices from the compile.
    pub notices: Vec<String>,
}

impl Engine {
    /// The credentials `request.auth` gives under `request`'s profiles and `--set`s, by name.
    ///
    /// # Errors
    ///
    /// [`EngineError::Selection`] for an unknown auth or api, or an auth no api uses when no api
    /// is given; [`EngineError::Invalid`] and [`EngineError::Setting`] as for `vc call`, and the
    /// error of a secret that does not resolve. Credentials that cannot be obtained are no
    /// error: they are [`AuthTested::error`].
    pub async fn test_auth(&self, request: &TestAuthRequest) -> Result<AuthTested, EngineError> {
        let layers =
            CallRequest { profiles: request.profiles.clone(), sets: request.sets.clone(), ..CallRequest::default() };
        let mut prepared = self.adhoc(&layers)?;
        let (api, action) = probe_call(&mut prepared.program, request)?;
        let secrets = self.secrets(&prepared.program.config.secrets, &prepared.environment).await?;
        let cx = self.adhoc_cx(&prepared, (&[action], 0, secrets)).await?;
        let probed = probe_auth(&ActionRequest { action, owner: OwnerKey::Run, cx, frame: Frame::default() }).await;
        let tested = AuthTested { api, notices: std::mem::take(&mut prepared.notices), ..AuthTested::default() };
        Ok(match probed {
            Ok(Some(probed)) => AuthTested {
                headers: probed.headers,
                query: probed.query,
                cookies: probed.cookies,
                expires_in: probed.expires_in,
                ..tested
            },
            Ok(None) => AuthTested { error: Some(format!("`{}` gives no credentials", request.auth)), ..tested },
            Err(failure) => AuthTested { error: Some(failure.message), ..tested },
        })
    }
}

/// Adds the action that obtains the auth's credentials for the api, and returns the api's ID
/// with it.
fn probe_call(program: &mut Arc<Program>, request: &TestAuthRequest) -> Result<(String, ActionId), EngineError> {
    let auth = named_auth(program, &request.auth)?;
    let (api, action) = match &request.api {
        Some(id) => (id.clone(), using(program, named_api(program, id)?, auth)),
        None => first_user(program, auth).ok_or_else(|| EngineError::Selection {
            reason: format!("no api uses the auth `{}`", request.auth),
            help: Some("name the api to obtain its credentials for".to_owned()),
        })?,
    };
    Ok((api, pushed(program, action)?))
}

/// The auth whose ID is `name`.
fn named_auth(program: &Program, name: &str) -> Result<AuthId, EngineError> {
    let found = program.auth.values().find(|def| program.str(def.id.sym()) == Some(name));
    found.map(|def| def.id).ok_or_else(|| {
        let names = program.auth.values().filter_map(|def| program.str(def.id.sym()));
        EngineError::Selection {
            reason: format!("no auth `{name}`"),
            help: did_you_mean(&vc_planner::closest(name, names)),
        }
    })
}

/// A call of `api` that uses `auth`: through the api's own use of it, vars included, or else
/// with no vars.
fn using(program: &Program, api: &Api, auth: AuthId) -> Action {
    let mut action = called(api, true);
    if chosen_auth(program, (api, &action)) != Some(auth) {
        action.auth = Some(AuthRef::Use { id: auth, vars: VarDefs::default() });
    }
    action
}

/// The first api, by ID, that uses `auth`, with its call.
fn first_user(program: &Program, auth: AuthId) -> Option<(String, Action)> {
    let users = program.apis.values().filter_map(|api| {
        let action = called(api, true);
        let id = program.str(api.id.sym())?;
        (chosen_auth(program, (api, &action)) == Some(auth)).then(|| (id.to_owned(), action))
    });
    users.min_by(|(one, _), (other, _)| one.cmp(other))
}

#[cfg(test)]
#[path = "test_auth_tests.rs"]
mod tests;
