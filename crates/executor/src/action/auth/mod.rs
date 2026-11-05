//! Auth for a call (LLD "Auth and secrets", D106, D141, D168): which definition applies, its
//! vars and settings rendered for this call, and the stages that put its credentials on the
//! request and fetch them again when a response says they went stale.

mod flow;
mod stage;
mod token_cache;

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::{Map, Value};
use vc_interfaces::{AuthError, AuthProvider, AuthRequest, Credentials};
use vc_model::{Action, ActionId, Api, AuthDef, AuthRef, Failure, Program, VarDefs};

use super::request;
use crate::{ActionCx, ActionRequest, PipelineParts, Render, Saved};

use flow::FlowRunner;
use stage::{AuthStage, RefreshOn};
pub use token_cache::{AuthKey, TokenCache};

/// The setting a `flow` keeps its `apply` in; it renders after the steps, not before.
const APPLY: &str = "apply";

/// The provider a run has until the engine gives it one: it knows no auth type.
#[derive(Clone, Copy, Debug, Default)]
#[non_exhaustive]
pub struct NoAuth;

#[async_trait]
impl AuthProvider for NoAuth {
    async fn credentials(&self, request: &AuthRequest) -> Result<Credentials, AuthError> {
        Err(AuthError::Unknown { kind: request.kind.clone() })
    }
}

/// The auth of one call, ready to become pipeline stages.
pub struct Prepared {
    key: AuthKey,
    request: AuthRequest,
    refresh_before: Duration,
    refresh_on: Vec<u16>,
}

/// The auth the call `request` makes uses, if any, with its vars and settings rendered.
///
/// # Errors
///
/// A V5111 [`Failure`] when the definition's vars or settings do not render.
pub fn prepare(
    request: &ActionRequest,
    render: &Render<'_>,
    call: (&Api, &Action),
) -> Result<Option<Prepared>, Failure> {
    let program = render.program();
    let Some((def, vars)) = chosen(program, call) else { return Ok(None) };
    let given = Arc::new(given(render, def, vars)?);
    let settings = settings(request, def, &given)?;
    let name = program.str(def.id.sym()).unwrap_or_default();
    let key = request.cx.tokens.key(name, &Value::Object(Saved::clone(&given)), &Value::Object(settings.clone()));
    let mut auth = AuthRequest::new(name, def.kind.clone(), settings);
    if !def.steps.is_empty() {
        auth = auth.with_flow(Arc::new(FlowRunner::new(Arc::clone(&request.cx), def, given)));
    }
    let (refresh_before, refresh_on) = (def.refresh_before.get(), def.refresh_on.clone());
    Ok(Some(Prepared { key, request: auth, refresh_before, refresh_on }))
}

/// `parts` with `prepared` put in: the auth stage, and a retry that fetches the credentials
/// again once when a response has a `refreshOn` status.
#[must_use]
pub fn added(parts: PipelineParts, cx: &ActionCx, prepared: Option<Prepared>) -> PipelineParts {
    let Some(prepared) = prepared else { return parts };
    let tokens = Arc::clone(&cx.tokens);
    let retry = RefreshOn::new(parts.retry, (Arc::clone(&tokens), prepared.key), prepared.refresh_on.clone());
    let stage = AuthStage::new(prepared, tokens, (Arc::clone(&cx.auth), Arc::clone(&cx.redactor)));
    PipelineParts { retry: Arc::new(retry), auth: Arc::new(stage), ..parts }
}

/// The steps of the `flow` the call uses, so the start-up check reaches them.
#[must_use]
pub fn steps(program: &Program, call: (&Api, &Action)) -> Vec<ActionId> {
    chosen(program, call).map(|(def, _)| def.steps.clone()).unwrap_or_default()
}

/// The definition the call uses and the vars its use site passes: the first of the action's,
/// the api's and the api file's `auth` that does not inherit.
fn chosen<'p>(program: &'p Program, (api, action): (&'p Api, &'p Action)) -> Option<(&'p AuthDef, &'p VarDefs)> {
    let group = program.api_groups.get(&api.group).map(|group| &group.auth);
    let mut refs = [action.auth.as_ref(), Some(&api.auth), group].into_iter().flatten();
    let AuthRef::Use { id, vars } = refs.find(|auth| !matches!(auth, AuthRef::Inherit))? else { return None };
    Some((program.auth.get(id)?, vars))
}

/// The definition's vars, with the use site's over them, rendered where the call renders.
fn given(render: &Render<'_>, def: &AuthDef, vars: &VarDefs) -> Result<Saved, Failure> {
    let (values, program) = (render.values(), render.program());
    let mut defined = values.layer(program, &def.vars);
    defined.extend(values.layer(program, vars));
    let mut defined: Vec<_> = defined.into_iter().collect();
    defined.sort_unstable_by(|(left, _), (right, _)| left.cmp(right));
    let mut given = Saved::new();
    for (name, value) in defined {
        given.insert(name, render.value(&value).map_err(request::failed)?);
    }
    Ok(given)
}

/// The definition's settings, rendered with its vars found first.
fn settings(request: &ActionRequest, def: &AuthDef, given: &Arc<Saved>) -> Result<Map<String, Value>, Failure> {
    let render = Render::new(request).map_err(request::failed)?.with_given(Arc::clone(given));
    let mut settings = Map::new();
    for (key, value) in def.settings.iter().filter(|(key, _)| key.as_str() != APPLY) {
        settings.insert(key.clone(), render.value(value).map_err(request::failed)?);
    }
    Ok(settings)
}

#[cfg(test)]
#[path = "auth_tests.rs"]
mod tests;
