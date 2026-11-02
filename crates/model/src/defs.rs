use indexmap::IndexMap;

use crate::{ActionId, AuthId, Duration, Expect, FileId, ProfileId, Template, TemplateValue, VarName};

/// Vars set at one level; the executor stacks the levels at run time.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct VarDefs {
    /// `vars:` written at this level.
    pub values: IndexMap<VarName, TemplateValue>,
    /// `profiles.<id>.vars` written at this level.
    pub by_profile: IndexMap<ProfileId, IndexMap<VarName, TemplateValue>>,
}

/// An HTTP method an api may use.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Method {
    /// `GET`, the default.
    #[default]
    Get,
    /// `POST`.
    Post,
    /// `PUT`.
    Put,
    /// `PATCH`.
    Patch,
    /// `DELETE`.
    Delete,
    /// `HEAD`.
    Head,
    /// `OPTIONS`.
    Options,
}

/// A kebab-case tag, as written.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Tag(pub String);

/// Why and until when a file or test is skipped.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Skip {
    /// The reason shown in reports.
    pub reason: String,
    /// The date after which the skip expires and a lint warns, as written (`2026-12-01`).
    pub until: Option<String>,
}

/// `retry` on a test: re-run the whole instance.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Retry {
    /// Further attempts after the first.
    pub attempts: u32,
    /// Pause between attempts.
    pub delay: Duration,
}

/// `poll`: repeat the call until its `until` checks pass.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Poll {
    /// The checks that end the polling.
    pub until: Expect,
    /// Pause between calls.
    pub every: Duration,
    /// Give up after this long.
    pub timeout: Duration,
}

/// `repeat` or `forEach` on an action.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Loop {
    /// Run the action this many times.
    Repeat {
        /// How many times.
        times: u32,
        /// Run the iterations at once.
        parallel: bool,
    },
    /// Run the action once per item of a list.
    ForEach {
        /// The list: a template, a literal list or a dataset.
        items: TemplateValue,
        /// Run the iterations at once.
        parallel: bool,
    },
}

/// `cache` on an action: reuse its saves across runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CacheSpec {
    /// How long a cached result stays valid.
    pub ttl: Duration,
    /// An action whose checks must pass for the cached values to be reused.
    pub check: Option<ActionId>,
}

/// Which auth a call uses.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum AuthRef {
    /// Whatever the api file or config says.
    #[default]
    Inherit,
    /// `auth: none`.
    None,
    /// A named auth, with vars for it.
    Use {
        /// The auth.
        id: AuthId,
        /// Vars passed to it.
        vars: VarDefs,
    },
}

/// A function in a hook file: `hooks/users.ts#makeUser`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct HookRef {
    /// The project-relative file.
    pub file: String,
    /// The exported function.
    pub export: String,
}

/// `save:` names and where each value comes from.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SaveSpec {
    /// Var name to its template over the response.
    pub values: IndexMap<VarName, TemplateValue>,
}

/// `beforeRequest` and `afterResponse` hooks.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RequestHooks {
    /// Called with the request before it is sent.
    pub before_request: Vec<HookRef>,
    /// Called with the response before the checks.
    pub after_response: Vec<HookRef>,
}

/// The rows a test runs once each.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Examples {
    /// Rows written in the test.
    Rows(Vec<TemplateValue>),
    /// A CSV, JSON or YAML file of rows.
    File(Template),
    /// A template that yields a list.
    Template(Template),
}

/// The actions of a lifecycle block, in order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Lifecycle {
    /// Once, before the first test of the file (or run).
    pub before_all: Vec<ActionId>,
    /// Once, after the last test.
    pub after_all: Vec<ActionId>,
    /// Before every test instance.
    pub before_each: Vec<ActionId>,
    /// After every test instance.
    pub after_each: Vec<ActionId>,
}

/// Who owns an action, which decides how long its saves live.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Owner {
    /// `beforeRun` or `afterRun`.
    Run,
    /// A file's `beforeAll` or `afterAll`.
    File(FileId),
    /// One test instance: `beforeEach`, `before`, steps.
    Instance,
}

#[cfg(test)]
mod tests {
    use super::{AuthRef, Method, VarDefs};

    #[test]
    fn defaults_follow_the_syntax_reference() {
        assert_eq!(Method::default(), Method::Get);
        assert_eq!(AuthRef::default(), AuthRef::Inherit);
        assert!(VarDefs::default().values.is_empty());
    }
}
