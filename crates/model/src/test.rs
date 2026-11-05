use indexmap::IndexMap;

use crate::{
    ApiId, AuthRef, BodySpec, CacheSpec, Duration, Examples, Expect, FileId, HookRef, Lifecycle, Loop, Method, Owner,
    Poll, Retry, SaveSpec, Skip, SourceSpan, Sym, Tag, Template, TestId, VarDefs, VarName,
};

/// One test file: what its tests share.
#[derive(Clone, Debug, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "vc-model data type, shared across the workspace; every user is in this repo (D160)"
)]
pub struct TestFile {
    /// The file ID, from the path (D139).
    pub id: FileId,
    /// `name`.
    pub name: Option<String>,
    /// `tags`, added to every test.
    pub tags: Vec<Tag>,
    /// `vars` and `profiles.<id>.vars`.
    pub vars: VarDefs,
    /// `skip` for every test in the file.
    pub skip: Option<Skip>,
    /// `beforeAll`, `afterAll`, `beforeEach`, `afterEach`.
    pub lifecycle: Lifecycle,
    /// The tests, in file order.
    pub tests: Vec<TestId>,
}

/// One test.
#[derive(Clone, Debug, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "vc-model data type, shared across the workspace; every user is in this repo (D160)"
)]
pub struct Test {
    /// `<fileId>#<id>` (D138).
    pub id: TestId,
    /// Its test file.
    pub file: FileId,
    /// `name`.
    pub name: Option<String>,
    /// `tags`.
    pub tags: Vec<Tag>,
    /// `before`.
    pub before: Vec<crate::ActionId>,
    /// `after`.
    pub after: Vec<crate::ActionId>,
    /// `call` or `steps`.
    pub body: TestBody,
    /// `vars`.
    pub vars: VarDefs,
    /// Applied to the call; with steps, a default for every step (D124).
    pub expect: Expect,
    /// `examples`.
    pub examples: Option<Examples>,
    /// `repeat`; 1 when not written.
    pub repeat: u32,
    /// `retry`.
    pub retry: Option<Retry>,
    /// `timeout`.
    pub timeout: Option<Duration>,
    /// `skip`.
    pub skip: Option<Skip>,
    /// `standalone`; true when not written.
    pub standalone: bool,
    /// The whole item.
    pub span: SourceSpan,
}

/// What a test runs.
#[derive(Clone, Debug, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_enums,
    reason = "vc-model data type, shared across the workspace; every user is in this repo (D160)"
)]
pub enum TestBody {
    /// `call:` one api.
    Call(crate::ActionId),
    /// `steps:`, in order.
    Steps(Vec<crate::ActionId>),
}

/// One entry in a lifecycle block, a test's steps, or a flow auth.
#[derive(Clone, Debug, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "vc-model data type, shared across the workspace; every user is in this repo (D160)"
)]
pub struct Action {
    /// `id`; needed in `beforeRun` and `beforeAll` when the action saves values.
    pub id: Option<Sym>,
    /// What it runs.
    pub kind: ActionKind,
    /// Who owns its saves.
    pub owner: Owner,
    /// `vars`.
    pub vars: VarDefs,
    /// `auth`.
    pub auth: Option<AuthRef>,
    /// `headers`, over the api's.
    pub headers: IndexMap<String, Template>,
    /// `query`, over the api's.
    pub query: IndexMap<String, Template>,
    /// `body`, replacing the api's.
    pub body: Option<BodySpec>,
    /// Merged over the api's `expect` at compile time.
    pub expect: Expect,
    /// `save`.
    pub save: SaveSpec,
    /// `poll`.
    pub poll: Option<Poll>,
    /// `retry`.
    pub retry: Option<Retry>,
    /// `keep`: skip the api's cleanup.
    pub keep: bool,
    /// `provides`: the names a `function:` or `test:` action says it saves (D128).
    pub provides: Vec<VarName>,
    /// `cache`.
    pub cache: Option<CacheSpec>,
    /// `repeat` or `forEach`.
    pub repeat: Option<Loop>,
    /// The whole entry.
    pub span: SourceSpan,
}

/// The kinds of action.
#[derive(Clone, Debug, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_enums,
    reason = "vc-model data type, shared across the workspace; every user is in this repo (D160)"
)]
pub enum ActionKind {
    /// `call:` an api.
    Call(ApiId),
    /// `test:` reuse another test's calls.
    Test(TestId),
    /// `function:` a hook export.
    Function(HookRef),
    /// `command:` a shell command.
    Command(Template),
    /// A request written in place, in flow auth steps only.
    Inline(Box<InlineRequest>),
}

/// A request written in place in a flow auth.
#[derive(Clone, Debug, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "vc-model data type, shared across the workspace; every user is in this repo (D160)"
)]
pub struct InlineRequest {
    /// `method`.
    pub method: Method,
    /// `url`.
    pub url: Template,
    /// `query`, appended to the URL.
    pub query: IndexMap<String, Template>,
    /// `headers`.
    pub headers: IndexMap<String, Template>,
    /// `body`.
    pub body: BodySpec,
}

#[cfg(test)]
mod tests {
    use super::{ActionKind, TestBody};
    use crate::{ActionId, Template};

    #[test]
    fn bodies_and_kinds_compare_by_value() {
        assert_ne!(TestBody::Call(ActionId::new(0)), TestBody::Steps(vec![ActionId::new(0)]));
        assert_eq!(ActionKind::Command(Template::default()), ActionKind::Command(Template::default()));
    }
}
