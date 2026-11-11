//! A Visitor over everything one instance runs: the walker visits each template reference and
//! action once, following the calls the instance makes and the tests it reuses with `test:`.

#[cfg(test)]
mod tests;
mod values;

use std::collections::HashSet;

use vc_model::{
    Action, ActionId, ActionKind, ApiId, AuthRef, FileId, Program, Reference, Test, TestBody, TestId, VarDefs,
};

/// What a walk reports. Every method defaults to doing nothing.
pub trait Visit {
    /// A `${…}` reference in a template, a CEL expression or a matcher.
    fn reference(&mut self, _reference: &Reference) {}
    /// An action the instance may run, the first time it is reached.
    fn action(&mut self, _id: ActionId, _action: &Action) {}
    /// The api file of an api the instance calls.
    fn group(&mut self, _group: FileId) {}
}

/// Walks a program's definitions and reports them to its visitor, each action, api and reused
/// test at most once.
pub struct Walker<'p, V> {
    program: &'p Program,
    visitor: V,
    actions: HashSet<ActionId>,
    apis: HashSet<ApiId>,
    tests: HashSet<TestId>,
}

impl<'p, V: Visit> Walker<'p, V> {
    pub fn new(program: &'p Program, visitor: V) -> Self {
        let (actions, apis, tests) = Default::default();
        Self { program, visitor, actions, apis, tests }
    }

    pub fn into_visitor(self) -> V {
        self.visitor
    }

    /// The run-wide templates every instance can read: config vars and headers, and profiles.
    pub fn run_scope(&mut self) {
        let config = &self.program.config;
        self.vars(&config.vars);
        self.templates(config.defaults.headers.values());
        for profile in self.program.profiles.values() {
            self.values(profile.vars.values());
        }
    }

    /// The settings of every alert channel, which a run's end renders.
    pub fn alerts(&mut self) {
        let program = self.program;
        for channel in program.config.alerts.iter().flat_map(|alert| &alert.notify) {
            self.value(&channel.settings);
        }
    }

    /// Everything an instance of `test` runs: its file's vars and `beforeEach`/`afterEach`, then
    /// the test itself.
    pub fn instance(&mut self, test: &Test) {
        if let Some(file) = self.program.test_files.get(&test.file) {
            self.vars(&file.vars);
            self.actions(&file.lifecycle.before_each);
            self.actions(&file.lifecycle.after_each);
        }
        self.test(test);
    }

    fn test(&mut self, test: &Test) {
        self.vars(&test.vars);
        self.expect(&test.expect);
        if let Some(examples) = &test.examples {
            self.examples(examples);
        }
        self.actions(&test.before);
        self.actions(&body_actions(&test.body));
        self.actions(&test.after);
    }

    pub fn actions(&mut self, ids: &[ActionId]) {
        for id in ids {
            self.action(*id);
        }
    }

    pub fn action(&mut self, id: ActionId) {
        if !self.actions.insert(id) {
            return;
        }
        let Some(action) = usize::try_from(id.index()).ok().and_then(|at| self.program.actions.get(at)) else {
            return;
        };
        self.visitor.action(id, action);
        self.action_parts(action);
        self.kind(&action.kind);
    }

    fn action_parts(&mut self, action: &Action) {
        self.vars(&action.vars);
        if let Some(auth) = &action.auth {
            self.auth(auth);
        }
        self.templates(action.headers.values().chain(action.query.values()));
        if let Some(body) = &action.body {
            self.body(body);
        }
        self.expect(&action.expect);
        self.values(action.save.values.values());
        self.action_extras(action);
    }

    fn action_extras(&mut self, action: &Action) {
        if let Some(poll) = &action.poll {
            self.expect(&poll.until);
        }
        if let Some(vc_model::Loop::ForEach { items, .. }) = &action.repeat {
            self.value(items);
        }
        if let Some(check) = action.cache.as_ref().and_then(|cache| cache.check) {
            self.action(check);
        }
    }

    fn kind(&mut self, kind: &ActionKind) {
        match kind {
            ActionKind::Call(api) => self.api(*api),
            ActionKind::Test(test) => self.reused(*test),
            ActionKind::Command(template) => self.template(template),
            ActionKind::Inline(inline) => {
                self.template(&inline.url);
                self.templates(inline.query.values().chain(inline.headers.values()));
                self.body(&inline.body);
            }
            ActionKind::Function(_) => {}
        }
    }

    /// A test reused with `test:` runs only its call or steps, with its vars in scope.
    fn reused(&mut self, id: TestId) {
        if !self.tests.insert(id) {
            return;
        }
        let Some(test) = self.program.tests.get(&id) else { return };
        self.vars(&test.vars);
        self.actions(&body_actions(&test.body));
    }

    fn api(&mut self, id: ApiId) {
        if !self.apis.insert(id) {
            return;
        }
        let Some(api) = self.program.apis.get(&id) else { return };
        self.visitor.group(api.group);
        self.template(&api.path);
        self.vars(&api.vars);
        self.templates(api.query.values().chain(api.headers.values()));
        self.body(&api.body);
        self.auth(&api.auth);
        self.expect(&api.expect);
        self.values(api.save.values.values());
        if let Some(cleanup) = api.cleanup {
            self.action(cleanup);
        }
        self.group_parts(api.group);
    }

    fn group_parts(&mut self, id: FileId) {
        let Some(group) = self.program.api_groups.get(&id) else { return };
        if let Some(base_url) = &group.base_url {
            self.template(base_url);
        }
        self.templates(group.headers.values());
        self.vars(&group.vars);
        self.auth(&group.auth);
    }

    fn auth(&mut self, auth: &AuthRef) {
        let AuthRef::Use { id, vars } = auth else { return };
        self.vars(vars);
        if let Some(def) = self.program.auth.get(id) {
            self.values(def.settings.values());
            self.vars(&def.vars);
            self.templates(def.expires_in.iter());
            self.actions(&def.steps);
        }
    }

    fn vars(&mut self, defs: &VarDefs) {
        self.values(defs.values.values());
        for values in defs.by_profile.values() {
            self.values(values.values());
        }
    }
}

/// The actions a test's body runs: its call, or its steps in order.
pub fn body_actions(body: &TestBody) -> Vec<ActionId> {
    match body {
        TestBody::Call(action) => vec![*action],
        TestBody::Steps(steps) => steps.clone(),
    }
}
