//! The internal `paths` matcher a message check's `data.match` lowers to (D379): each path read
//! from the message's `data` as `body.match` reads the body.

use serde_json::Value;
use vc_model::MatcherArg;

use super::{Judge, MatcherRegistry};

/// `registry` with `paths`, which holds when each path's matcher holds on the value there.
pub fn register(registry: MatcherRegistry) -> MatcherRegistry {
    registry.internal("paths", paths)
}

fn paths(judge: &Judge<'_>, actual: Option<&Value>, arg: &MatcherArg) -> Result<bool, String> {
    let MatcherArg::Paths(paths) = arg else { return Err("expected a matcher per path".to_owned()) };
    for (path, matcher) in paths {
        let value = actual.and_then(|data| vc_template::select("data", data, &path.segments).ok());
        if !judge.matches(matcher, value.as_ref())? {
            return Ok(false);
        }
    }
    Ok(true)
}

#[cfg(test)]
#[path = "messages_tests.rs"]
mod tests;
