//! A hand-built program, plan and server for the executor's tests.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

use async_trait::async_trait;
use indexmap::IndexMap;
use vc_interfaces::{SendContext, Transport, TransportError};
use vc_model::{
    Action, ActionId, ActionKind, Api, ApiGroup, ApiId, BodySpec, CelSource, ConnectionInfo, Exchange, FileId,
    Instance, InstanceId, Lifecycle, Matcher, Method, Needs, Part, Plan, PlannedAction, PlannedFile, Pos, Program,
    RenderedRequest, Retry, ScalarSource, ScalarStyle, SourceFileId, SourceFiles, SourceSpan, StatusPattern,
    StringTable, Sym, Template, TemplateValue, Test, TestBody, TestFile, TestId, Timing, VarName,
};
use vc_template::{CelCompiler, Roots};
use vc_testkit::{MemoryEnvironment, ScriptedRandom};

use crate::{ActionCx, ActionExecutors, BodyFiles, RunInputs};

/// Words every fixture interns up front, so templates parsed later share the program's symbols
/// for var names and step ids.
pub const WORDS: &[&str] = &[
    "created",
    "fetched",
    "user",
    "userId",
    "email",
    "name",
    "token",
    "region",
    "greeting",
    "dev",
    "prod",
    "page",
    "orders.flow",
    "code",
    "n",
    "path",
    "cities",
    "gone",
    "aaaId",
    "spareId",
    "svc",
    "login",
    "salt",
    "who",
    "nonce",
    "k",
    "tenant",
    "lastId",
];

/// How long `/slow` takes to answer.
pub const SLOW: Duration = Duration::from_secs(60);

/// A program with one api file (`users`, based at `http://api.test`) and one test file
/// (`users.crud`), grown by the builder methods.
pub struct Fixture {
    pub program: Program,
    pub file: FileId,
    group: FileId,
    names: Vec<Sym>,
    strings: StringTable,
    /// The `$file` bodies the run is given, read before it (D272).
    pub bodies: BodyFiles,
}

impl Fixture {
    pub fn new() -> Self {
        let mut strings = StringTable::new();
        let group = FileId::new(strings.intern("users").unwrap());
        let file = FileId::new(strings.intern("users.crud").unwrap());
        let names = (0..64).map(|n| strings.intern(&format!("users#n{n}")).unwrap()).rev().collect();
        let _: Vec<Sym> = WORDS.iter().map(|word| strings.intern(word).unwrap()).collect();
        let kept = strings.clone();
        let mut program = Program::new(strings, SourceFiles::new());
        let base_url = Some(literal("http://api.test"));
        let apis = Vec::new();
        let (auth, headers, vars, hooks) = Default::default();
        program.api_groups.insert(
            group,
            ApiGroup { id: group, name: None, base_url, auth, headers, vars, hooks, source: None, apis },
        );
        let (tags, vars, lifecycle, tests) = Default::default();
        program.test_files.insert(file, TestFile { id: file, name: None, tags, vars, skip: None, lifecycle, tests });
        Self { program, file, group, names, strings: kept, bodies: BodyFiles::default() }
    }

    /// A new action of `kind`.
    pub fn action(&mut self, kind: ActionKind) -> ActionId {
        let id = ActionId::new(u32::try_from(self.program.actions.len()).unwrap());
        self.program.actions.push(Action::new(kind, span()));
        id
    }

    /// A `call:` of a new `GET` api at `path`.
    pub fn call(&mut self, path: &str) -> ActionId {
        let api = self.api(path, None);
        self.action(ActionKind::Call(api))
    }

    /// A `call:` of a new api at `path` whose `cleanup` is `cleanup`.
    pub fn creating(&mut self, path: &str, cleanup: ActionId) -> ActionId {
        let api = self.api(path, Some(cleanup));
        self.action(ActionKind::Call(api))
    }

    fn api(&mut self, path: &str, cleanup: Option<ActionId>) -> ApiId {
        let id = ApiId::new(self.names.pop().unwrap());
        let (vars, query, headers, auth, expect, save, hooks) = Default::default();
        let api = Api {
            id,
            name: None,
            group: self.group,
            method: Method::Get,
            path: literal(path),
            vars,
            query,
            headers,
            body: BodySpec::None,
            auth,
            timeout: None,
            http_retry: None,
            expect,
            save,
            save_on: Matcher::Status(vec![StatusPattern::Class(2)]),
            cleanup,
            hooks,
            deprecated: false,
            operation_id: None,
            span: span(),
        };
        self.program.apis.insert(id, api);
        id
    }

    /// A new test in `users.crud` running `steps`.
    pub fn test(&mut self, steps: Vec<ActionId>, retry: Option<Retry>) -> TestId {
        let id = TestId::new(self.names.pop().unwrap());
        let (tags, before, after, vars, expect) = Default::default();
        let test = Test {
            id,
            file: self.file,
            name: None,
            tags,
            before,
            after,
            body: TestBody::Steps(steps),
            vars,
            expect,
            examples: None,
            repeat: 1,
            retry,
            timeout: None,
            skip: None,
            standalone: false,
            span: span(),
        };
        self.program.tests.insert(id, test);
        self.lifecycle(|_| {});
        id
    }

    /// A new, empty test file whose id is `word`, one of [`WORDS`].
    pub fn test_file(&mut self, word: &str) -> FileId {
        let id = FileId::new(self.sym(word));
        let (name, tags, vars, skip, lifecycle, tests) = Default::default();
        self.program.test_files.insert(id, TestFile { id, name, tags, vars, skip, lifecycle, tests });
        id
    }

    /// `text` parsed as a template; its step roots must be among [`WORDS`].
    pub fn template(&self, text: &str) -> Template {
        let source = ScalarSource { span: span(), style: ScalarStyle::Plain };
        vc_template::parse(text, source, &mut self.strings.clone()).unwrap()
    }

    /// `text` parsed as a template value.
    pub fn value(&self, text: &str) -> TemplateValue {
        TemplateValue::String(self.template(text))
    }

    /// `text` checked as a `$cel` value.
    pub fn cel(&self, text: &str) -> TemplateValue {
        let source = CelSource { text: text.to_owned(), span: span(), style: ScalarStyle::Plain };
        let roots = Roots::new().with_call();
        TemplateValue::Cel(CelCompiler::new().check(&source, &roots, &mut self.strings.clone()).unwrap())
    }

    /// The symbol of one of [`WORDS`].
    pub fn sym(&self, word: &str) -> Sym {
        self.strings.get(word).unwrap()
    }

    /// The var named by one of [`WORDS`].
    pub fn var(&self, word: &str) -> VarName {
        VarName::new(self.sym(word))
    }

    /// Var definitions from `(name, template)` pairs.
    pub fn vars(&self, pairs: &[(&str, &str)]) -> IndexMap<VarName, TemplateValue> {
        pairs.iter().map(|(name, text)| (self.var(name), self.value(text))).collect()
    }

    /// Headers or query parameters from `(name, template)` pairs.
    pub fn templates(&self, pairs: &[(&str, &str)]) -> IndexMap<String, Template> {
        pairs.iter().map(|(name, text)| ((*name).to_owned(), self.template(text))).collect()
    }

    /// The action `id`, to change.
    pub fn action_mut(&mut self, id: ActionId) -> &mut Action {
        &mut self.program.actions[usize::try_from(id.index()).unwrap()]
    }

    /// The api a `call:` action calls, to change.
    pub fn api_mut(&mut self, call: ActionId) -> &mut Api {
        let ActionKind::Call(api) = self.action_mut(call).kind else { unreachable!("not a call") };
        self.program.apis.get_mut(&api).unwrap()
    }

    /// The api file `users`, to change.
    pub fn group_mut(&mut self) -> &mut ApiGroup {
        self.program.api_groups.get_mut(&self.group).unwrap()
    }

    /// The test `id`, to change.
    pub fn test_mut(&mut self, id: TestId) -> &mut Test {
        self.program.tests.get_mut(&id).unwrap()
    }

    /// What a run of the program shares, with the default inputs.
    pub fn cx(self, executors: ActionExecutors) -> Arc<ActionCx> {
        let inputs = inputs(&[]).with_bodies(self.bodies.clone());
        self.cx_with(executors, inputs)
    }

    /// What a run of the program shares, given `inputs`.
    pub fn cx_with(self, executors: ActionExecutors, inputs: RunInputs) -> Arc<ActionCx> {
        Arc::new(ActionCx::new(Arc::new(self.program), Arc::new(executors), inputs))
    }

    /// Changes the lifecycle of `users.crud`.
    pub fn lifecycle(&mut self, change: impl FnOnce(&mut Lifecycle)) {
        if let Some(file) = self.program.test_files.get_mut(&self.file) {
            change(&mut file.lifecycle);
        }
    }

    /// A plan of one file: `before_all`, then `count` instances of each test, all needing every
    /// `before_all` action.
    pub fn plan(&self, before_all: &[ActionId], tests: &[(TestId, u32)]) -> Plan {
        let needs: Needs = before_all.iter().copied().collect();
        let instances = tests
            .iter()
            .flat_map(|&(test, count)| (0..count).map(move |repeat_index| (test, repeat_index)))
            .enumerate()
            .map(|(index, (test, repeat_index))| Instance {
                id: InstanceId::new(u32::try_from(index).unwrap()),
                test,
                example_row: None,
                repeat_index,
                needs: needs.clone(),
            })
            .collect();
        let before_all = before_all.iter().map(|&action| PlannedAction { action, needs: Needs::default() }).collect();
        let file = PlannedFile { id: self.file, before_all, instances, after_all: Vec::new(), skipped: Vec::new() };
        Plan { files: vec![file], ..Plan::default() }
    }
}

/// Run inputs with a seeded random source and `env` as the environment.
pub fn inputs(env: &[(&str, &str)]) -> RunInputs {
    RunInputs::new(Arc::new(ScriptedRandom::seeded(1)), Arc::new(MemoryEnvironment::new(env)))
}

pub fn literal(text: &str) -> Template {
    Template { parts: vec![Part::Literal(text.to_owned())] }
}

pub fn span() -> SourceSpan {
    SourceSpan::new(SourceFileId::new(0), Pos::new(1, 1, 0), Pos::new(1, 1, 0))
}

/// A server that answers by path: `/fail` 500, `/missing` 404, `/flaky` 503 the first time and 200 after,
/// `/sinking` 503 the first time and refuses the connection after, `/slow` 200 after [`SLOW`],
/// anything else 200. It records every path it was sent.
pub struct Server {
    sent: Sender<String>,
    flaky: AtomicUsize,
}

/// The paths a [`Server`] was sent.
pub struct Sent(Receiver<String>);

impl Sent {
    /// How many requests went to each path so far (drains the record).
    pub fn counts(&self) -> BTreeMap<String, usize> {
        let mut counts = BTreeMap::new();
        self.0.try_iter().for_each(|path| *counts.entry(path).or_default() += 1);
        counts
    }
}

impl Server {
    pub fn new() -> (Arc<Self>, Sent) {
        let (sent, paths) = mpsc::channel();
        (Arc::new(Self { sent, flaky: AtomicUsize::new(0) }), Sent(paths))
    }
}

#[async_trait]
impl Transport for Server {
    async fn send(&self, request: RenderedRequest, _: &SendContext) -> Result<Exchange, TransportError> {
        let path = request.uri.path().to_owned();
        self.sent.send(path.clone()).ok();
        if path == "/slow" {
            tokio::time::sleep(SLOW).await;
        }
        let first = || self.flaky.fetch_add(1, Ordering::SeqCst) == 0;
        let code = match path.as_str() {
            "/fail" => 500,
            "/missing" => 404,
            "/flaky" | "/sinking" if first() => 503,
            "/sinking" => return Err(TransportError::Connect { host: "api.test:80".into(), reason: "refused".into() }),
            _ => 200,
        };
        Ok(Exchange {
            request,
            response: vc_testkit::status(code),
            timing: Timing::default(),
            connection: ConnectionInfo::default(),
        })
    }
}
