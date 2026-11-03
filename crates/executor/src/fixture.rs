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
    Action, ActionId, ActionKind, Api, ApiGroup, ApiId, BodySpec, ConnectionInfo, Exchange, Expect, FileId, Instance,
    InstanceId, Lifecycle, Method, Needs, Owner, Part, Plan, PlannedAction, PlannedFile, Pos, Program, RenderedRequest,
    Retry, SaveSpec, SourceFileId, SourceFiles, SourceSpan, StringTable, Sym, Template, Test, TestBody, TestFile,
    TestId, Timing, VarDefs,
};
use vc_testkit::status;

/// How long `/slow` takes to answer.
pub const SLOW: Duration = Duration::from_secs(60);

/// A program with one api file (`users`, based at `http://api.test`) and one test file
/// (`users.crud`), grown by the builder methods.
pub struct Fixture {
    pub program: Program,
    pub file: FileId,
    group: FileId,
    names: Vec<Sym>,
}

impl Fixture {
    pub fn new() -> Self {
        let mut strings = StringTable::new();
        let group = FileId::new(strings.intern("users").unwrap());
        let file = FileId::new(strings.intern("users.crud").unwrap());
        let names = (0..64).map(|n| strings.intern(&format!("users#n{n}")).unwrap()).rev().collect();
        let mut program = Program::new(strings, SourceFiles::new());
        let base_url = Some(literal("http://api.test"));
        let apis = Vec::new();
        let (auth, headers, vars, hooks) = Default::default();
        program
            .api_groups
            .insert(group, ApiGroup { id: group, name: None, base_url, auth, headers, vars, hooks, apis });
        let (tags, vars, lifecycle, tests) = Default::default();
        program.test_files.insert(file, TestFile { id: file, name: None, tags, vars, skip: None, lifecycle, tests });
        Self { program, file, group, names }
    }

    /// A new action of `kind`.
    pub fn action(&mut self, kind: ActionKind) -> ActionId {
        let id = ActionId::new(u32::try_from(self.program.actions.len()).unwrap());
        self.program.actions.push(Action {
            id: None,
            kind,
            owner: Owner::Instance,
            vars: VarDefs::default(),
            auth: None,
            headers: IndexMap::default(),
            query: IndexMap::default(),
            body: None,
            expect: Expect::default(),
            save: SaveSpec::default(),
            poll: None,
            retry: None,
            keep: false,
            cache: None,
            repeat: None,
            span: span(),
        });
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
            expect,
            save,
            cleanup,
            hooks,
            deprecated: false,
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
        let file = PlannedFile { id: self.file, before_all, instances, after_all: Vec::new() };
        Plan { before_run: Vec::new(), files: vec![file], after_run: Vec::new(), skipped: Vec::new() }
    }
}

pub fn literal(text: &str) -> Template {
    Template { parts: vec![Part::Literal(text.to_owned())] }
}

fn span() -> SourceSpan {
    SourceSpan::new(SourceFileId::new(0), Pos::new(1, 1, 0), Pos::new(1, 1, 0))
}

/// A server that answers by path: `/fail` 500, `/flaky` 503 the first time and 200 after,
/// `/slow` 200 after [`SLOW`], anything else 200. It records every path it was sent.
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
        let code = match path.as_str() {
            "/fail" => 500,
            "/flaky" if self.flaky.fetch_add(1, Ordering::SeqCst) == 0 => 503,
            _ => 200,
        };
        Ok(Exchange {
            request,
            response: status(code),
            timing: Timing::default(),
            connection: ConnectionInfo::default(),
        })
    }
}
