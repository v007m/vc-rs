//! Walks every test and block in run order with the levels each spot sees, so [`Checker`] can
//! check vars per profile (V2101, V2304) and step ids against what ran before (V2105).

use vc_diagnostics::Diagnostics;
use vc_model::{FileKind, SourceSpan};
use vc_parser::Node;

use super::layer::{Layer, Scope};
use super::project;
use super::project::{FILE_BLOCKS, Item, Project, RUN_BLOCKS, Source, TEST_BLOCKS};
use super::saves;
use super::saves::Saves;
use super::uses::{Checker, Spot};

/// Checks what every spot reads.
pub fn check(project: &Project<'_>) -> Diagnostics {
    let config = project.config();
    let profiles = config.map(|config| project::list(config.root, "profiles")).unwrap_or_default();
    let profiles = profiles.iter().filter_map(|profile| project::text(profile, "id")).map(str::to_owned).collect();
    let mut flow = Flow { checker: Checker::new(project, profiles), saves: Saves::new(project) };
    let run = config.map_or_else(Scope::default, |config| flow.config(config));
    for source in project.sources.iter().filter(|source| source.kind == FileKind::Test) {
        flow.test_file(source, &run);
    }
    flow.checker.diagnostics
}

/// The parts of an action checked on their own: its blocks, and its `cache`, whose `check`
/// runs after the action and sees what it saved (under its `id` too).
const ACTION_PARTS: [&str; 4] = ["before", "steps", "after", "cache"];

/// The parts of a `flow` auth entry that run after its steps and see their saves.
const AFTER_STEPS: [&str; 2] = ["apply", "expiresIn"];

/// Whether a block's saves are shared under the action's `id` or kept as plain names.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Saved {
    Shared,
    Plain,
}

struct Flow<'p, 'a> {
    checker: Checker<'p, 'a>,
    saves: Saves<'p, 'a>,
}

impl<'p, 'a> Flow<'p, 'a> {
    /// The config's own values, then `beforeRun` and `afterRun`; gives what every test sees.
    fn config(&mut self, source: &'p Source<'a>) -> Scope<'a> {
        let root = source.root;
        let base = Scope::default().with(Layer::vars(source, root));
        let mut keys = vec!["auth"];
        keys.extend(RUN_BLOCKS);
        self.spot(source, (root, &project::holes(root, &keys)), &base);
        self.auth(source, &base);
        let mut before = base.clone();
        let shared = self.block(source, project::list(root, "beforeRun"), (&mut before, Saved::Shared));
        let run = base.with(shared);
        self.block(source, project::list(root, "afterRun"), (&mut run.clone(), Saved::Plain));
        run
    }

    /// Each `auth` entry with its own vars; its `steps` run in order and `apply` and
    /// `expiresIn` see their saves.
    fn auth(&mut self, source: &'p Source<'a>, base: &Scope<'a>) {
        for entry in project::list(source.root, "auth") {
            let mut scope = base.with(Layer::vars(source, entry));
            let mut keys = vec!["steps"];
            keys.extend(AFTER_STEPS);
            self.spot(source, (entry, &project::holes(entry, &keys)), &scope);
            self.block(source, project::list(entry, "steps"), (&mut scope, Saved::Plain));
            for part in AFTER_STEPS.iter().filter_map(|key| entry.get(key)) {
                self.spot(source, (part, &[]), &scope);
            }
        }
    }

    /// A test file: its values, its lifecycle blocks and its tests.
    fn test_file(&mut self, source: &'p Source<'a>, run: &Scope<'a>) {
        let root = source.root;
        let mut file = run.with(Layer::vars(source, root));
        let mut keys = vec!["tests"];
        keys.extend(FILE_BLOCKS);
        self.spot(source, (root, &project::holes(root, &keys)), &file);
        let shared = self.block(source, project::list(root, "beforeAll"), (&mut file.clone(), Saved::Shared));
        file.push(shared);
        let each = self.block(source, project::list(root, "beforeEach"), (&mut file.clone(), Saved::Plain));
        let per_test = file.with(each);
        for test in project::list(root, "tests") {
            self.test(source, test, &per_test);
        }
        self.block(source, project::list(root, "afterEach"), (&mut per_test.clone(), Saved::Plain));
        self.block(source, project::list(root, "afterAll"), (&mut file, Saved::Plain));
    }

    /// One test: `before`, its call or steps, then `after`.
    fn test(&mut self, source: &'p Source<'a>, test: &'a Node, scope: &Scope<'a>) {
        let mut run = scope.with(Layer::vars(source, test));
        self.block(source, project::list(test, "before"), (&mut run, Saved::Plain));
        if test.get("call").is_some() {
            let saved = self.action(source, test, &run);
            run.push(Layer::names(saved));
        } else {
            self.block(source, project::list(test, "steps"), (&mut run, Saved::Plain));
            self.spot(source, (test, &project::holes(test, &TEST_BLOCKS)), &run);
            run.push(Layer::names(saves::keys(test.get("save"))));
        }
        self.block(source, project::list(test, "after"), (&mut run, Saved::Plain));
    }

    /// Runs `actions` in order in `scope`, which gains each one's saves and id; gives the
    /// level the block adds for what runs after it.
    fn block(&mut self, source: &'p Source<'a>, actions: &'a [Node], (scope, saved): (&mut Scope<'a>, Saved)) -> Layer {
        let mut added = Layer::default();
        for action in actions {
            let names = self.action(source, action, scope);
            let id = project::text(action, "id");
            let names = if saved == Saved::Shared { id.map(str::to_owned).into_iter().collect() } else { names };
            let layer = Layer::names(names);
            added.absorb(&layer);
            scope.push(layer);
            scope.ran_now(id);
        }
        added
    }

    /// One action, or a test with `call`: its own values, then the api it calls; gives its saves.
    fn action(&mut self, source: &'p Source<'a>, node: &'a Node, scope: &Scope<'a>) -> Vec<String> {
        let own = scope.with(Layer::vars(source, node));
        let api = saves::called(self.checker.project, (source, node));
        let inner = api.map_or_else(|| own.clone(), |api| own.with(self.api_layer(api)));
        self.spot(source, (node, &project::holes(node, &ACTION_PARTS)), &inner);
        if let Some(api) = api {
            self.api(api, &inner, call_site(node));
        }
        let saved = self.saves.action(source, node);
        if let Some(check) = node.get("cache").and_then(|cache| cache.get("check")) {
            let mut cached = saved.clone();
            cached.extend(project::text(node, "id").map(str::to_owned));
            self.action(source, check, &own.with(Layer::names(cached)));
        }
        saved
    }

    /// The api's templates, from one call site; then its `cleanup`, which also sees its saves.
    fn api(&mut self, api: Item<'a>, scope: &Scope<'a>, site: SourceSpan) {
        let project = self.checker.project;
        let source = project.source(api);
        let mut inner = scope.clone();
        inner.steps.clear();
        self.api_templates(api, &inner, site);
        let Some(cleanup) = api.node.get("cleanup") else { return };
        let after = inner.with(Layer::names(saves::keys(api.node.get("save")))).with(Layer::vars(source, cleanup));
        self.checker.check(&Spot { source, span: cleanup.span, holes: &[], site: Some(site) }, &after);
        if let Some(target) = saves::called(project, (source, cleanup)) {
            self.api_templates(target, &after.with(self.api_layer(target)), call_site(cleanup));
        }
    }

    /// An api entry, without its cleanup, and the values of its file.
    fn api_templates(&mut self, api: Item<'a>, scope: &Scope<'a>, site: SourceSpan) {
        let source = self.checker.project.source(api);
        let entry = project::holes(api.node, &["cleanup"]);
        self.checker.check(&Spot { source, span: api.node.span, holes: &entry, site: Some(site) }, scope);
        let file = project::holes(source.root, &["apis"]);
        self.checker.check(&Spot { source, span: source.root.span, holes: &file, site: Some(site) }, scope);
    }

    /// The vars an api and its file set.
    fn api_layer(&self, api: Item<'a>) -> Layer {
        let source = self.checker.project.source(api);
        let mut layer = Layer::vars(source, source.root);
        layer.absorb(&Layer::vars(source, api.node));
        layer
    }

    fn spot(&mut self, source: &Source<'a>, (node, holes): (&Node, &[SourceSpan]), scope: &Scope<'a>) {
        self.checker.check(&Spot { source, span: node.span, holes, site: None }, scope);
    }
}

/// Where a call is shown: its `call:` value.
fn call_site(node: &Node) -> SourceSpan {
    node.get("call").map_or(node.span, |call| call.span)
}
