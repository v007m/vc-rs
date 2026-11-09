//! The agent policy's gate on what an agent sends (D178, D195): the methods a call or a run
//! would send, its cleanups included, judged and, where the policy asks, approved before
//! anything goes out; and the log entry once it is done.

use std::collections::BTreeSet;

use serde_json::json;
use vc_model::{ActionId, ActionKind, ApiId, Method, Plan, Program, TestBody, TestId};

use crate::approvals::{APPROVAL_WAIT, Admission, AgentOutcome};
use crate::plan::Prepared;
use crate::policy::AgentCall;
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

    /// For an agent's run, the gate on `run` and every method `prepared`'s plan sends, one
    /// approval for the whole run; `None` for a person's run, which is not gated.
    ///
    /// # Errors
    ///
    /// As [`Engine::gated`].
    pub(crate) async fn run_gate(
        &self,
        request: &RunRequest,
        prepared: &Prepared,
    ) -> Result<Option<Gated>, EngineError> {
        let Some(agent) = &request.agent else { return Ok(None) };
        let count = prepared.plan.instance_count();
        let selector = history::selector(&request.plan.select);
        let tests = if count == 1 { "test" } else { "tests" };
        let call = AgentCall {
            agent: agent.clone(),
            tool: RUN_TOOL.to_owned(),
            profiles: request.plan.profiles.clone(),
            methods: plan_methods(&prepared.program, &prepared.plan),
            runs: true,
            summary: format!("run {count} {tests} {selector}").trim_end().to_owned(),
            arguments: json!({"select": selector, "profiles": request.plan.profiles}),
            ..AgentCall::default()
        };
        self.gated(call).await.map(Some)
    }

    /// Logs a gated call as `ok` when it `passed`, else `failed`; a person's call (`None`) is
    /// not logged.
    ///
    /// # Errors
    ///
    /// As [`Engine::record_agent_action`].
    pub(crate) fn settle(&self, gated: Option<Gated>, passed: bool) -> Result<(), EngineError> {
        let Some((call, approved_by)) = gated else { return Ok(()) };
        let result = if passed { "ok" } else { "failed" }.to_owned();
        self.record_agent_action(&call, &AgentOutcome { result, approved_by })
    }
}

/// The methods `api` sends, then those of its cleanup, in order, each once.
pub fn api_methods(program: &Program, api: ApiId) -> Vec<Method> {
    let mut reach = Reach { program, methods: Vec::new(), seen: BTreeSet::new() };
    reach.api(api);
    reach.methods
}

/// The methods every action `plan` runs would send, in plan order, each once.
pub fn plan_methods(program: &Program, plan: &Plan) -> Vec<Method> {
    let mut reach = Reach { program, methods: Vec::new(), seen: BTreeSet::new() };
    reach.actions(plan.before_run.iter().map(|planned| planned.action));
    for file in &plan.files {
        reach.actions(file.before_all.iter().map(|planned| planned.action));
        let each = program.test_files.get(&file.id).map(|found| &found.lifecycle);
        reach.actions(each.iter().flat_map(|each| each.before_each.iter().copied()));
        for instance in &file.instances {
            reach.test(instance.test);
        }
        reach.actions(each.iter().flat_map(|each| each.after_each.iter().copied()));
        reach.actions(file.after_all.iter().copied());
    }
    reach.actions(plan.after_run.iter().copied());
    reach.methods
}

/// A walk over the actions a run reaches, collecting the methods they send.
struct Reach<'p> {
    program: &'p Program,
    methods: Vec<Method>,
    /// The apis and tests walked so far, so a cleanup or `test:` action that loops back stops.
    seen: BTreeSet<Seen>,
}

/// An api or test the walk has reached.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Seen {
    Api(ApiId),
    Test(TestId),
}

impl Reach<'_> {
    fn actions(&mut self, ids: impl Iterator<Item = ActionId>) {
        for id in ids {
            self.action(id);
        }
    }

    fn action(&mut self, id: ActionId) {
        let found = usize::try_from(id.index()).ok().and_then(|index| self.program.actions.get(index));
        match found.map(|action| &action.kind) {
            Some(ActionKind::Call(api)) => self.api(*api),
            Some(ActionKind::Test(test)) => self.test(*test),
            _ => {}
        }
    }

    fn api(&mut self, id: ApiId) {
        let Some(api) = self.program.apis.get(&id) else { return };
        if !self.seen.insert(Seen::Api(id)) {
            return;
        }
        if !self.methods.contains(&api.method) {
            self.methods.push(api.method);
        }
        if let Some(cleanup) = api.cleanup {
            self.action(cleanup);
        }
    }

    fn test(&mut self, id: TestId) {
        let Some(test) = self.program.tests.get(&id) else { return };
        if !self.seen.insert(Seen::Test(id)) {
            return;
        }
        self.actions(test.before.iter().copied());
        match &test.body {
            TestBody::Call(action) => self.action(*action),
            TestBody::Steps(steps) => self.actions(steps.iter().copied()),
        }
        self.actions(test.after.iter().copied());
    }
}

#[cfg(test)]
#[path = "agent_gate_tests.rs"]
mod tests;
