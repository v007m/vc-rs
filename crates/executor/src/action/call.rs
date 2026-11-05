use std::borrow::Cow;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::Value;
use tokio::time::{self, Instant};
use tower::ServiceExt;
use vc_diagnostics::Code;
use vc_interfaces::SendContext;
use vc_model::{
    Action, ActionId, ActionKind, ActionOutcome, Api, Check, CheckKind, CheckResult, Exchange, Failure, Matcher,
    Namespace, Origin, Poll, Program, StatusPattern,
};

use super::http_retry;
use super::request;
use super::{ActionExecutor, ActionRequest};
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
/// check holds does it save the api's and then the action's `save` values.
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
        let timeout = timeout(render.program(), api);
        let call = Call { request: request::rendered(&render, api, action)?, timeout, ctx: SendContext::default() };
        let retry = http_retry::policy(render.program(), api, &request.cx.values.inputs().random);
        let pipeline = crate::pipeline(PipelineParts { retry, ..self.parts.clone() });
        let exchange = match &action.poll {
            Some(poll) => self.poll(&mut render, (&pipeline, call), poll).await?,
            None => send(&pipeline, call).await?,
        };
        let response = bind(&mut render, &exchange);
        let checks = self.judge(&render, &expected(action, api), subject(&response, &exchange));
        let registers = !action.keep && request.frame.given.is_none() && exchange.response.status.is_success();
        let cleanups = if registers { cleanup(&render, api)? } else { None };
        let saved = check::verdict(&checks).map_or_else(|| saves(&render, api, action), Err);
        let mut report = ActionReport { cleanups: cleanups.into_iter().collect(), checks, ..ActionReport::passed() };
        match saved {
            Ok(saved) => report.saved = saved,
            Err(failure) => report.outcome = ActionOutcome::Failed(failure),
        }
        Ok(ActionReport { response: Some(response), fakes: render.into_memo(), exchanges: vec![exchange], ..report })
    }

    /// Sends `call` until `poll.until` holds, waiting `poll.every` between sends; the first
    /// exchange that holds is the call's.
    async fn poll(
        &self,
        render: &mut Render<'_>,
        (pipeline, call): (&Pipeline, Call),
        poll: &Poll,
    ) -> Result<Exchange, Failure> {
        let deadline = Instant::now() + poll.timeout.get();
        loop {
            let exchange = send(pipeline, call.clone()).await?;
            let response = bind(render, &exchange);
            let Some(failure) = check::verdict(&self.judge(render, &poll.until.checks, subject(&response, &exchange)))
            else {
                return Ok(exchange);
            };
            if Instant::now() + poll.every.get() > deadline {
                let message = format!("`poll.until` did not hold within {:?}: {}", poll.timeout.get(), failure.message);
                return Err(Failure::new(message).with_code(Code::V5104.to_string()));
            }
            time::sleep(poll.every.get()).await;
        }
    }

    /// Evaluates `checks` against `subject`, masked by the run's redactor so neither the results
    /// nor a failure built from them shows a masked header.
    fn judge(&self, render: &Render<'_>, checks: &[Check], subject: Subject<'_>) -> Vec<CheckResult> {
        let results = check::evaluate(&Judge::new(&self.matchers, render), checks, subject);
        results.into_iter().map(|result| render.redactor().check(result)).collect()
    }
}

/// Sends `call` through `pipeline`; a transport error fails it.
async fn send(pipeline: &Pipeline, call: Call) -> Result<Exchange, Failure> {
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
        called(program, action).and_then(|api| api.cleanup).into_iter().collect()
    }
}

fn called<'p>(program: &'p Program, action: &Action) -> Option<&'p Api> {
    let ActionKind::Call(api) = &action.kind else { return None };
    program.apis.get(api)
}

/// The per-attempt limit: the api's `timeout`, else the config's `defaults.timeout`.
fn timeout(program: &Program, api: &Api) -> Duration {
    api.timeout.or(program.config.defaults.timeout).map_or(DEFAULT_TIMEOUT, vc_model::Duration::get)
}

/// Binds `response` and `request` for the checks, saves and cleanup vars; returns the response.
fn bind(render: &mut Render<'_>, exchange: &Exchange) -> Arc<Value> {
    let response = Arc::new(request::received(exchange));
    render.bind(Namespace::Response, Value::clone(&response));
    render.bind(Namespace::Request, request::sent(&exchange.request));
    response
}

fn subject<'s>(response: &'s Value, exchange: &Exchange) -> Subject<'s> {
    Subject { response, size: u64::try_from(exchange.response.body.len()).unwrap_or(u64::MAX) }
}

/// The action's merged checks, after the api's default `status: 2xx` when none checks the
/// status.
fn expected<'a>(action: &'a Action, api: &Api) -> Cow<'a, [Check]> {
    let checks = &action.expect.checks;
    let ActionKind::Call(id) = action.kind else { return Cow::Borrowed(checks) };
    if checks.iter().any(|check| check.kind == CheckKind::Status) {
        return Cow::Borrowed(checks);
    }
    let matcher = Matcher::Status(vec![StatusPattern::Class(2)]);
    let status = Check { kind: CheckKind::Status, target: None, matcher, origin: Origin::Api(id), span: api.span };
    Cow::Owned(std::iter::once(status).chain(checks.iter().cloned()).collect())
}

/// The api's `save` values, then the action's, rendered against the response.
fn saves(render: &Render<'_>, api: &Api, action: &Action) -> Result<Saved, Failure> {
    let mut saved = Saved::new();
    for (name, value) in api.save.values.iter().chain(&action.save.values) {
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
