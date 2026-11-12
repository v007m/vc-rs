//! The web UI's auth test (LLD `TestAuth`): obtains the credentials of one auth as a call of an
//! api would, through the run's token cache, without sending the call. Only what the credentials
//! are called and how long they hold leave; a value never does, and a failure is masked.

use std::iter;
use std::sync::Arc;
use std::time::Duration;

use vc_executor::{ActionCx, ActionRequest, chosen_auth, probe_auth, probe_origin};
use vc_model::{Action, ActionId, AuthId, AuthRef, Program, VarDefs};

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
    /// An api that does not use it is given it only when it sends to the origin of one that does.
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
    /// [`EngineError::Selection`] for an unknown auth or api, an auth no api uses, or a named api
    /// that does not use it and sends to an origin no api that uses it sends to (then nothing is
    /// sent); [`EngineError::Invalid`] and [`EngineError::Setting`] as for `vc call`, and the
    /// error of a secret that does not resolve. Credentials that cannot be obtained are no
    /// error: they are [`AuthTested::error`].
    pub async fn test_auth(&self, request: &TestAuthRequest) -> Result<AuthTested, EngineError> {
        let layers =
            CallRequest { profiles: request.profiles.clone(), sets: request.sets.clone(), ..CallRequest::default() };
        let mut prepared = self.adhoc(&layers)?;
        let probe = probe_call(&mut prepared.program, request)?;
        let secrets = self.secrets(&prepared.program.config.secrets, &prepared.environment).await?;
        let actions: Vec<ActionId> = iter::once(probe.action).chain(probe.senders.iter().copied()).collect();
        let cx = self.adhoc_cx(&prepared, (&actions, 0, secrets)).await?;
        same_origin(&cx, &probe, &request.auth).await?;
        let probed = probe_auth(&ActionRequest::run(&cx, probe.action)).await;
        let tested =
            AuthTested { api: probe.api, notices: std::mem::take(&mut prepared.notices), ..AuthTested::default() };
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

/// The call that obtains an auth's credentials, and the calls of the apis that use the auth
/// when the call is given it, whose origins bound where it may go.
struct Probe {
    /// The api's ID.
    api: String,
    action: ActionId,
    /// None when the api uses the auth itself.
    senders: Vec<ActionId>,
}

/// Adds the action that obtains the auth's credentials for the api, and, when the api is given
/// the auth, the calls of the apis that use it.
fn probe_call(program: &mut Arc<Program>, request: &TestAuthRequest) -> Result<Probe, EngineError> {
    let auth = named_auth(program, &request.auth)?;
    let mut users = users(program, auth).into_iter();
    let Some(id) = &request.api else {
        let (api, action) = users.next().ok_or_else(|| unused(&request.auth))?;
        return Ok(Probe { api, action: pushed(program, action)?, senders: Vec::new() });
    };
    let api = named_api(program, id)?;
    let mut action = called(api, true);
    let given = chosen_auth(program, (api, &action)) != Some(auth);
    let senders = if given {
        action.auth = Some(AuthRef::Use { id: auth, vars: VarDefs::default() });
        let senders: Vec<Action> = users.map(|(_, action)| action).collect();
        if senders.is_empty() {
            return Err(unused(&request.auth));
        }
        senders.into_iter().map(|sender| pushed(program, sender)).collect::<Result<_, _>>()?
    } else {
        Vec::new()
    };
    Ok(Probe { api: id.clone(), action: pushed(program, action)?, senders })
}

/// Refuses a call given an auth that sends to an origin none of the auth's own users send to,
/// so a test never takes credentials to a host a run would not send them to.
async fn same_origin(cx: &Arc<ActionCx>, probe: &Probe, auth: &str) -> Result<(), EngineError> {
    if probe.senders.is_empty() {
        return Ok(());
    }
    let mut allowed: Vec<String> =
        probe.senders.iter().filter_map(|&sender| probe_origin(&ActionRequest::run(cx, sender)).ok()).collect();
    allowed.sort();
    allowed.dedup();
    let reason = match probe_origin(&ActionRequest::run(cx, probe.action)) {
        Ok(origin) if allowed.contains(&origin) => return Ok(()),
        Ok(origin) => format!(
            "`{}` sends to `{origin}`, where no api that uses `{auth}` sends; it is not given `{auth}`",
            probe.api
        ),
        Err(failure) => format!("`{}` is not given `{auth}`: {}", probe.api, failure.message),
    };
    let shown = allowed.iter().map(|origin| format!("`{origin}`")).collect::<Vec<_>>().join(", ");
    let help = format!("name an api on {shown}, where `{auth}` is sent");
    Err(EngineError::Selection {
        reason: cx.redactor.masked_text(&reason).await,
        help: Some(cx.redactor.masked_text(&help).await),
    })
}

/// The refusal of an auth no api uses.
fn unused(auth: &str) -> EngineError {
    EngineError::Selection {
        reason: format!("no api uses the auth `{auth}`"),
        help: Some("name the api to obtain its credentials for".to_owned()),
    }
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

/// The apis that use `auth`, by ID, each with its call.
fn users(program: &Program, auth: AuthId) -> Vec<(String, Action)> {
    let mut users: Vec<(String, Action)> = program
        .apis
        .values()
        .filter_map(|api| {
            let action = called(api, true);
            let id = program.str(api.id.sym())?;
            (chosen_auth(program, (api, &action)) == Some(auth)).then(|| (id.to_owned(), action))
        })
        .collect();
    users.sort_by(|(one, _), (other, _)| one.cmp(other));
    users
}

#[cfg(test)]
#[path = "test_auth_tests.rs"]
mod tests;
