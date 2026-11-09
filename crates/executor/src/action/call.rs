use std::borrow::Cow;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::Value;
use tower::ServiceExt;
use vc_interfaces::SendContext;
use vc_model::{
    Action, ActionId, ActionKind, ActionOutcome, Api, Check, CheckKind, CheckResult, Exchange, Failure, Matcher,
    Namespace, Origin, Program, SaveSpec, SourceSpan, StatusPattern,
};

use super::flight::Flight;
use super::hooks::{self, HookBridge};
use super::request;
use super::{ActionExecutor, ActionRequest};
use super::{auth, http_retry};
use crate::check::{self, Judge, MatcherRegistry, Subject};
use crate::{ActionReport, Call, PendingCleanup, Pipeline, PipelineParts, Render, Saved};

/// The per-attempt limit when neither the api nor the config's `defaults` sets a `timeout`.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// Runs `call:` actions through the request pipeline.
///
/// It renders the request from the action's scope and sends it; with `poll`, it sends again
/// every `every` until the `until` checks hold or `timeout` passes (V5104). It then evaluates
/// the merged checks, with the api's default `status: 2xx` when none checks the status. On a
/// 2xx response it registers the api's `cleanup` with its vars rendered against the response,
/// whatever the checks say, unless the action is `keep` or is itself a cleanup. Only when every
/// check holds does it save the api's `save` values, when the status matches the api's `saveOn`
/// (D191), and then the action's.
#[derive(Clone)]
pub struct CallExecutor {
    parts: PipelineParts,
    matchers: Arc<MatcherRegistry>,
}

impl std::fmt::Debug for CallExecutor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CallExecutor").field("matchers", &self.matchers).finish_non_exhaustive()
    }
}

impl CallExecutor {
    /// An executor that sends through a pipeline built from `parts`, with the retry of each call
    /// set by its api's or the config's `http.retry` (D143), and judges with the
    /// standard matchers.
    #[must_use]
    pub fn new(parts: PipelineParts) -> Self {
        Self { parts, matchers: Arc::new(MatcherRegistry::standard()) }
    }

    async fn call(&self, request: &ActionRequest) -> Result<ActionReport, Failure> {
        let action =
            request.cx.action(request.action).ok_or_else(|| Failure::new("the action is not in the program"))?;
        let api = called(&request.cx.program, action)
            .ok_or_else(|| Failure::new("the action calls no api in the program"))?;
        let mut render = Render::new(request).map_err(request::failed)?;
        let timeout = timeout(render.program(), api, action);
        let call = Call { request: request::rendered(&render, api, action)?, timeout, ctx: SendContext::default() };
        let trip = self.flight(request, &render, (api, action))?;
        let exchange = match &action.poll {
            Some(poll) => trip.poll(&mut render, (call, poll)).await?,
            None => trip.send(call).await?,
        };
        let response = bind(&mut render, &exchange);
        let expected = judged(request, action, api);
        let checks = trip.judge(&render, (&*expected, subject(&response, &exchange))).await;
        let registers = !action.keep && request.frame.given.is_none() && exchange.response.status.is_success();
        let cleanups = if registers { cleanup(&render, api)? } else { None };
        let specs: Vec<_> = self.api_saves(&render, api, &exchange).into_iter().chain([&action.save]).collect();
        let saved = check::verdict(&checks).map_or_else(|| saves(&render, &specs), Err);
        let mut report = ActionReport { cleanups: cleanups.into_iter().collect(), checks, ..ActionReport::passed() };
        match saved {
            Ok(saved) => report.saved = saved,
            Err(failure) => report.outcome = ActionOutcome::Failed(failure),
        }
        let warnings = render.warnings();
        Ok(ActionReport {
            response: Some(response),
            fakes: render.into_memo(),
            exchanges: vec![exchange],
            warnings,
            ..report
        })
    }

    /// The api's `save`, when the response's status matches its `saveOn` (D191).
    fn api_saves<'a>(&self, render: &Render<'_>, api: &'a Api, exchange: &Exchange) -> Option<&'a SaveSpec> {
        let status = Value::from(exchange.response.status.as_u16());
        let judge = Judge::new(&self.matchers, render);
        (judge.matches(&api.save_on, Some(&status)) == Ok(true)).then_some(&api.save)
    }

    /// The call's flight: the pipeline from these parts with the api's retry (D143), auth and
    /// request hooks, judged with these matchers.
    fn flight(&self, request: &ActionRequest, render: &Render<'_>, call: (&Api, &Action)) -> Result<Flight, Failure> {
        let program = render.program();
        let retry = http_retry::policy(program, call.0, &request.cx.values.inputs().random);
        let prepared = auth::prepare(request, render, call)?;
        let parts = auth::added(PipelineParts { retry, ..self.parts.clone() }, &request.cx, prepared);
        let bridge = HookBridge::new(self.parts.hook_runner.clone(), program);
        Ok(Flight::new(parts, (hooks::request_hooks(program, call.0), bridge), Arc::clone(&self.matchers)))
    }
}

/// Evaluates `checks` against `subject`, masked by the run's redactor so neither the results
/// nor a failure built from them shows a masked header.
pub fn judge(
    matchers: &MatcherRegistry,
    render: &Render<'_>,
    (checks, subject): (&[Check], Subject<'_>),
) -> Vec<CheckResult> {
    let results = check::evaluate(&Judge::new(matchers, render), checks, subject);
    results.into_iter().map(|result| render.redactor().check(result)).collect()
}

/// Sends `call` through `pipeline`; a transport error fails it.
pub async fn send(pipeline: &Pipeline, call: Call) -> Result<Exchange, Failure> {
    let delivery = match pipeline.clone().oneshot(call).await {
        Ok(delivery) => delivery,
        Err(never) => match never {},
    };
    delivery.result.map_err(|error| Failure::new(error.to_string()).with_code(error.code().to_string()))
}

#[async_trait]
impl ActionExecutor for CallExecutor {
    async fn execute(&self, request: ActionRequest) -> ActionReport {
        let mut report = self.call(&request).await.unwrap_or_else(ActionReport::failed);
        if let ActionOutcome::Failed(failure) = report.outcome {
            report.outcome = ActionOutcome::Failed(failure.at(request.action));
        }
        report
    }

    fn follows(&self, program: &Program, action: &Action) -> Vec<ActionId> {
        let Some(api) = called(program, action) else { return Vec::new() };
        api.cleanup.into_iter().chain(auth::steps(program, (api, action))).collect()
    }
}

/// The api a `call:` action calls; `None` for any other action.
pub fn called<'p>(program: &'p Program, action: &Action) -> Option<&'p Api> {
    let ActionKind::Call(api) = &action.kind else { return None };
    program.apis.get(api)
}

/// The per-attempt limit: the action's own `timeout` (D192), else the api's, else the config's
/// `defaults.timeout`.
fn timeout(program: &Program, api: &Api, action: &Action) -> Duration {
    let limit = action.timeout.or(api.timeout).or(program.config.defaults.timeout);
    limit.map_or(DEFAULT_TIMEOUT, vc_model::Duration::get)
}

/// Binds `response` and `request` for the checks, saves and cleanup vars; returns the response.
pub fn bind(render: &mut Render<'_>, exchange: &Exchange) -> Arc<Value> {
    let response = Arc::new(request::received(exchange));
    render.bind(Namespace::Response, Value::clone(&response));
    render.bind(Namespace::Request, request::sent(&exchange.request));
    response
}

/// What the checks read of `exchange`, with `response` as bound.
pub fn subject<'s>(response: &'s Value, exchange: &Exchange) -> Subject<'s> {
    Subject { response, size: u64::try_from(exchange.response.body.len()).unwrap_or(u64::MAX) }
}

/// The checks a call judges: all of the action's merged ones, or with `checks: api` in a load
/// iteration only those from the api's own `expect`; either way after a default `status: 2xx`.
fn judged<'a>(request: &ActionRequest, action: &'a Action, api: &Api) -> Cow<'a, [Check]> {
    let origin = (Origin::Api(api.id), api.span);
    if !request.frame.api_checks_only {
        return expected(action, origin);
    }
    let own = action.expect.checks.iter().filter(|check| matches!(check.origin, Origin::Api(_))).cloned().collect();
    defaulted(Cow::Owned(own), origin)
}

/// The action's merged checks, after a default `status: 2xx` from `origin` when none checks
/// the status.
pub fn expected(action: &Action, origin: (Origin, SourceSpan)) -> Cow<'_, [Check]> {
    defaulted(Cow::Borrowed(&action.expect.checks), origin)
}

/// `checks` after a default `status: 2xx` from `origin` when none checks the status.
fn defaulted(checks: Cow<'_, [Check]>, (origin, span): (Origin, SourceSpan)) -> Cow<'_, [Check]> {
    if checks.iter().any(|check| check.kind == CheckKind::Status) {
        return checks;
    }
    let matcher = Matcher::Status(vec![StatusPattern::Class(2)]);
    let status = Check { kind: CheckKind::Status, target: None, matcher, origin, span };
    Cow::Owned(std::iter::once(status).chain(checks.iter().cloned()).collect())
}

/// The `save` values of each of `specs` in turn (an api's, then the action's), rendered
/// against the response.
pub fn saves(render: &Render<'_>, specs: &[&SaveSpec]) -> Result<Saved, Failure> {
    let mut saved = Saved::new();
    for (name, value) in specs.iter().flat_map(|spec| &spec.values) {
        let name = render.program().str(name.sym()).unwrap_or_default().to_owned();
        saved.insert(name, render.value(value).map_err(request::failed)?);
    }
    Ok(saved)
}

/// The api's `cleanup`, with its vars rendered now against the response, in name order.
fn cleanup(render: &Render<'_>, api: &Api) -> Result<Option<PendingCleanup>, Failure> {
    let Some(action) = api.cleanup else { return Ok(None) };
    let defined = render.values().action(action).unwrap_or_default();
    let mut defined: Vec<_> = defined.iter().collect();
    defined.sort_unstable_by_key(|(name, _)| *name);
    let mut vars = Saved::new();
    for (name, value) in defined {
        vars.insert(name.clone(), render.value(value).map_err(request::failed)?);
    }
    Ok(Some(PendingCleanup::new(action).with_vars(vars)))
}

#[cfg(test)]
#[path = "call_tests.rs"]
mod tests;
