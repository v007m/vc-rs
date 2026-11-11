//! A test as `vc explain` shows it: the lifecycle actions a run of it triggers, around each
//! request its body makes with the vars in force, values that earlier calls save among them.

use std::fmt::Write as _;

use vc_model::{Action, ActionId, ActionKind, Api, Lifecycle, Test, TestBody};

use super::Explainer;
use super::call::Called;
use crate::shown::{self, Var};

/// Lifecycle blocks by the name they are written under, in the order a run runs them.
type Blocks<'t> = [(&'static str, &'t [ActionId]); 4];

/// A test being walked, with what its actions so far save: vars that only a run fills in.
struct Walk<'t> {
    test: &'t Test,
    saved: Vec<Var>,
}

impl Explainer<'_> {
    /// The test's name and profiles, the blocks run before it, each request its body makes and
    /// the blocks run after it.
    pub(super) fn test(&self, test: &Test) -> String {
        let mut out = self.header(&self.text(test.id.sym()), test.name.as_deref());
        let (before, after) = self.blocks(test);
        out.push_str(&self.sections(&before));
        let mut walk = Walk { test, saved: Vec::new() };
        let [.., (_, each), (_, own)] = before;
        for id in each.iter().chain(own) {
            self.save(&mut walk, *id);
        }
        let body = match &test.body {
            TestBody::Call(action) => std::slice::from_ref(action),
            TestBody::Steps(actions) => actions.as_slice(),
        };
        for id in body {
            out.push_str(&self.step(&mut walk, *id));
        }
        out.push_str(&self.sections(&after));
        out
    }

    /// The run's, the file's and the test's blocks before the test, then those after it.
    fn blocks<'t>(&'t self, test: &'t Test) -> (Blocks<'t>, Blocks<'t>) {
        let run = &self.program.run_blocks;
        let file = self.program.test_files.get(&test.file).map(|file| &file.lifecycle);
        let of = |block: fn(&Lifecycle) -> &[ActionId]| file.map_or(&[][..], block);
        let before = [
            ("beforeRun", run.before_run.as_slice()),
            ("beforeAll", of(|file| file.before_all.as_slice())),
            ("beforeEach", of(|file| file.before_each.as_slice())),
            ("before", test.before.as_slice()),
        ];
        let after = [
            ("after", test.after.as_slice()),
            ("afterEach", of(|file| file.after_each.as_slice())),
            ("afterAll", of(|file| file.after_all.as_slice())),
            ("afterRun", run.after_run.as_slice()),
        ];
        (before, after)
    }

    /// `before  call users#create (cleanup users#remove)`, for each block that has actions.
    fn sections(&self, blocks: &Blocks<'_>) -> String {
        let mut out = String::new();
        for (name, actions) in blocks.iter().filter(|(_, actions)| !actions.is_empty()) {
            let labels: Vec<String> = actions.iter().map(|id| self.block_label(*id)).collect();
            let _ = writeln!(out, "{name}  {}", labels.join(", "));
        }
        out
    }

    /// An action in a block, with the cleanup its call registers.
    fn block_label(&self, id: ActionId) -> String {
        let label = vc_planner::action_label(self.program, id);
        let api = self.action(id).and_then(|action| match &action.kind {
            ActionKind::Call(api) => self.program.apis.get(api),
            _ => None,
        });
        match api.and_then(|api| api.cleanup) {
            Some(cleanup) => format!("{label} (cleanup {})", self.cleanup(cleanup)),
            None => label,
        }
    }

    /// One step: its label, and for a call the request with the vars in force at it.
    fn step(&self, walk: &mut Walk<'_>, id: ActionId) -> String {
        let mut out = format!("{}\n", vc_planner::action_label(self.program, id));
        let Some(action) = self.action(id) else { return out };
        if let ActionKind::Call(api) = &action.kind
            && let Some(api) = self.program.apis.get(api)
        {
            let vars = self.step_vars(walk, (api, action));
            out.push_str(&self.request(&Called { api, action: Some(action) }, &vars));
        }
        self.save(walk, id);
        out
    }

    /// The vars at a call, furthest first: the config's and the api's, the test file's, those
    /// saved earlier in the test, the test's and the call's own.
    fn step_vars(&self, walk: &Walk<'_>, (api, action): (&Api, &Action)) -> Vec<Var> {
        let mut vars = self.config_vars();
        self.api_vars(api, &mut vars);
        if let Some(file) = self.program.test_files.get(&walk.test.file) {
            self.level(&mut vars, &file.vars, &format!("file {}", self.text(file.id.sym())));
        }
        for var in &walk.saved {
            shown::set(&mut vars, var.clone());
        }
        self.level(&mut vars, &walk.test.vars, "test");
        self.level(&mut vars, &action.vars, "call");
        vars
    }

    /// Notes what the action saves, its api's `save` then its own, as vars set from then on.
    fn save(&self, walk: &mut Walk<'_>, id: ActionId) {
        let Some(action) = self.action(id) else { return };
        let api = match &action.kind {
            ActionKind::Call(api) => self.program.apis.get(api).map(|api| &api.save),
            _ => None,
        };
        let origin = format!("saved by {}", vc_planner::action_label(self.program, id));
        for (name, value) in api.into_iter().chain([&action.save]).flat_map(|save| &save.values) {
            let var = Var { name: self.text(name.sym()), value: value.clone(), origin: origin.clone(), saved: true };
            shown::set(&mut walk.saved, var);
        }
    }
}
