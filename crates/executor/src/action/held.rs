//! What a response's GraphQL errors hold back (D345, D361): an api's default `save` and its
//! `cleanup` wait for a response without errors, a response its runtime cannot read fails the
//! call (V5124), and the errors of a call with no `expect.errors` check are listed as a warning
//! (V5128, D364), with one more when a save or cleanup was held back (V5129).

use serde_json::Value;
use vc_diagnostics::Code;
use vc_model::{Api, CheckKind, CheckResult, Expect, Failure, Origin};

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

    /// The failure of a response its runtime could not read (V5124).
    pub fn failure(&self) -> Option<Failure> {
        self.unread.clone()
    }

    /// That failure as a failed `errors` check of `api`'s, so it is reported along with the
    /// checks that failed too, not instead of them.
    pub fn check(&self, api: &Api) -> Option<CheckResult> {
        self.unread.as_ref().map(|failure| CheckResult {
            kind: CheckKind::Errors,
            target: None,
            origin: Origin::Api(api.id),
            span: api.span,
            passed: false,
            expected: Value::String("a GraphQL response".to_owned()),
            actual: None,
            diff: None,
            error: Some(failure.message.clone()),
            code: failure.code.clone(),
        })
    }

    /// V5128 listing the errors when `expect`, the call's checks as written, has no `expect.errors`
    /// check (a passing one asserted them, a failing one is V5123; D364), also in a load iteration
    /// that judges only the api's; and V5129 when they `withheld` a save or cleanup; none for a
    /// response without errors. Both go out masked, with every event (`RedactingSink`).
    pub fn warnings(&self, expect: &Expect, withheld: bool) -> Vec<Failure> {
        if self.errors.is_empty() {
            return Vec::new();
        }
        let asserted = expect.checks.iter().any(|check| check.kind == CheckKind::Errors);
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
