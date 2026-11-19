//! What a response's GraphQL errors hold back (D345, D361): an api's default `save` and its
//! `cleanup` wait for a response without errors, a response its runtime cannot read fails the
//! call (V5124), and the errors of a call that judged no `expect.errors` check are listed as a
//! warning (V5128, D364, D365), with one more when a save or cleanup was held back (V5129).

use serde_json::Value;
use vc_diagnostics::Code;
use vc_model::{Api, CheckKind, CheckResult, Failure, Origin, SourceSpan};

/// A response's errors as its runtime read them.
#[derive(Debug)]
pub struct Held {
    errors: Vec<Value>,
    unread: Option<Failure>,
}

impl Held {
    /// The errors `read` gave, or the failure of a response it could not read.
    pub fn new(read: Result<Vec<Value>, Failure>) -> Self {
        match read {
            Ok(errors) => Self { errors, unread: None },
            Err(failure) => Self { errors: Vec::new(), unread: Some(failure) },
        }
    }

    /// Whether the response was read and has no errors, so the api's `cleanup` may be registered.
    pub const fn clean(&self) -> bool {
        self.unread.is_none() && self.errors.is_empty()
    }

    /// Whether `api`'s `save` may run once its `saveOn` matched: an explicit `saveOn` is status
    /// only, the default one needs no errors.
    pub const fn saves(&self, api: &Api) -> bool {
        api.save_on_written || self.errors.is_empty()
    }

    /// `checks` with a response its runtime could not read reported first, so the call's verdict
    /// is V5124 and it shows with any check that failed too. One result per cause: a judged
    /// `expect.errors` check, which failed on that body, becomes it; else it is the call's, at
    /// `at`, the call's origin and span.
    pub fn reported(&self, checks: Vec<CheckResult>, at: (Origin, SourceSpan)) -> Vec<CheckResult> {
        let Some(failure) = &self.unread else { return checks };
        let (mut own, rest): (Vec<_>, Vec<_>) = checks.into_iter().partition(|check| check.kind == CheckKind::Errors);
        let unread = CheckResult {
            passed: false,
            error: Some(failure.message.clone()),
            code: failure.code.clone(),
            ..own.pop().unwrap_or_else(|| CheckResult {
                kind: CheckKind::Errors,
                target: None,
                origin: at.0,
                span: at.1,
                passed: false,
                expected: Value::String("a GraphQL response".to_owned()),
                actual: None,
                diff: None,
                error: None,
                code: None,
            })
        };
        std::iter::once(unread).chain(own).chain(rest).collect()
    }

    /// V5128 listing the errors when `checks`, the ones judged, hold no `expect.errors` check (a
    /// passing one asserted them, a failing one is V5123; D364); a step's check that a load
    /// iteration with `checks: api` skipped did not run, so it lists them (D365). V5129 when
    /// they `withheld` a save or cleanup. None for a response without errors.
    pub fn warnings(&self, checks: &[CheckResult], withheld: bool) -> Vec<Failure> {
        if self.errors.is_empty() {
            return Vec::new();
        }
        let asserted = checks.iter().any(|check| check.kind == CheckKind::Errors);
        let listed = (!asserted).then(|| listed(&self.errors));
        let skipped = withheld.then(|| {
            let message = "Save skipped: the response has GraphQL errors, which hold back the api's default `save` \
                           and its `cleanup` (an explicit `saveOn` is status only)";
            Failure::new(message).with_code(Code::V5129.to_string())
        });
        listed.into_iter().chain(skipped).collect()
    }
}

/// V5128: "The response has GraphQL errors", each as `CODE at path: message`.
fn listed(errors: &[Value]) -> Failure {
    let each: Vec<_> = errors.iter().map(line).collect();
    Failure::new(format!("The response has GraphQL errors: {}", each.join("; "))).with_code(Code::V5128.to_string())
}

/// One normalised error as `CODE at path: message`, leaving out the parts it lacks.
fn line(error: &Value) -> String {
    let text = |name: &str| match error.get(name) {
        Some(Value::String(text)) => Some(text.clone()),
        None | Some(Value::Null) => None,
        Some(other) => Some(other.to_string()),
    };
    let path = text("path").map(|path| format!("at {path}"));
    let head = [text("code"), path].into_iter().flatten().collect::<Vec<_>>().join(" ");
    match (head.is_empty(), text("message")) {
        (true, None) => "an error with no message".to_owned(),
        (false, None) => head,
        (true, Some(message)) => message,
        (false, Some(message)) => format!("{head}: {message}"),
    }
}

#[cfg(test)]
#[path = "held_tests.rs"]
mod tests;
