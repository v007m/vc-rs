//! Value types of the IR. Newtypes keep ids, names and templates apart.
use std::collections::BTreeMap;
use std::fmt;
use std::hash::{DefaultHasher, Hash, Hasher};

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Index of a test in [`crate::Program`]'s arena.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
pub struct TestId(u32);

impl TestId {
    pub(crate) const fn new(index: u32) -> Self {
        Self(index)
    }

    #[must_use]
    pub const fn index(self) -> usize {
        self.0 as usize
    }

    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// Index of a source file in [`crate::Program`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FileId(u32);

impl FileId {
    pub(crate) const fn new(index: u32) -> Self {
        Self(index)
    }

    #[must_use]
    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

macro_rules! string_newtype {
    ($($(#[$doc:meta])* $name:ident),+ $(,)?) => {$(
        $(#[$doc])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            #[must_use]
            pub fn new(value: impl Into<String>) -> Self {
                Self(value.into())
            }

            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }
    )+};
}

string_newtype!(
    /// Human readable test name.
    TestName,
    /// Source file name.
    FileName,
    /// Name of a variable produced by `extract` and consumed by `{{name}}`.
    VarName,
    /// Top-level field of a JSON response body.
    FieldName,
    /// HTTP header name.
    HeaderName,
    /// Rendered request path (no templates left).
    UrlPath,
);

/// HTTP status code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct StatusCode(u16);

impl StatusCode {
    #[must_use]
    pub const fn new(code: u16) -> Self {
        Self(code)
    }

    #[must_use]
    pub const fn get(self) -> u16 {
        self.0
    }
}

impl fmt::Display for StatusCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Method {
    Get,
    Post,
    Put,
    Patch,
    Delete,
}

/// Variables available to templates. Ordered so it hashes deterministically.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct Vars(BTreeMap<VarName, String>);

impl Vars {
    #[must_use]
    pub fn get(&self, name: &VarName) -> Option<&str> {
        self.0.get(name).map(String::as_str)
    }

    pub fn insert(&mut self, name: VarName, value: impl Into<String>) {
        self.0.insert(name, value.into());
    }

    /// `self` overlaid with `other` (other wins).
    #[must_use]
    pub fn merged(&self, other: &Self) -> Self {
        let mut out = self.clone();
        out.0.extend(other.0.iter().map(|(k, v)| (k.clone(), v.clone())));
        out
    }

    /// Stable within one process; used as part of setup memoization keys.
    #[must_use]
    pub fn fingerprint(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        self.hash(&mut hasher);
        hasher.finish()
    }
}

/// A string with `{{var}}` placeholders.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Template(String);

impl Template {
    #[must_use]
    pub fn new(text: impl Into<String>) -> Self {
        Self(text.into())
    }

    /// Replace each `{{name}}` with its value; unknown names are left as-is.
    #[must_use]
    pub fn render(&self, vars: &Vars) -> String {
        let mut out = String::with_capacity(self.0.len());
        let mut rest = self.0.as_str();
        while let Some(start) = rest.find("{{") {
            let Some(len) = rest[start..].find("}}") else { break };
            let name = VarName::new(rest[start + 2..start + len].trim());
            out.push_str(&rest[..start]);
            out.push_str(vars.get(&name).unwrap_or_else(|| &rest[start..start + len + 2]));
            rest = &rest[start + len + 2..];
        }
        out.push_str(rest);
        out
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestSpec {
    pub method: Method,
    pub path: Template,
    pub headers: Vec<(HeaderName, Template)>,
    pub body: Option<Value>,
}

/// Copy a top-level response field into a variable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Extract {
    pub var: VarName,
    pub field: FieldName,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Assertion {
    Status(StatusCode),
    BodyField { field: FieldName, expected: Template },
}

/// How long a setup result is shared.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Scope {
    /// Once per source file.
    File,
    /// Once per run.
    Run,
}

/// "Run test `test` as my setup, shared within `scope`."
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SetupRef {
    pub test: TestId,
    pub scope: Scope,
}

/// A TypeScript hook export, e.g. `hooks/sign.ts#addSignature`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct HookRef {
    pub module: FileName,
    pub export: VarName,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// Runs on its own and may also be reused as a setup.
    Test,
    /// Only ever runs as another test's setup.
    SetupOnly,
}

/// A test before it has an id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestDraft {
    pub name: TestName,
    pub file: FileId,
    pub role: Role,
    pub request: RequestSpec,
    pub setup: Option<SetupRef>,
    pub extract: Vec<Extract>,
    pub assertions: Vec<Assertion>,
    pub before_request: Option<HookRef>,
}

/// A compiled test: a draft plus its arena id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Test {
    pub id: TestId,
    pub spec: TestDraft,
}
