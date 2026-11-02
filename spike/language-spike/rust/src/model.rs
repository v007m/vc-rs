//! Typed model of the YAML files. serde derives decoding; garde derives
//! validation; `Spanned<T>` keeps the source location of reference fields.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use garde::Validate;
use regex::Regex;
use serde::Deserialize;
use serde_saphyr::Spanned;

static ID_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[A-Za-z][\w-]*(\.[\w-]+)+$").unwrap());
static JSONPATH_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\$(\.[A-Za-z_][\w-]*|\[\d+\])*$").unwrap());

fn valid_id(value: &Spanned<String>, _: &()) -> garde::Result {
    if ID_RE.is_match(&value.value) { Ok(()) } else { Err(garde::Error::new(r#"ids look like "group.name""#)) }
}

fn ids(values: &BTreeMap<String, Spanned<String>>, ctx: &()) -> garde::Result {
    values.values().try_for_each(|v| valid_id(v, ctx))
}

fn jsonpaths<V>(values: &BTreeMap<String, V>, _: &()) -> garde::Result {
    match values.keys().find(|k| !JSONPATH_RE.is_match(k)) {
        Some(bad) => Err(garde::Error::new(format!("`{bad}` is not a JSONPath like $.items[0].id"))),
        None => Ok(()),
    }
}

fn jsonpath_values(values: &BTreeMap<String, String>, _: &()) -> garde::Result {
    match values.values().find(|v| !JSONPATH_RE.is_match(v)) {
        Some(bad) => Err(garde::Error::new(format!("`{bad}` is not a JSONPath like $.items[0].id"))),
        None => Ok(()),
    }
}

/// `"2xx"` or one exact status code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum StatusGuard {
    Class(StatusClass),
    Code(u16),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum StatusClass {
    #[serde(rename = "2xx")]
    Success,
}

impl StatusGuard {
    pub fn matches(self, status: u16) -> bool {
        match self {
            StatusGuard::Class(StatusClass::Success) => (200..300).contains(&status),
            StatusGuard::Code(code) => code == status,
        }
    }
}

impl std::fmt::Display for StatusGuard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StatusGuard::Class(StatusClass::Success) => f.write_str("2xx"),
            StatusGuard::Code(c) => write!(f, "{c}"),
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct Guards {
    #[garde(skip)]
    pub status: StatusGuard,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Method {
    Get,
    Post,
    Put,
    Patch,
    Delete,
}

impl Method {
    pub fn as_str(self) -> &'static str {
        match self {
            Method::Get => "GET",
            Method::Post => "POST",
            Method::Put => "PUT",
            Method::Patch => "PATCH",
            Method::Delete => "DELETE",
        }
    }
}

#[derive(Debug, Clone, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct Api {
    #[garde(custom(valid_id))]
    pub id: Spanned<String>,
    #[garde(skip)]
    pub method: Method,
    #[garde(prefix("/"))]
    pub path: String,
    #[serde(default)]
    #[garde(skip)]
    pub inputs: Vec<String>,
    #[serde(default)]
    #[garde(skip)]
    pub body: Option<serde_json::Value>,
    #[serde(default)]
    #[garde(custom(jsonpath_values))]
    pub outputs: BTreeMap<String, String>,
    #[serde(default)]
    #[garde(dive)]
    pub guards: Option<Guards>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Scope {
    Run,
    File,
    Test,
}

#[derive(Debug, Clone, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct Given {
    #[garde(custom(valid_id))]
    pub id: Spanned<String>,
    #[garde(skip)]
    pub scope: Scope,
    #[garde(custom(valid_id))]
    pub api: Spanned<String>,
    #[serde(default)]
    #[garde(custom(ids))]
    pub uses: BTreeMap<String, Spanned<String>>,
    #[serde(default, rename = "with")]
    #[garde(skip)]
    pub with_: BTreeMap<String, serde_json::Value>,
    #[serde(default)]
    #[garde(skip)]
    pub expose: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct Expect {
    #[serde(default)]
    #[garde(inner(range(min = 100, max = 599)))]
    pub status: Option<u16>,
    #[serde(default)]
    #[garde(custom(jsonpaths))]
    pub r#match: BTreeMap<String, serde_json::Value>,
    #[serde(default)]
    #[garde(skip)]
    pub schema: Option<serde_json::Value>,
    #[serde(default)]
    #[garde(skip)]
    pub cel: Option<Spanned<String>>,
}

fn one() -> u32 {
    1
}

#[derive(Debug, Clone, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct Test {
    #[garde(custom(valid_id))]
    pub id: Spanned<String>,
    #[garde(custom(valid_id))]
    pub api: Spanned<String>,
    #[serde(default)]
    #[garde(custom(ids))]
    pub uses: BTreeMap<String, Spanned<String>>,
    #[serde(default, rename = "with")]
    #[garde(skip)]
    pub with_: BTreeMap<String, serde_json::Value>,
    #[serde(default = "one")]
    #[garde(range(min = 1, max = 10_000))]
    pub repeat: u32,
    #[garde(dive)]
    pub expect: Expect,
}

#[derive(Debug, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct ApiFile {
    #[garde(length(min = 1), dive)]
    pub apis: Vec<Api>,
}

#[derive(Debug, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct GivenFile {
    #[garde(length(min = 1), dive)]
    pub givens: Vec<Given>,
}

#[derive(Debug, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct TestFile {
    #[garde(length(min = 1), dive)]
    pub tests: Vec<Test>,
}

/// 1-based source position.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Span {
    pub file: std::sync::Arc<str>,
    pub line: u32,
    pub col: u32,
    pub len: u32,
}

#[derive(Debug)]
pub struct Located<T> {
    pub def: T,
    pub file: std::sync::Arc<str>,
}

impl<T> Located<T> {
    pub fn span_of<V>(&self, s: &Spanned<V>, len: usize) -> Span {
        Span {
            file: self.file.clone(),
            line: s.referenced.line() as u32,
            col: s.referenced.column() as u32,
            len: len.max(1) as u32,
        }
    }
}

#[derive(Debug, Default)]
pub struct Program {
    pub apis: std::collections::HashMap<String, Located<Api>>,
    pub givens: std::collections::HashMap<String, Located<Given>>,
    /// Deterministic order for cycle reports.
    pub given_order: Vec<String>,
    pub tests: Vec<Located<Test>>,
}
