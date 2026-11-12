//! The web UI's auth test (LLD `TestAuth`): obtains the credentials of one auth as a call of an
//! api would, through the run's token cache, without sending the call. Only what the credentials
//! are called and how long they hold leave; a value never does, and a failure is masked.

use std::sync::Arc;
use std::time::Duration;

use vc_executor::{ActionCx, ActionRequest, chosen_auth, probe_auth, probe_origin};
use vc_model::{Action, ActionId, ActionKind, AuthId, Failure, Owner, Program};

use crate::call::{called, named_api, pushed};
use crate::explain::did_you_mean;
use crate::ports::SecretSource;
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
    /// [`EngineError::Selection`] for an unknown auth or api, an auth no call uses, or a named api
    /// that does not use it and sends to an origin no api that uses it sends to (then nothing is
    /// sent); [`EngineError::Invalid`] and [`EngineError::Setting`] as for `vc call`, and the
    /// error of a secret that does not resolve. Credentials that cannot be obtained are no
    /// error: they are [`AuthTested::error`], masked.
    pub async fn test_auth(&self, request: &TestAuthRequest) -> Result<AuthTested, EngineError> {
        let layers =
            CallRequest { profiles: request.profiles.clone(), sets: request.sets.clone(), ..CallRequest::default() };
        let mut prepared = self.adhoc(&layers)?;
        let probe = probe_call(&mut prepared.program, request, prepared.names.join(", "))?;
        let secrets = self.secrets(SecretSource::of(&prepared)).await?;
        let actions: Vec<ActionId> = probe.target.into_iter().chain(probe.senders.iter().copied()).collect();
        let cx = self.adhoc_cx(&prepared, (&actions, 0, secrets)).await?;
        let through = sender(&cx, &probe, &request.auth).await?;
        let probed = probe_auth(&ActionRequest::run(&cx, through)).await;
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
            Err(failure) => AuthTested { error: Some(cx.redactor.masked_text(&failure.message).await), ..tested },
        })
    }
}

/// The calls that send an auth, the first the one to obtain its credentials through, and the
/// call of a named api that does not use the auth, which only picks the sender by its origin.
struct Probe {
    /// The api's ID.
    api: String,
    /// The named api's call, when the api does not use the auth itself.
    target: Option<ActionId>,
    /// The calls that use the auth, each in its own scope: by api ID, an api's own first.
    senders: Vec<ActionId>,
    /// The profiles the test runs under, for a refusal.
    profiles: String,
}

/// Adds the calls that send the auth and, for a named api that does not use it, the api's call.
fn probe_call(program: &mut Arc<Program>, request: &TestAuthRequest, profiles: String) -> Result<Probe, EngineError> {
    let auth = named_auth(program, &request.auth)?;
    let users = users(program, auth);
    let first = users.first().map(|(api, _)| api.clone()).ok_or_else(|| unused(&request.auth))?;
    let pushed_all = |program: &mut Arc<Program>, users: Vec<(String, Action)>| -> Result<Vec<ActionId>, EngineError> {
        users.into_iter().map(|(_, action)| pushed(program, action)).collect()
    };
    let Some(id) = &request.api else {
        let senders = pushed_all(program, users)?;
        return Ok(Probe { api: first, target: None, senders, profiles });
    };
    let api = named_api(program, id)?;
    let action = called(api, true);
    if chosen_auth(program, (api, &action)) == Some(auth) {
        let senders = vec![pushed(program, action)?];
        return Ok(Probe { api: id.clone(), target: None, senders, profiles });
    }
    let target = Some(pushed(program, action)?);
    Ok(Probe { api: id.clone(), target, senders: pushed_all(program, users)?, profiles })
}

/// The origin a call sends to, or why its URL does not render.
type Origin = Result<String, Failure>;

/// The call to obtain the credentials through: the first sender, or, for a named api that does
/// not use the auth, the first sender on the api's origin, so the auth renders in the scope of a
/// call a run makes and a test never takes credentials to a host a run would not send them to.
async fn sender(cx: &Arc<ActionCx>, probe: &Probe, auth: &str) -> Result<ActionId, EngineError> {
    let first = probe.senders.first().copied().ok_or_else(|| unused(auth))?;
    let Some(target) = probe.target else { return Ok(first) };
    let origins: Vec<(ActionId, Origin)> =
        probe.senders.iter().map(|&sender| (sender, probe_origin(&ActionRequest::run(cx, sender)))).collect();
    let wanted = probe_origin(&ActionRequest::run(cx, target));
    if let Ok(origin) = &wanted
        && let Some((found, _)) = origins.iter().find(|(_, sent)| sent.as_ref().ok() == Some(origin))
    {
        return Ok(*found);
    }
    let rendered: Vec<Origin> = origins.into_iter().map(|(_, origin)| origin).collect();
    let (reason, help) = refusal((probe, auth), &wanted, &rendered);
    Err(EngineError::Selection {
        reason: cx.redactor.masked_text(&reason).await,
        help: Some(cx.redactor.masked_text(&help).await),
    })
}

/// Why the named api is not given `auth`, and what to do: name an api on a sender's origin, or,
/// when no sender's URL renders, the profile and the reason.
fn refusal((probe, auth): (&Probe, &str), wanted: &Origin, rendered: &[Origin]) -> (String, String) {
    let reason = match wanted {
        Ok(origin) => format!(
            "`{}` sends to `{origin}`, where no api that uses `{auth}` sends; it is not given `{auth}`",
            probe.api
        ),
        Err(failure) => format!("`{}` is not given `{auth}`: {}", probe.api, failure.message),
    };
    let mut allowed: Vec<&str> = rendered.iter().filter_map(|origin| origin.as_deref().ok()).collect();
    allowed.sort_unstable();
    allowed.dedup();
    let help = if allowed.is_empty() {
        let why = rendered.iter().find_map(|origin| origin.as_ref().err()).map(|failure| failure.message.as_str());
        format!(
            "no api that uses `{auth}` has a base URL under the profile `{}`: {}",
            probe.profiles,
            why.unwrap_or_default()
        )
    } else {
        let shown = allowed.iter().map(|origin| format!("`{origin}`")).collect::<Vec<_>>().join(", ");
        format!("name an api on {shown}, where `{auth}` is sent")
    };
    (reason, help)
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

/// The calls that send `auth`, by api ID, each in its own scope: each api that uses it, then
/// each test, step or action whose own `auth` is it, as a call of the api it calls.
fn users(program: &Program, auth: AuthId) -> Vec<(String, Action)> {
    let own = program.apis.values().map(|api| (api, called(api, true)));
    let written = program.actions.iter().filter_map(|action| {
        let ActionKind::Call(id) = &action.kind else { return None };
        let api = program.apis.get(id)?;
        action.auth.is_some().then(|| (api, Action { owner: Owner::Run, ..action.clone() }))
    });
    let mut users: Vec<(String, Action)> = own
        .chain(written)
        .filter(|(api, action)| chosen_auth(program, (api, action)) == Some(auth))
        .filter_map(|(api, action)| Some((program.str(api.id.sym())?.to_owned(), action)))
        .collect();
    users.sort_by(|(one, _), (other, _)| one.cmp(other));
    users
}

#[cfg(test)]
#[path = "test_auth_tests.rs"]
mod tests;
