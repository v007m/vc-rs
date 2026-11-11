//! A hand-built program for the planner's unit tests: one api file `users` with `users#list`,
//! one test file `users.crud` at `services/users/crud.test.yaml`, and names interned up front.

use std::collections::HashMap;

use indexmap::IndexMap;
use vc_model::{
    Action, ActionId, ActionKind, Api, ApiGroup, ApiId, BodySpec, ContentHash, Expect, FileId, Instance, InstanceId,
    Matcher, Method, Namespace, Needs, Owner, Part, Path, Plan, PlannedFile, Pos, Program, Reference, SaveSpec,
    Segment, SourceFile, SourceFiles, SourcePath, SourceSpan, StatusPattern, StringTable, Sym, Tag, Template, Test,
    TestBody, TestFile, TestId, VarDefs,
};

/// Every name the fixture can use.
const NAMES: &[&str] = &[
    "users",
    "users.crud",
    "users#list",
    "warm",
    "seed",
    "users.crud#a",
    "users.crud#b",
    "users.crud#c",
    "admin",
    "dev",
    "token",
];

pub struct Fixture {
    pub program: Program,
    pub file: FileId,
    pub api: ApiId,
    names: HashMap<&'static str, Sym>,
}

impl Fixture {
    pub fn new() -> Self {
        let mut strings = StringTable::new();
        let names: HashMap<_, _> = NAMES.iter().map(|name| (*name, strings.intern(name).unwrap())).collect();
        let mut files = SourceFiles::new();
        let path = SourcePath::new("services/users/crud.test.yaml").unwrap();
        files.push(SourceFile { path, text: String::new(), hash: ContentHash::new([0; 32]) }).unwrap();
        let mut program = Program::new(strings, files);
        let (group, file, api) =
            (FileId::new(names["users"]), FileId::new(names["users.crud"]), ApiId::new(names["users#list"]));
        let (auth, headers, vars, hooks) = Default::default();
        let base_url = None;
        program.api_groups.insert(
            group,
            ApiGroup { id: group, name: None, base_url, auth, headers, vars, hooks, source: None, apis: vec![api] },
        );
        program.apis.insert(api, list(group, api));
        let (tags, vars, lifecycle, tests) = Default::default();
        program.test_files.insert(file, TestFile { id: file, name: None, tags, vars, skip: None, lifecycle, tests });
        Self { program, file, api, names }
    }

    pub fn sym(&self, name: &str) -> Sym {
        self.names[name]
    }

    /// A new action of `kind`, named `id` when given.
    pub fn action(&mut self, kind: ActionKind, id: Option<&str>) -> ActionId {
        let action = ActionId::new(u32::try_from(self.program.actions.len()).unwrap());
        self.program.actions.push(Action {
            kind,
            id: id.map(|id| self.sym(id)),
            owner: Owner::Instance,
            span: span(),
            vars: VarDefs::default(),
            headers: IndexMap::new(),
            query: IndexMap::new(),
            auth: None,
            body: None,
            save: SaveSpec::default(),
            expect: Expect::default(),
            cache: None,
            poll: None,
            repeat: None,
            retry: None,
            timeout: None,
            keep: false,
            provides: vec![],
        });
        action
    }

    /// A `call: users#list`, named `id` when given.
    pub fn call(&mut self, id: Option<&str>) -> ActionId {
        self.action(ActionKind::Call(self.api), id)
    }

    /// A new standalone test `users.crud#<name>` calling `users#list`, with `tags`.
    pub fn test(&mut self, name: &str, tags: &[&str]) -> TestId {
        let id = TestId::new(self.sym(&format!("users.crud#{name}")));
        let body = TestBody::Call(self.call(None));
        let tags = tags.iter().map(|tag| Tag((*tag).to_owned())).collect();
        let (before, after, vars, expect) = Default::default();
        let file = self.file;
        let test = Test {
            id,
            file,
            name: None,
            tags,
            body,
            before,
            after,
            vars,
            expect,
            examples: None,
            repeat: 1,
            retry: None,
            timeout: None,
            skip: None,
            standalone: true,
            span: span(),
        };
        self.program.tests.insert(id, test);
        self.program.test_files[&file].tests.push(id);
        id
    }

    pub fn action_mut(&mut self, id: ActionId) -> &mut Action {
        &mut self.program.actions[usize::try_from(id.index()).unwrap()]
    }

    pub fn test_mut(&mut self, id: TestId) -> &mut Test {
        &mut self.program.tests[&id]
    }

    pub fn file_mut(&mut self) -> &mut TestFile {
        &mut self.program.test_files[&self.file]
    }

    /// A plan running one instance of `test`.
    pub fn plan_of(&self, test: TestId) -> Plan {
        let instance =
            Instance { id: InstanceId::new(0), test, example_row: None, repeat_index: 0, needs: Needs::default() };
        let file = PlannedFile {
            id: self.file,
            before_all: vec![],
            instances: vec![instance],
            after_all: vec![],
            skipped: vec![],
        };
        Plan { files: vec![file], ..Plan::default() }
    }
}

/// `${var.<name>.token}`.
pub fn read(name: &str) -> Template {
    let segments = vec![Segment::Key(name.to_owned()), Segment::Key("token".to_owned())];
    let reference = Reference {
        ns: Namespace::Var,
        path: Path { segments },
        args: Vec::new(),
        pipes: Vec::new(),
        span: span(),
        root_span: span(),
    };
    Template { parts: vec![Part::Literal("Bearer ".to_owned()), Part::Ref(reference)] }
}

fn list(group: FileId, id: ApiId) -> Api {
    let (vars, query, headers, auth, expect, save, hooks) = Default::default();
    let path = Template { parts: vec![Part::Literal("/users".to_owned())] };
    Api {
        id,
        group,
        path,
        name: None,
        method: Method::Get,
        body: BodySpec::None,
        vars,
        query,
        headers,
        auth,
        expect,
        save,
        save_on: Matcher::Status(vec![StatusPattern::Class(2)]),
        hooks,
        timeout: None,
        http_retry: None,
        cleanup: None,
        deprecated: false,
        operation_id: None,
        span: span(),
    }
}

pub fn span() -> SourceSpan {
    SourceSpan::new(vc_model::SourceFileId::new(0), Pos::new(1, 1, 0), Pos::new(1, 1, 0))
}
