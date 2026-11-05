//! One call's way through the pipeline and its hooks: `beforeRequest` hooks on each attempt,
//! `afterResponse` hooks on the exchange, `poll` and the checks with their custom functions.

use std::sync::Arc;

use serde_json::json;
use tokio::sync::watch;
use tokio::time::{self, Instant};
use tower::ServiceExt;
use vc_diagnostics::Code;
use vc_interfaces::TransportError;
use vc_model::{Check, CheckKind, CheckResult, Exchange, Failure, HookRef, Poll, RequestHooks};

use super::call::{bind, subject};
use super::hooks::{self, HookBridge, HookStage};
use super::request;
use crate::check::{self, Judge, MatcherRegistry, Subject};
use crate::{Call, Pipeline, PipelineParts, Render};

/// A call's pipeline, with its api's hooks set, and the matchers that judge it.
pub struct Flight {
    pipeline: Pipeline,
    bridge: HookBridge,
    after: Vec<HookRef>,
    refused: watch::Receiver<Option<Failure>>,
    matchers: Arc<MatcherRegistry>,
}

impl Flight {
    /// A pipeline built from `parts` with `hooks`' `beforeRequest` hooks as its hooks stage;
    /// every hook runs through `bridge`.
    pub fn new(
        parts: PipelineParts,
        (hooks, bridge): (RequestHooks, HookBridge),
        matchers: Arc<MatcherRegistry>,
    ) -> Self {
        let (refusals, refused) = watch::channel(None);
        let parts = if hooks.before_request.is_empty() {
            parts
        } else {
            let stage = HookStage::new(bridge.clone(), hooks.before_request, refusals);
            PipelineParts { hooks: Arc::new(stage), ..parts }
        };
        Self { pipeline: crate::pipeline(parts), bridge, after: hooks.after_response, refused, matchers }
    }

    /// Sends `call`, then runs the `afterResponse` hooks on the exchange; what they return is
    /// not used.
    ///
    /// # Errors
    ///
    /// A transport error under its code, a `beforeRequest` hook's failure (V5115, V5116), or an
    /// `afterResponse` hook's.
    pub async fn send(&self, call: Call) -> Result<Exchange, Failure> {
        let delivery = match self.pipeline.clone().oneshot(call).await {
            Ok(delivery) => delivery,
            Err(never) => match never {},
        };
        let exchange = delivery.result.map_err(|error| self.failed(&error))?;
        if !self.after.is_empty() {
            let input =
                json!({ "request": request::sent(&exchange.request), "response": request::received(&exchange) });
            for hook in &self.after {
                self.bridge.call(hook, input.clone()).await.map_err(|error| hooks::failure(hook, &error))?;
            }
        }
        Ok(exchange)
    }

    /// Sends `call` until `poll.until` holds, waiting `poll.every` between sends; the first
    /// exchange that holds is the call's.
    ///
    /// # Errors
    ///
    /// As [`Self::send`], or V5104 when `poll.until` does not hold within `poll.timeout`.
    pub async fn poll(&self, render: &mut Render<'_>, (call, poll): (Call, &Poll)) -> Result<Exchange, Failure> {
        let deadline = Instant::now() + poll.timeout.get();
        loop {
            let exchange = self.send(call.clone()).await?;
            let response = bind(render, &exchange);
            let results = self.judge(render, (poll.until.checks.as_slice(), subject(&response, &exchange))).await;
            let Some(failure) = check::verdict(&results) else { return Ok(exchange) };
            if Instant::now() + poll.every.get() > deadline {
                let message = format!("`poll.until` did not hold within {:?}: {}", poll.timeout.get(), failure.message);
                return Err(Failure::new(message).with_code(Code::V5104.to_string()));
            }
            time::sleep(poll.every.get()).await;
        }
    }

    /// Evaluates `checks` against `subject`, running the custom ones with `{response, vars}`,
    /// masked by the run's redactor so neither the results nor a failure built from them shows
    /// a masked header.
    pub async fn judge(&self, render: &Render<'_>, (checks, subject): (&[Check], Subject<'_>)) -> Vec<CheckResult> {
        let mut results = check::evaluate(&Judge::new(&self.matchers, render), checks, subject);
        if checks.iter().any(|check| check.kind == CheckKind::Custom) {
            let input = json!({ "response": subject.response, "vars": render.vars() });
            hooks::judge(&self.bridge, checks, (results.as_mut_slice(), &input)).await;
        }
        let redactor = render.redactor();
        results.into_iter().map(|result| redactor.check(result)).collect()
    }

    /// A transport error as a failure; a refused attempt is the failure of the hook that
    /// refused it.
    fn failed(&self, error: &TransportError) -> Failure {
        let refusal = || self.refused.borrow().clone();
        let refused = if matches!(error, TransportError::InvalidRequest { .. }) { refusal() } else { None };
        refused.unwrap_or_else(|| Failure::new(error.to_string()).with_code(error.code().to_string()))
    }
}
