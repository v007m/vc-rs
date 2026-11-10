//! The agent policy's gate on what an agent sends (D178, D195): the methods a call or a run
//! would send, its cleanups and the steps of the `flow` auths it uses included, and the
//! `command:` and `function:` actions it would run (D202), judged and, where the policy asks,
//! approved before anything goes out; and the log entry once it is done, or once it fails
//! before the gate (D204).

use std::collections::BTreeSet;

use serde_json::json;
use vc_executor::auth_steps;
use vc_model::{Action, ActionId, ActionKind, ApiId, Method, Plan, Program, TestBody, TestId};

use crate::approvals::{APPROVAL_WAIT, Admission, AgentOutcome};
use crate::plan::Prepared;
use crate::policy::AgentCall;
use crate::shown::Shown;
use crate::{Engine, EngineError, RunRequest, history};

/// The tool an agent's run is judged and logged as.
pub const RUN_TOOL: &str = "run_tests";

/// What a person can do about a refused agent call.
pub const REFUSED_HELP: &str = "a person can widen the profile's `agents` block in vibranium.config.yaml, or approve the call with `vc approve <id>`";

/// An admitted agent call, and who approved it.
pub type Gated = (AgentCall, Option<String>);

impl Engine {
    /// `call` once the policy lets it through, waiting for a person when it asks.
    ///
    /// # Errors
    ///
    /// [`EngineError::Refused`] when the policy refuses it or no one approves it in time; as
    /// [`Engine::admit`] otherwise.
    pub(crate) async fn gated(&self, call: AgentCall) -> Result<Gated, EngineError> {
        match self.admit(&call, APPROVAL_WAIT).await? {
            Admission::Allowed { approved_by } => Ok((call, approved_by)),
            Admission::Refused { reason } => Err(EngineError::Refused { reason, help: Some(REFUSED_HELP.to_owned()) }),
        }
    }

    /// For an agent's run, the gate on `run`, every method `prepared`'s plan sends and the code
    /// it runs, one approval for the whole run; `None` for a person's run, which is not gated.
    ///
    /// # Errors
    ///
    /// As [`Engine::gated`].
    pub(crate) async fn run_gate(
        &self,
        request: &RunRequest,
        prepared: &Prepared,
    ) -> Result<Option<Gated>, EngineError> {
        let Some(call) = run_call(request) else { return Ok(None) };
        let count = prepared.plan.instance_count();
        let tests = if count == 1 { "test" } else { "tests" };
        let selector = history::selector(&request.plan.select);
        let reached = plan_reach(&prepared.program, &prepared.plan);
        let summary = format!("run {count} {tests} {selector}").trim_end().to_owned();
        let call = AgentCall { methods: reached.methods, code: reached.code, runs: true, summary, ..call };
        self.gated(call).await.map(Some)
    }

    /// `before`, the work an agent's `call` does before the gate; when it fails, the call is
    /// logged as `failed: <reason>` (D204). A person's call (`None`) is not logged.
    ///
    /// # Errors
    ///
    /// `before`'s error; as [`Engine::record_agent_action`] when it cannot be logged.
    pub(crate) async fn before_gate<T>(
        &self,
        call: Option<&AgentCall>,
        before: Result<T, EngineError>,
    ) -> Result<T, EngineError> {
        let failed = call.zip(before.as_ref().err().map(|error| format!("failed: {error}")));
        if let Some((call, result)) = failed {
            self.record_agent_action(call, &AgentOutcome { result, approved_by: None }).await?;
        }
        before
    }

    /// Logs a gated call as `ok` when it `passed`, else `failed`; a person's call (`None`) is
    /// not logged.
    ///
    /// # Errors
    ///
    /// As [`Engine::record_agent_action`].
    pub(crate) async fn settle(&self, gated: Option<Gated>, passed: bool) -> Result<(), EngineError> {
        let Some((call, approved_by)) = gated else { return Ok(()) };
        let result = if passed { "ok" } else { "failed" }.to_owned();
        self.record_agent_action(&call, &AgentOutcome { result, approved_by }).await
    }
}

/// An agent's run as the log names it before its plan is known; `None` for a person's run.
pub fn run_call(request: &RunRequest) -> Option<AgentCall> {
    let agent = request.agent.as_ref()?;
    let selector = history::selector(&request.plan.select);
    Some(AgentCall {
        agent: agent.clone(),
        tool: RUN_TOOL.to_owned(),
        profiles: request.plan.profiles.clone(),
        summary: format!("run {selector}").trim_end().to_owned(),
        arguments: json!({"select": selector, "profiles": request.plan.profiles}),
        ..AgentCall::default()
    })
}

/// What a call or a run reaches: the methods it sends and the code it runs, in order, each once.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Reached {
    pub methods: Vec<Method>,
    /// Each `command:` and `function:` action, named by the test or block it sits in and what
    /// it runs.
    pub code: Vec<String>,
}

/// What the call `action` reaches: its api's method, its api's cleanup unless `keep`, and
/// what the steps of the `flow` auth it uses send and run.
pub fn call_reach(program: &Program, action: ActionId, keep: bool) -> Reached {
    let found = usize::try_from(action.index()).ok().and_then(|index| program.actions.get(index));
    let Some(ActionKind::Call(api)) = found.map(|found| &found.kind) else { return Reached::default() };
    let mut reach = Reach::new(program, program.str(api.sym()).unwrap_or_default());
    if keep {
        reach.seen.insert(Seen::Api(*api));
        reach.method(program.apis.get(api).map_or(Method::Get, |api| api.method));
    }
    reach.action(action);
    reach.reached
}

/// What every action `plan` runs reaches, in plan order.
pub fn plan_reach(program: &Program, plan: &Plan) -> Reached {
    let mut reach = Reach::new(program, "beforeRun");
    reach.actions(plan.before_run.iter().map(|planned| planned.action));
    for file in &plan.files {
        let name = program.str(file.id.sym()).unwrap_or_default();
        reach.at(format!("{name} beforeAll"), file.before_all.iter().map(|planned| planned.action));
        let each = program.test_files.get(&file.id).map(|found| &found.lifecycle);
        reach.at(format!("{name} beforeEach"), each.iter().flat_map(|each| each.before_each.iter().copied()));
        for instance in &file.instances {
            reach.test(instance.test);
        }
        reach.at(format!("{name} afterEach"), each.iter().flat_map(|each| each.after_each.iter().copied()));
        reach.at(format!("{name} afterAll"), file.after_all.iter().copied());
    }
    reach.at("afterRun".to_owned(), plan.after_run.iter().copied());
    reach.reached
}

/// A walk over the actions a run reaches, collecting the methods they send and the code they
/// run.
struct Reach<'p> {
    program: &'p Program,
    reached: Reached,
    /// The apis, tests and flow steps walked so far, so a cleanup, `test:` action or flow that
    /// loops back stops.
    seen: BTreeSet<Seen>,
    /// Where the walk is, a lifecycle block or a test's ID, to name the code it finds.
    place: String,
}

/// An api or test the walk has reached.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Seen {
    Api(ApiId),
    Test(TestId),
    Step(ActionId),
}

impl<'p> Reach<'p> {
    fn new(program: &'p Program, place: &str) -> Self {
        Self { program, reached: Reached::default(), seen: BTreeSet::new(), place: place.to_owned() }
    }

    fn at(&mut self, place: String, ids: impl Iterator<Item = ActionId>) {
        self.place = place;
        self.actions(ids);
    }

    fn actions(&mut self, ids: impl Iterator<Item = ActionId>) {
        for id in ids {
            self.action(id);
        }
    }

    fn action(&mut self, id: ActionId) {
        let program = self.program;
        let Some(action) = usize::try_from(id.index()).ok().and_then(|index| program.actions.get(index)) else {
            return;
        };
        match &action.kind {
            ActionKind::Call(api) => self.call(*api, action),
            ActionKind::Test(test) => self.test(*test),
            ActionKind::Command(command) => {
                let text = Shown { program, vars: &[] }.template(command);
                self.code(&format!("command `{text}`"));
            }
            ActionKind::Function(hook) => self.code(&format!("function `{}#{}`", hook.file, hook.export)),
            ActionKind::Inline(inline) => self.method(inline.method),
        }
    }

    /// The api `action` calls, then the steps of the `flow` auth it uses, each step once.
    fn call(&mut self, id: ApiId, action: &Action) {
        self.api(id);
        let Some(api) = self.program.apis.get(&id) else { return };
        let steps = auth_steps(self.program, (api, action));
        let fresh: Vec<ActionId> = steps.into_iter().filter(|step| self.seen.insert(Seen::Step(*step))).collect();
        self.actions(fresh.into_iter());
    }

    fn method(&mut self, method: Method) {
        if !self.reached.methods.contains(&method) {
            self.reached.methods.push(method);
        }
    }

    fn code(&mut self, what: &str) {
        let named = format!("{}: {what}", self.place);
        if !self.reached.code.contains(&named) {
            self.reached.code.push(named);
        }
    }

    fn api(&mut self, id: ApiId) {
        let Some(api) = self.program.apis.get(&id) else { return };
        if !self.seen.insert(Seen::Api(id)) {
            return;
        }
        self.method(api.method);
        if let Some(cleanup) = api.cleanup {
            self.action(cleanup);
        }
    }

    fn test(&mut self, id: TestId) {
        let Some(test) = self.program.tests.get(&id) else { return };
        if !self.seen.insert(Seen::Test(id)) {
            return;
        }
        let outer = std::mem::replace(&mut self.place, self.program.str(id.sym()).unwrap_or_default().to_owned());
        self.actions(test.before.iter().copied());
        match &test.body {
            TestBody::Call(action) => self.action(*action),
            TestBody::Steps(steps) => self.actions(steps.iter().copied()),
        }
        self.actions(test.after.iter().copied());
        self.place = outer;
    }
}

#[cfg(test)]
#[path = "agent_gate_tests.rs"]
mod tests;
