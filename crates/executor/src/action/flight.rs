//! One call's way through the pipeline and its hooks: `beforeRequest` hooks on each attempt,
//! `afterResponse` hooks on the exchange, `poll`, a stream's open and the checks with their
//! custom functions.

use std::sync::Arc;

use serde_json::{Value, json};
use tokio::sync::watch;
use tokio::time::{self, Instant};
use tower::ServiceExt;
use vc_diagnostics::Code;
use vc_interfaces::TransportError;
use vc_model::{Check, CheckKind, CheckResult, Exchange, Failure, HookRef, Poll, RequestHooks};

use super::call::{bind, bound, subject};
use super::hooks::{self, HookBridge, HookStage};
use super::protocol::ProtocolRuntime;
use super::request;
use crate::check::{self, Judge, MatcherRegistry, Subject};
use crate::{ActionReport, Call, Opening, Pipeline, PipelineParts, Render, StreamPipeline};

/// A call's pipeline, with its api's hooks set, the matchers that judge it, and its api type's
/// runtime, which reads its response.
pub struct Flight {
    pipeline: Pipeline,
    streams: Option<StreamPipeline>,
    bridge: HookBridge,
    after: Vec<HookRef>,
    refused: watch::Receiver<Option<Failure>>,
    matchers: Arc<MatcherRegistry>,
    runtime: Arc<dyn ProtocolRuntime>,
}

/// Why a call stopped before its checks: the failure, and the exchanges it got until then,
/// which its events still show.
pub struct Halted {
    failure: Failure,
    exchanges: Vec<Exchange>,
}

/// What a call answered: its exchange and, for a stream call, its `response.exchange` items and
/// the checks its exchange made (V5130, V5131).
pub struct Answer {
    /// The exchange: the response and, for a stream call, its messages.
    pub exchange: Exchange,
    /// `response.exchange`, for a stream call (D380).
    pub items: Option<Vec<Value>>,
    /// The checks the stream's exchange made.
    pub checks: Vec<CheckResult>,
}

impl Answer {
    /// The `response` root: as `runtime` reads the exchange, with its `exchange` items.
    pub fn response(&self, runtime: &dyn ProtocolRuntime) -> Value {
        let mut response = runtime.response(&self.exchange);
        if let (Some(items), Some(root)) = (&self.items, response.as_object_mut()) {
            drop(root.insert("exchange".to_owned(), Value::Array(items.clone())));
        }
        response
    }
}

impl From<Exchange> for Answer {
    fn from(exchange: Exchange) -> Self {
        Self { exchange, items: None, checks: Vec::new() }
    }
}

impl From<Failure> for Halted {
    fn from(failure: Failure) -> Self {
        Self { failure, exchanges: Vec::new() }
    }
}

impl Halted {
    /// `failure`, after `exchange` was made.
    pub fn with(failure: Failure, exchange: Exchange) -> Self {
        Self { failure, exchanges: vec![exchange] }
    }

    /// The failed call's report, with the exchanges it got.
    pub fn report(self) -> ActionReport {
        ActionReport { exchanges: self.exchanges, ..ActionReport::failed(self.failure) }
    }
}

impl Flight {
    /// A pipeline built from `parts` with `hooks`' `beforeRequest` hooks as its hooks stage;
    /// every hook runs through `bridge`.
    pub fn new(
        parts: PipelineParts,
        (hooks, bridge): (RequestHooks, HookBridge),
        (matchers, runtime): (Arc<MatcherRegistry>, Arc<dyn ProtocolRuntime>),
    ) -> Self {
        let (refusals, refused) = watch::channel(None);
        let parts = if hooks.before_request.is_empty() {
            parts
        } else {
            let stage = HookStage::new(bridge.clone(), hooks.before_request, refusals);
            PipelineParts { hooks: Arc::new(stage), ..parts }
        };
        let mode = runtime.stream().map(super::stream::StreamRuntime::mode);
        let opens = parts.streams.clone().zip(mode);
        let streams = opens.map(|opens| crate::stream_pipeline(parts.clone(), opens));
        let pipeline = crate::pipeline(parts);
        Self { pipeline, streams, bridge, after: hooks.after_response, refused, matchers, runtime }
    }

    /// The runtime of the api type called.
    pub fn runtime(&self) -> &dyn ProtocolRuntime {
        &*self.runtime
    }

    /// The matchers that judge the call.
    pub fn matchers(&self) -> &MatcherRegistry {
        &self.matchers
    }

    /// Opens a stream for `call` through the stream pipeline (D420); its `afterResponse` hooks
    /// run once the exchange is done (D423).
    ///
    /// # Errors
    ///
    /// When the open failed, or when the engine has no stream transport.
    pub async fn open(&self, call: Call) -> Result<Opening, Failure> {
        let protocol = self.runtime.protocol();
        let none = || Failure::new(format!("`{}` calls need a stream transport, and none is set", protocol.key()));
        let delivery = match self.streams.clone().ok_or_else(none)?.oneshot(call).await {
            Ok(delivery) => delivery,
            Err(never) => match never {},
        };
        delivery.result.map_err(|error| self.failed(&error))
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
        self.after(&exchange, || self.runtime.response(&exchange)).await?;
        Ok(exchange)
    }

    /// Runs the `afterResponse` hooks on `exchange`, whose `response` root `response` gives.
    ///
    /// # Errors
    ///
    /// The first hook that failed.
    pub async fn after(&self, exchange: &Exchange, response: impl FnOnce() -> Value + Send) -> Result<(), Failure> {
        if self.after.is_empty() {
            return Ok(());
        }
        let input = json!({ "request": request::sent(&exchange.request), "response": response() });
        for hook in &self.after {
            self.bridge.call(hook, input.clone()).await.map_err(|error| hooks::failure(hook, &error))?;
        }
        Ok(())
    }

    /// Sends `call` until `poll.until` holds, waiting `poll.every` between sends; the first
    /// exchange that holds is the call's.
    ///
    /// # Errors
    ///
    /// As [`Self::send`], or V5104 with the last exchange when `poll.until` does not hold
    /// within `poll.timeout`, so the events show what the server last answered.
    pub async fn poll(&self, render: &mut Render<'_>, (call, poll): (Call, &Poll)) -> Result<Exchange, Halted> {
        let deadline = Instant::now() + poll.timeout.get();
        loop {
            let exchange = self.send(call.clone()).await?;
            let response = self.bind(render, &exchange);
            let results = self.judge(render, (poll.until.checks.as_slice(), subject(&response, &exchange))).await;
            let Some(failure) = check::verdict(&results) else { return Ok(exchange) };
            if Instant::now() + poll.every.get() > deadline {
                let message = format!("`poll.until` did not hold within {:?}: {}", poll.timeout.get(), failure.message);
                let failure = Failure::new(message).with_code(Code::V5104.to_string());
                return Err(Halted { failure, exchanges: vec![exchange] });
            }
            time::sleep(poll.every.get()).await;
        }
    }

    /// Binds `response` and `request` for `exchange`, read by the api type's runtime.
    pub fn bind(&self, render: &mut Render<'_>, exchange: &Exchange) -> Arc<Value> {
        bind(render, exchange, &*self.runtime)
    }

    /// Binds `answer`'s `request` and `response` roots, its `exchange` items included.
    pub fn answered(&self, render: &mut Render<'_>, answer: &Answer) -> Arc<Value> {
        bound(render, &answer.exchange, answer.response(&*self.runtime))
    }

    /// Evaluates `checks` against `subject`, running the custom ones with `{response, vars}`,
    /// masked by the run's redactor so neither the results nor a failure built from them shows
    /// a masked header.
    /// The render is read before the hooks run, so the future does not hold it.
    pub fn judge<'f>(
        &'f self,
        render: &Render<'f>,
        (checks, subject): (&'f [Check], Subject<'_>),
    ) -> impl Future<Output = Vec<CheckResult>> + Send + use<'f> {
        let mut results = check::evaluate(&Judge::new(&self.matchers, render), checks, subject);
        let custom = checks.iter().any(|check| check.kind == CheckKind::Custom);
        let input = custom.then(|| json!({ "response": subject.response, "vars": render.hook_vars() }));
        let (bridge, redactor) = (&self.bridge, render.redactor());
        async move {
            if let Some(input) = input {
                hooks::judge(bridge, checks, (results.as_mut_slice(), &input)).await;
            }
            results.into_iter().map(|result| redactor.check(result)).collect()
        }
    }

    /// A transport error as a failure; a refused attempt is the failure of the hook that
    /// refused it.
    fn failed(&self, error: &TransportError) -> Failure {
        let refusal = || self.refused.borrow().clone();
        let refused = if matches!(error, TransportError::InvalidRequest { .. }) { refusal() } else { None };
        refused.unwrap_or_else(|| Failure::new(error.to_string()).with_code(error.code().to_string()))
    }
}
