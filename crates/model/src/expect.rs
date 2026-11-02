use crate::{ActionId, ApiId, Path, SourceSpan, StatusPattern, TemplateValue, TestId};

/// The merged checks for one call: api, then test, then step, the more specific replacing a
/// check with the same key; `cel` and `custom` checks are added instead (D123).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Expect {
    /// The checks, in source order within each level.
    pub checks: Vec<Check>,
    /// `replace: true`: drop every inherited check.
    pub replace: bool,
}

/// One check, reported on its own with its source line.
#[derive(Clone, Debug, PartialEq, Eq)]
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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CheckKind {
    /// `status`.
    Status,
    /// `headers.<name>`.
    Header,
    /// `body.schema`.
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

/// The level a check was written at.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Origin {
    /// The api's own `expect`.
    Api(ApiId),
    /// A test's `expect`.
    Test(TestId),
    /// A step's or action's `expect`.
    Action(ActionId),
}

/// A literal (equals) or a set of matchers that must all pass.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Matcher {
    /// The value equals this one (deep for objects).
    Equals(TemplateValue),
    /// Every call must pass: `{ type: string, length: 32 }`.
    All(Vec<MatcherCall>),
    /// A status code, list of codes or class.
    Status(Vec<StatusPattern>),
}

/// One registered matcher with its argument, such as `gt: 0`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MatcherCall {
    /// The matcher's name as registered (`gt`, `every`, `format`).
    pub name: String,
    /// Its compiled argument.
    pub arg: MatcherArg,
    /// Where the matcher key was written.
    pub span: SourceSpan,
}

/// A matcher's argument, already checked against the matcher's signature (V2306).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MatcherArg {
    /// A value: a number, text, list or flag; durations and sizes are stored in ms and bytes.
    Value(TemplateValue),
    /// One nested matcher (`not`, `every`, `some`, `none`, `length`).
    Matcher(Box<Matcher>),
    /// Several nested matchers (`allOf`, `anyOf`).
    Matchers(Vec<Matcher>),
}

#[cfg(test)]
mod tests {
    use super::{Expect, Matcher, MatcherArg};
    use crate::TemplateValue;

    #[test]
    fn expect_defaults_to_no_checks() {
        let expect = Expect::default();
        assert!(expect.checks.is_empty() && !expect.replace);
        let nested = MatcherArg::Matcher(Box::new(Matcher::Equals(TemplateValue::Null)));
        assert_ne!(nested, MatcherArg::Value(TemplateValue::Null));
    }
}
