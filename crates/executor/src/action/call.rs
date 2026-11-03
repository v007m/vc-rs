use std::time::Duration;

use async_trait::async_trait;
use http::Uri;
use tower::ServiceExt;
use vc_diagnostics::Code;
use vc_interfaces::SendContext;
use vc_model::{Action, ActionId, ActionKind, Api, Failure, Method, Part, Program, RenderedRequest, Template};

use super::{ActionExecutor, ActionRequest};
use crate::{ActionReport, Call, PendingCleanup, PipelineParts, pipeline};

/// The per-attempt limit when the api sets no `timeout`.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// Runs `call:` actions: renders the api's request, sends it through the request pipeline and
/// registers the api's `cleanup` on a 2xx response unless the action is `keep`.
///
/// Until runtime values land, only templates without references render, and the status check
/// is the default `2xx`.
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
        let program = &request.cx.program;
        let action = request.cx.action(request.action).ok_or_else(|| Failure::new("the action is not in the program"))?;
        let api = called(program, action).ok_or_else(|| Failure::new("the action calls no api in the program"))?;
        let timeout = api.timeout.map_or(DEFAULT_TIMEOUT, vc_model::Duration::get);
        let call = Call { request: render(program, api)?, timeout, ctx: SendContext::default() };
        let delivery = match pipeline(self.parts.clone()).oneshot(call).await {
            Ok(delivery) => delivery,
            Err(never) => match never {},
        };
        let exchange = delivery.result.map_err(|error| Failure::new(error.to_string()).with_code(error.code().to_string()))?;
        let status = exchange.response.status;
        if !status.is_success() {
            let failure = Failure::new(format!("expected a 2xx status, got {status}"));
            return Err(failure.with_code(Code::V5101.to_string()));
        }
        let cleanups = api.cleanup.filter(|_| !action.keep).map(|action| PendingCleanup { action });
        Ok(ActionReport { cleanups: cleanups.into_iter().collect(), ..ActionReport::passed() })
    }
}

#[async_trait]
impl ActionExecutor for CallExecutor {
    async fn execute(&self, request: ActionRequest) -> ActionReport {
        match self.call(&request).await {
            Ok(report) => report,
            Err(failure) => ActionReport::failed(failure.at(request.action)),
        }
    }

    fn follows(&self, program: &Program, action: &Action) -> Vec<ActionId> {
        called(program, action).and_then(|api| api.cleanup).into_iter().collect()
    }
}

fn called<'p>(program: &'p Program, action: &Action) -> Option<&'p Api> {
    let ActionKind::Call(api) = &action.kind else { return None };
    program.apis.get(api)
}

/// The request line: the api file's `baseUrl` followed by the api's `path`.
fn render(program: &Program, api: &Api) -> Result<RenderedRequest, Failure> {
    let base = program.api_groups.get(&api.group).and_then(|group| group.base_url.as_ref());
    let url = base.into_iter().chain([&api.path]).map(literal).collect::<Result<String, _>>()?;
    let uri = url.parse::<Uri>().map_err(|error| rendering(&format!("`{url}` is not a URL: {error}")))?;
    Ok(RenderedRequest::new(method(api.method), uri))
}

fn literal(template: &Template) -> Result<String, Failure> {
    template.parts.iter().map(text).collect()
}

fn text(part: &Part) -> Result<&str, Failure> {
    match part {
        Part::Literal(text) => Ok(text.as_str()),
        Part::Ref(_) => Err(rendering("a template with references renders once runtime values land")),
    }
}

fn rendering(message: &str) -> Failure {
    Failure::new(message).with_code(Code::V5111.to_string())
}

const fn method(method: Method) -> http::Method {
    match method {
        Method::Get => http::Method::GET,
        Method::Post => http::Method::POST,
        Method::Put => http::Method::PUT,
        Method::Patch => http::Method::PATCH,
        Method::Delete => http::Method::DELETE,
        Method::Head => http::Method::HEAD,
        Method::Options => http::Method::OPTIONS,
    }
}
