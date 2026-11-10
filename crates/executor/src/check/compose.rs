//! Matchers over matchers: `every`, `some`, `none`, `not`, `allOf`, `anyOf`.

use serde_json::Value;
use vc_model::{Matcher, MatcherArg};

use super::{Judge, MatcherRegistry};

/// `registry` with the combining matchers.
pub fn register(registry: MatcherRegistry) -> MatcherRegistry {
    registry
        .with("every", |judge, actual, arg| each(judge, actual, arg).map(|seen| seen.is_some_and(Seen::all)))
        .with("some", |judge, actual, arg| {
            each(judge, actual, arg).map(|seen| seen.is_some_and(|seen| seen.passed > 0))
        })
        .with("none", |judge, actual, arg| {
            each(judge, actual, arg).map(|seen| seen.is_some_and(|seen| seen.passed == 0))
        })
        .with("not", |judge, actual, arg| judge.matches(nested(arg)?, actual).map(|passed| !passed))
        .with("allOf", |judge, actual, arg| several(judge, actual, arg).map(Seen::all))
        .with("anyOf", |judge, actual, arg| several(judge, actual, arg).map(|seen| seen.passed > 0))
}

/// How many of a list's elements, or of several matchers, passed.
#[derive(Clone, Copy, Debug)]
struct Seen {
    passed: usize,
    total: usize,
}

impl Seen {
    const fn all(self) -> bool {
        self.passed == self.total
    }
}

fn nested(arg: &MatcherArg) -> Result<&Matcher, String> {
    match arg {
        MatcherArg::Matcher(matcher) => Ok(matcher),
        _ => Err("expected a matcher".to_owned()),
    }
}

/// The nested matcher applied to each element of the list; `None` when `actual` is no list.
fn each(judge: &Judge<'_>, actual: Option<&Value>, arg: &MatcherArg) -> Result<Option<Seen>, String> {
    let matcher = nested(arg)?;
    let Some(items) = actual.and_then(Value::as_array) else { return Ok(None) };
    let mut passed = 0;
    for item in items {
        passed += usize::from(judge.matches(matcher, Some(item))?);
    }
    Ok(Some(Seen { passed, total: items.len() }))
}

/// Each of several matchers applied to `actual`.
fn several(judge: &Judge<'_>, actual: Option<&Value>, arg: &MatcherArg) -> Result<Seen, String> {
    let MatcherArg::Matchers(matchers) = arg else { return Err("expected a list of matchers".to_owned()) };
    let mut passed = 0;
    for matcher in matchers {
        passed += usize::from(judge.matches(matcher, actual)?);
    }
    Ok(Seen { passed, total: matchers.len() })
}
