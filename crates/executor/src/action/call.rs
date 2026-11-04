use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::Value;
use tower::ServiceExt;
use vc_diagnostics::Code;
use vc_interfaces::SendContext;
use vc_model::{Action, ActionId, ActionKind, ActionOutcome, Api, Exchange, Failure, Namespace, Program};

use super::request::{failed, received, rendered, sent};
use super::{ActionExecutor, ActionRequest};
use crate::{ActionReport, Call, PendingCleanup, PipelineParts, Render, Saved, pipeline};

/// The per-attempt limit when the api sets no `timeout`.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// Runs `call:` actions through the request pipeline.
///
/// It renders the request from the action's scope, sends it, saves the api's and then the
/// action's `save` values, and on a 2xx response registers the api's `cleanup` with its vars
/// rendered against the response, unless the action is `keep` or is itself a cleanup.
///
/// Until checks land, the status check is the default `2xx`.
#[derive(Clone)]
pub struct CallExecutor {
    parts: PipelineParts,
}

impl std::fmt::Debug for CallExecutor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CallExecutor").finish_non_exhaustive()
    }
}

impl CallExecutor {
    /// An executor that sends through a pipeline built from `parts`.
    #[must_use]
    pub const fn new(parts: PipelineParts) -> Self {
        Self { parts }
    }

    async fn call(&self, request: &ActionRequest) -> Result<ActionReport, Failure> {
        let action =
            request.cx.action(request.action).ok_or_else(|| Failure::new("the action is not in the program"))?;
        let api = called(&request.cx.program, action)
            .ok_or_else(|| Failure::new("the action calls no api in the program"))?;
        let mut render = Render::new(request).map_err(failed)?;
        let timeout = api.timeout.map_or(DEFAULT_TIMEOUT, vc_model::Duration::get);
        let call = Call { request: rendered(&render, api, action)?, timeout, ctx: SendContext::default() };
        let exchange = self.send(call).await?;
        let response = Arc::new(received(&exchange));
        render.bind(Namespace::Response, Value::clone(&response));
        render.bind(Namespace::Request, sent(&exchange.request));
        let registers = !action.keep && request.frame.given.is_none();
        let cleanups = if registers { cleanup(&render, api)? } else { None };
        let mut report = ActionReport { cleanups: cleanups.into_iter().collect(), ..ActionReport::passed() };
        match saves(&render, api, action) {
            Ok(saved) => report.saved = saved,
            Err(failure) => report.outcome = ActionOutcome::Failed(failure),
        }
        Ok(ActionReport { response: Some(response), fakes: render.into_memo(), ..report })
    }

    /// Sends `call`; a transport error or a status other than 2xx fails it.
    async fn send(&self, call: Call) -> Result<Exchange, Failure> {
        let delivery = match pipeline(self.parts.clone()).oneshot(call).await {
            Ok(delivery) => delivery,
            Err(never) => match never {},
        };
        let exchange =
            delivery.result.map_err(|error| Failure::new(error.to_string()).with_code(error.code().to_string()))?;
        let status = exchange.response.status;
        if !status.is_success() {
            let failure = Failure::new(format!("expected a 2xx status, got {status}"));
            return Err(failure.with_code(Code::V5101.to_string()));
        }
        Ok(exchange)
    }
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

/// The api's `save` values, then the action's, rendered against the response.
fn saves(render: &Render<'_>, api: &Api, action: &Action) -> Result<Saved, Failure> {
    let mut saved = Saved::new();
    for (name, value) in api.save.values.iter().chain(&action.save.values) {
        let name = render.program().str(name.sym()).unwrap_or_default().to_owned();
        saved.insert(name, render.value(value).map_err(failed)?);
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
        vars.insert(name.clone(), render.value(value).map_err(failed)?);
    }
    Ok(Some(PendingCleanup::new(action).with_vars(vars)))
}

#[cfg(test)]
#[path = "call_tests.rs"]
mod tests;
