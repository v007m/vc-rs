//! Inline requests: a `flow` auth's steps, written with the api fields in place of a `call:`.

use std::sync::Arc;

use async_trait::async_trait;
use vc_interfaces::SendContext;
use vc_model::{Action, ActionId, ActionKind, ActionOutcome, Failure, Origin, Program};

use super::call::{bind, expected, judge, saves, send, subject};
use super::request;
use super::{ActionExecutor, ActionRequest};
use crate::check::{self, MatcherRegistry};
use crate::{ActionReport, Call, PipelineParts, Render, own_limit};

/// Runs inline requests through the request pipeline, with no auth and the pipeline's own
/// retry. Each attempt's limit is the step's own `timeout`, else `defaults.timeout` (D192).
///
/// It judges the step's `expect` (with a default `status: 2xx` when none checks the status)
/// and, when every check holds, saves the step's `save` values.
#[derive(Clone)]
pub struct InlineExecutor {
    parts: PipelineParts,
    matchers: Arc<MatcherRegistry>,
}

impl std::fmt::Debug for InlineExecutor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InlineExecutor").field("matchers", &self.matchers).finish_non_exhaustive()
    }
}

impl InlineExecutor {
    /// An executor that sends through a pipeline built from `parts` and judges with the
    /// standard matchers.
    #[must_use]
    pub fn new(parts: PipelineParts) -> Self {
        Self { parts, matchers: Arc::new(MatcherRegistry::standard()) }
    }

    async fn send(&self, request: &ActionRequest) -> Result<ActionReport, Failure> {
        let action =
            request.cx.action(request.action).ok_or_else(|| Failure::new("the action is not in the program"))?;
        let ActionKind::Inline(inline) = &action.kind else { return Err(Failure::new("the action is not inline")) };
        let mut render = Render::new(request).map_err(request::failed)?;
        let timeout = own_limit(render.program(), action);
        let call =
            Call { request: request::inline(&render, (request.action, inline))?, timeout, ctx: SendContext::default() };
        let exchange = send(&crate::pipeline(self.parts.clone()), call).await?;
        let response = bind(&mut render, &exchange);
        let checks = expected(action, (Origin::Action(request.action), action.span));
        let checks = judge(&self.matchers, &render, (&checks, subject(&response, &exchange)));
        let mut report = ActionReport { checks, ..ActionReport::passed() };
        match check::verdict(&report.checks).map_or_else(|| saves(&render, &[&action.save]), Err) {
            Ok(saved) => report.saved = saved,
            Err(failure) => report.outcome = ActionOutcome::Failed(failure),
        }
        let warnings = render.warnings();
        Ok(ActionReport { response: Some(response), fakes: render.into_memo(), warnings, ..report })
    }
}

#[async_trait]
impl ActionExecutor for InlineExecutor {
    async fn execute(&self, request: ActionRequest) -> ActionReport {
        let mut report = self.send(&request).await.unwrap_or_else(ActionReport::failed);
        if let ActionOutcome::Failed(failure) = report.outcome {
            report.outcome = ActionOutcome::Failed(failure.at(request.action));
        }
        report
    }

    fn follows(&self, _program: &Program, _action: &Action) -> Vec<ActionId> {
        Vec::new()
    }
}

#[cfg(test)]
#[path = "inline_tests.rs"]
mod tests;
