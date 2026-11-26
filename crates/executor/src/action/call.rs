use std::borrow::Cow;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::Value;
use tower::ServiceExt;
use vc_interfaces::SendContext;
use vc_model::{
    Action, ActionId, ActionKind, ActionOutcome, Api, Check, CheckKind, CheckResult, Exchange, Failure, Matcher,
    Namespace, Origin, Program, Protocol, SaveSpec, SourceSpan, StatusPattern,
};

use super::flight::{Flight, Halted};
use super::held::Held;
use super::hooks::{self, HookBridge};
use super::protocol::{ProtocolRuntime, ProtocolRuntimes};
use super::request;
use super::stream_call::{self, Asked};
use super::{ActionExecutor, ActionRequest, LiveStream};
use super::{auth, http_retry};
use crate::check::{self, Judge, MatcherRegistry, Subject};
use crate::{ActionReport, Call, PendingCleanup, Pipeline, PipelineParts, Redactor, Render, Saved};

/// The per-attempt limit when neither the api nor the config's `defaults` sets a `timeout`.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// Runs `call:` actions through the request pipeline.
///
/// It renders the request from the action's scope and sends it; with `poll`, it sends again
/// every `every` until the `until` checks hold or `timeout` passes (V5104). It then evaluates
/// the merged checks, with the api's success status (D386) when none checks the status. On a
/// response of that success status (`2xx`; `101` on a `websocket` api), it registers the api's
/// `cleanup` with its vars rendered against the response, whatever the checks or its
/// `expect.status` say (D71, D387), unless the action is `keep` or is itself a cleanup. Only when every
/// check holds does it save the api's `save` values, when the status matches the api's `saveOn`
/// (D191), and then the action's. A response with GraphQL errors registers no cleanup and, under
/// the default `saveOn`, runs no api `save` (V5129); its errors are listed as V5128 when no
/// errors check ran, the step's own or one it inherits from its api (D123, D365), and a 2xx one
/// its runtime cannot read fails (V5124, D345). Every warning is masked by the run's redactor.
#[derive(Clone)]
pub struct CallExecutor {
    parts: PipelineParts,
    matchers: Arc<MatcherRegistry>,
    runtimes: Arc<ProtocolRuntimes>,
}

impl std::fmt::Debug for CallExecutor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let runtimes = &self.runtimes;
        f.debug_struct("CallExecutor")
            .field("matchers", &self.matchers)
            .field("runtimes", runtimes)
            .finish_non_exhaustive()
    }
}

impl CallExecutor {
    /// An executor that sends through a pipeline built from `parts`, with the retry of each call
    /// set by its api's or the config's `http.retry` (D143), and judges with the
    /// standard matchers.
    #[must_use]
    pub fn new(parts: PipelineParts) -> Self {
        let runtimes = Arc::new(ProtocolRuntimes::standard());
        Self { parts, matchers: Arc::new(MatcherRegistry::standard()), runtimes }
    }

    /// The executor with `runtimes` in place of the standard ones.
    #[cfg(test)]
    #[must_use]
    pub fn with_runtimes(self, runtimes: ProtocolRuntimes) -> Self {
        Self { runtimes: Arc::new(runtimes), ..self }
    }

    /// The runtime of `api`'s type; one the registry lacks fails the call.
    fn runtime(&self, api: &Api) -> Result<Arc<dyn ProtocolRuntime>, Failure> {
        let missing = || Failure::new(format!("no runtime for {} api", api.protocol.named()));
        self.runtimes.get(api.protocol).ok_or_else(missing)
    }

    /// What sending `request` needs: its action and api, the render of its scope, the call
    /// rendered for the api's runtime, and its flight, with the api's auth and hooks.
    pub(super) fn begin<'r>(&self, request: &'r ActionRequest) -> Result<Begun<'r>, Failure> {
        let action =
            request.cx.action(request.action).ok_or_else(|| Failure::new("the action is not in the program"))?;
        let api = called(&request.cx.program, action)
            .ok_or_else(|| Failure::new("the action calls no api in the program"))?;
        let mut render = Render::new(request).map_err(request::failed)?;
        let timeout = timeout(render.program(), api, action);
        let runtime = self.runtime(api)?;
        let client_cert = auth::cert_choice(&request.cx.program, request.cx.client_certs(), (api, action))?;
        let call = Call {
            request: runtime.request(&render, api, (request.action, action))?,
            timeout,
            ctx: SendContext { client_cert, ..SendContext::default() },
        };
        let trip = self.flight(request, &mut render, (api, action, Arc::clone(&runtime)))?;
        Ok(Begun { action, api, render, runtime, call, trip })
    }

    async fn call(&self, request: &ActionRequest) -> Result<ActionReport, Halted> {
        let Begun { action, api, mut render, runtime, call, trip } = self.begin(request)?;
        let asked = Asked { action: (request.action, action), api };
        let answer = match (runtime.stream(), &action.poll) {
            (Some(_), _) => stream_call::run(&trip, &mut render, (call, asked)).await?,
            (None, Some(poll)) => trip.poll(&mut render, (call, poll)).await?.into(),
            (None, None) => trip.send(call).await?.into(),
        };
        let response = trip.answered(&mut render, &answer);
        let expected = judged(request, action, api);
        let mut checks = trip.judge(&render, (&*expected, subject(&response, &answer.exchange))).await;
        checks.extend(answer.checks.into_iter().map(|check| render.redactor().check(check)));
        let exchange = answer.exchange;
        let held = Held::new(runtime.errors(&response));
        let answered = Answered { request, action, api, exchange: &exchange };
        let report = self.settle(&render, &answered, (checks, &held))?;
        let warnings = masked(&request.cx.redactor, render.warnings().into_iter().chain(report.warnings)).await;
        Ok(ActionReport {
            response: Some(response),
            fakes: render.into_memo(),
            exchanges: vec![exchange],
            warnings,
            ..report
        })
    }

    /// The report of an `answered` call judged by `checks`: its cleanup and saves, each held back
    /// by the response's errors as `held` says (D345), and the warnings that gives (D361). A
    /// response its runtime could not read is the first failed check (V5124); a save held back
    /// while a check failed was not going to run (no V5129).
    fn settle(
        &self,
        render: &Render<'_>,
        answered: &Answered<'_>,
        (checks, held): (Vec<CheckResult>, &Held),
    ) -> Result<ActionReport, Failure> {
        let Answered { request, action, api, exchange } = *answered;
        let succeeded = api.success_status().matches(exchange.response.status.as_u16());
        let registers = !action.keep && request.frame.given.is_none() && succeeded;
        let cleanups = if registers && held.clean() { cleanup(render, api)? } else { None };
        let checks = held.reported(checks, (Origin::Action(request.action), action.span));
        let failure = check::verdict(&checks);
        let on_status = failure.is_none() && self.on_status(render, api, exchange);
        let api_saves = on_status && held.saves(api);
        let withheld = (registers && api.cleanup.is_some() && !held.clean())
            || (on_status && !api_saves && !api.save.values.is_empty());
        let specs: Vec<_> = api_saves.then_some(&api.save).into_iter().chain([&action.save]).collect();
        let saved = failure.map_or_else(|| saves(render, &specs), Err);
        let warnings = held.warnings(&checks, withheld).into_iter();
        let warnings = warnings.map(|warning| warning.at(request.action)).collect();
        let mut report =
            ActionReport { cleanups: cleanups.into_iter().collect(), checks, warnings, ..ActionReport::passed() };
        match saved {
            Ok(saved) => report.saved = saved,
            Err(failure) => report.outcome = ActionOutcome::Failed(failure),
        }
        Ok(report)
    }

    /// Whether the response's status matches the api's `saveOn` (D191).
    fn on_status(&self, render: &Render<'_>, api: &Api, exchange: &Exchange) -> bool {
        let status = Value::from(exchange.response.status.as_u16());
        let judge = Judge::new(&self.matchers, render);
        judge.matches(&api.save_on, Some(&status)) == Ok(true)
    }

    /// The call's flight: the pipeline from these parts with the api's retry (D143), auth and
    /// request hooks, judged with these matchers and read back through the api's `runtime`.
    fn flight(
        &self,
        request: &ActionRequest,
        render: &mut Render<'_>,
        (api, action, runtime): (&Api, &Action, Arc<dyn ProtocolRuntime>),
    ) -> Result<Flight, Failure> {
        let program = render.program();
        let retry = http_retry::policy(program, api, &request.cx.values.inputs().random);
        let prepared = auth::prepare(request, render, (api, action))?;
        let parts = auth::added(PipelineParts { retry, ..self.parts.clone() }, &request.cx, prepared);
        let bridge = HookBridge::new(self.parts.hook_runner.clone(), program)
            .with_secrets(request.cx.values.inputs().secrets.clone());
        let read = (Arc::clone(&self.matchers), runtime);
        Ok(Flight::new(parts, (hooks::request_hooks(program, api), bridge), read))
    }
}

/// A call about to be sent: what [`CallExecutor::begin`] gives.
pub(super) struct Begun<'r> {
    pub action: &'r Action,
    pub api: &'r Api,
    pub render: Render<'r>,
    pub runtime: Arc<dyn ProtocolRuntime>,
    pub call: Call,
    pub trip: Flight,
}

/// A call that got its answer: what it ran, as run.
#[derive(Clone, Copy)]
struct Answered<'a> {
    request: &'a ActionRequest,
    action: &'a Action,
    api: &'a Api,
    exchange: &'a Exchange,
}

/// `warnings` with the run's masked values hidden from their messages, by the built-in masking
/// and the rules, so the report holds none; the event bus masks them again on the way out
/// (`RedactingSink`), which is harmless.
async fn masked(redactor: &Redactor, warnings: impl Iterator<Item = Failure>) -> Vec<Failure> {
    let mut masked = Vec::new();
    for warning in warnings {
        masked.push(Failure { message: redactor.masked_text(&warning.message).await, ..warning });
    }
    masked
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
        let mut report = self.call(&request).await.unwrap_or_else(Halted::report);
        if let ActionOutcome::Failed(failure) = report.outcome {
            report.outcome = ActionOutcome::Failed(failure.at(request.action));
        }
        report
    }

    async fn open_live(&self, request: ActionRequest) -> Result<LiveStream, Failure> {
        self.live(&request).await
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

/// Binds `response` (as `runtime` reads it) and `request` for the checks, saves and cleanup vars;
/// returns the response.
pub fn bind(render: &mut Render<'_>, exchange: &Exchange, runtime: &dyn ProtocolRuntime) -> Arc<Value> {
    bound(render, exchange, runtime.response(exchange))
}

/// Binds `response` as the `response` root and `exchange`'s request as the `request` root.
pub fn bound(render: &mut Render<'_>, exchange: &Exchange, response: Value) -> Arc<Value> {
    let response = Arc::new(response);
    render.bind(Namespace::Response, Value::clone(&response));
    render.bind(Namespace::Request, request::sent(&exchange.request));
    response
}

/// What the checks read of `exchange`, with `response` as bound.
pub fn subject<'s>(response: &'s Value, exchange: &Exchange) -> Subject<'s> {
    Subject { response, size: u64::try_from(exchange.response.body.len()).unwrap_or(u64::MAX) }
}

/// The checks a call judges: all of the action's merged ones, or with `checks: api` in a load
/// iteration only those from the api's own `expect`; either way after the api's default status
/// check.
fn judged<'a>(request: &ActionRequest, action: &'a Action, api: &Api) -> Cow<'a, [Check]> {
    let (origin, success) = ((Origin::Api(api.id), api.span), api.success_status());
    if !request.frame.api_checks_only {
        return defaulted(Cow::Borrowed(&action.expect.checks), origin, success);
    }
    let own = action.expect.checks.iter().filter(|check| matches!(check.origin, Origin::Api(_))).cloned().collect();
    defaulted(Cow::Owned(own), origin, success)
}

/// An inline request's merged checks, after its `http` success status from `origin` when none
/// checks the status.
pub fn expected(action: &Action, origin: (Origin, SourceSpan)) -> Cow<'_, [Check]> {
    defaulted(Cow::Borrowed(&action.expect.checks), origin, Protocol::Http.success_status())
}

/// `checks` after a default `status: <success>` from `origin` when none checks the status.
fn defaulted(
    checks: Cow<'_, [Check]>,
    (origin, span): (Origin, SourceSpan),
    success: StatusPattern,
) -> Cow<'_, [Check]> {
    if checks.iter().any(|check| check.kind == CheckKind::Status) {
        return checks;
    }
    let matcher = Matcher::Status(vec![success]);
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
