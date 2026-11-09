//! `http.retry` (D143): the config's keys over the defaults, and an api's keys over the config's.

use vc_model::{HttpRetry, Method, StatusPattern};
use vc_parser::Node;

use super::read;
use crate::link::project;
use crate::link::project::Source;

/// `retry` under the `http` at `node`, each key it writes replacing `base`'s; `None` when there
/// is none.
pub fn over(source: &Source<'_>, node: &Node, base: &HttpRetry) -> Option<HttpRetry> {
    let retry = node.get("http")?.get("retry")?;
    let mut merged = base.clone();
    if let Some(attempts) = read::number(retry, "attempts") {
        merged.attempts = attempts;
    }
    if retry.get("on").is_some() {
        (merged.on_status, merged.on_transport) = on(retry);
    }
    if retry.get("methods").is_some() {
        merged.methods = read::texts(retry, "methods").iter().filter_map(|name| Method::named(name)).collect();
    }
    if let Some(backoff) = retry.get("backoff") {
        merged.base = read::duration(source, backoff, "base").unwrap_or(merged.base);
        merged.max = read::duration(source, backoff, "max").unwrap_or(merged.max);
        let jitter = project::text(backoff, "jitter").map(|text| text.eq_ignore_ascii_case("true"));
        merged.jitter = jitter.unwrap_or(merged.jitter);
    }
    Some(merged)
}

/// `on`: the statuses, and the transport failures (`connect`, `timeout`) by name.
fn on(retry: &Node) -> (Vec<StatusPattern>, Vec<String>) {
    let mut statuses = Vec::new();
    let mut failures = Vec::new();
    for entry in read::texts(retry, "on") {
        match entry.parse() {
            Ok(code) => statuses.push(StatusPattern::Code(code)),
            Err(_) => failures.push(entry),
        }
    }
    (statuses, failures)
}
