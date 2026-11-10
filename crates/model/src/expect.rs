use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{ActionId, ApiId, Duration, Path, Size, SourceSpan, StatusPattern, TemplateValue, TestId};

/// The merged checks for one call: api, then test, then step, the more specific replacing a
/// check with the same key; `cel` and `custom` checks are added instead (D123).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "vc-model data type, shared across the workspace; every user is in this repo (D160)"
)]
pub struct Expect {
    /// The checks, in source order within each level.
    pub checks: Vec<Check>,
    /// `replace: true`: drop every inherited check.
    pub replace: bool,
}

/// One check, reported on its own with its source line.
#[derive(Clone, Debug, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "vc-model data type, shared across the workspace; every user is in this repo (D160)"
)]
pub struct Check {
    /// What it looks at.
    pub kind: CheckKind,
    /// The path it reads, such as `body.items[*].price`; `None` for status, size and the like.
    pub target: Option<Path>,
    /// The matcher with its compiled arguments.
    pub matcher: Matcher,
    /// The level that wrote it.
    pub origin: Origin,
    /// The YAML line reported when it fails.
    pub span: SourceSpan,
}

/// The kinds of check, one per `expect` key.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[expect(
    clippy::exhaustive_enums,
    reason = "vc-model data type, shared across the workspace; every user is in this repo (D160)"
)]
pub enum CheckKind {
    /// `status`.
    Status,
    /// `headers.<name>`.
    Header,
    /// `body.schema`: the matcher is the literal key of the schema in [`crate::Program::schemas`],
    /// with any `#/pointer` inside it (D201).
    Schema,
    /// `body.match.<path>`.
    Match,
    /// `body.equals`.
    Equals,
    /// `size`.
    Size,
    /// `timing.<phase>`.
    Timing,
    /// `cel.<label>`.
    Cel,
    /// `custom`: a function.
    Custom,
}

impl CheckKind {
    /// Every kind, in `expect` key order.
    pub const ALL: [Self; 9] = [
        Self::Status,
        Self::Header,
        Self::Schema,
        Self::Match,
        Self::Equals,
        Self::Size,
        Self::Timing,
        Self::Cel,
        Self::Custom,
    ];

    /// The `expect` key it is written under, which the history stores: `body.match`.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Status => "status",
            Self::Header => "headers",
            Self::Schema => "body.schema",
            Self::Match => "body.match",
            Self::Equals => "body.equals",
            Self::Size => "size",
            Self::Timing => "timing",
            Self::Cel => "cel",
            Self::Custom => "custom",
        }
    }

    /// The kind written under `key`, as [`CheckKind::key`] gives it.
    #[must_use]
    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.key() == key)
    }
}

/// The level a check was written at.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[expect(
    clippy::exhaustive_enums,
    reason = "vc-model data type, shared across the workspace; every user is in this repo (D160)"
)]
pub enum Origin {
    /// The api's own `expect`.
    Api(ApiId),
    /// A test's `expect`.
    Test(TestId),
    /// A step's or action's `expect`.
    Action(ActionId),
}

/// A literal (equals) or a set of matchers that must all pass.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[expect(
    clippy::exhaustive_enums,
    reason = "vc-model data type, shared across the workspace; every user is in this repo (D160)"
)]
pub enum Matcher {
    /// The value equals this one (deep for objects).
    Equals(TemplateValue),
    /// Every call must pass: `{ type: string, length: 32 }`.
    All(Vec<MatcherCall>),
    /// A status code, list of codes or class.
    Status(Vec<StatusPattern>),
}

/// One registered matcher with its argument, such as `gt: 0`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[expect(
    clippy::exhaustive_structs,
    reason = "vc-model data type, shared across the workspace; every user is in this repo (D160)"
)]
pub struct MatcherCall {
    /// The matcher's name as registered (`gt`, `every`, `format`).
    pub name: String,
    /// Its compiled argument.
    pub arg: MatcherArg,
    /// Where the matcher key was written.
    pub span: SourceSpan,
}

/// A matcher's argument, already checked against the matcher's signature (V2306).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[expect(
    clippy::exhaustive_enums,
    reason = "vc-model data type, shared across the workspace; every user is in this repo (D160)"
)]
pub enum MatcherArg {
    /// A value: a number, text, list or flag.
    Value(TemplateValue),
    /// A duration, for `gt`, `lt` and the like under `timing` and load latency thresholds.
    Duration(Duration),
    /// A size, for `gt`, `lt` and the like under `size`.
    Size(Size),
    /// The inclusive bounds of `between`.
    Range(Box<[Self; 2]>),
    /// One nested matcher (`not`, `every`, `some`, `none`, `length`).
    Matcher(Box<Matcher>),
    /// Several nested matchers (`allOf`, `anyOf`).
    Matchers(Vec<Matcher>),
}

/// One evaluated check, reported on its own: where it was written, what it expected, what came
/// back and, for objects and lists that differ, a line diff.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[expect(
    clippy::exhaustive_structs,
    reason = "vc-model data type, shared across the workspace; every user is in this repo (D160)"
)]
pub struct CheckResult {
    /// What the check reads.
    pub kind: CheckKind,
    /// The header, body path or timing phase it reads.
    pub target: Option<Path>,
    /// The level it was written at.
    pub origin: Origin,
    /// Where it was written.
    pub span: SourceSpan,
    /// Whether it held.
    pub passed: bool,
    /// What it expected, rendered: a value, or the matchers with their arguments.
    pub expected: Value,
    /// What the response held there; `None` when the path is absent.
    pub actual: Option<Value>,
    /// A unified diff of the expected and actual JSON, when both are objects or lists.
    pub diff: Option<String>,
    /// Why it could not be evaluated, such as a matcher argument that did not render.
    pub error: Option<String>,
    /// The diagnostic code if it failed: V5101, V5102 for CEL, V5103 for custom.
    pub code: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::{CheckKind, Expect, Matcher, MatcherArg};
    use crate::TemplateValue;

    #[test]
    fn expect_defaults_to_no_checks() {
        let expect = Expect::default();
        assert!(expect.checks.is_empty() && !expect.replace);
        let nested = MatcherArg::Matcher(Box::new(Matcher::Equals(TemplateValue::Null)));
        assert_ne!(nested, MatcherArg::Value(TemplateValue::Null));
    }

    #[test]
    fn every_check_kind_is_listed_once_and_read_back_from_its_key() {
        let listed = |kind: CheckKind| match kind {
            CheckKind::Status
            | CheckKind::Header
            | CheckKind::Schema
            | CheckKind::Match
            | CheckKind::Equals
            | CheckKind::Size
            | CheckKind::Timing
            | CheckKind::Cel
            | CheckKind::Custom => CheckKind::ALL.iter().filter(|other| **other == kind).count(),
        };
        for kind in CheckKind::ALL {
            assert_eq!((listed(kind), CheckKind::from_key(kind.key())), (1, Some(kind)), "{kind:?}");
        }
        assert_eq!(CheckKind::from_key("body"), None);
    }
}
