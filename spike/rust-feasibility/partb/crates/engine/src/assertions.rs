//! Turn a response into a pass/fail outcome.
use ir::{Assertion, Vars};
use ports::{HttpResponse, Outcome};

pub fn evaluate(assertions: &[Assertion], response: &HttpResponse, vars: &Vars) -> Outcome {
    let failures: Vec<String> = assertions.iter().filter_map(|a| failure(a, response, vars)).collect();
    if failures.is_empty() { Outcome::Passed } else { Outcome::Failed(failures) }
}

fn failure(assertion: &Assertion, response: &HttpResponse, vars: &Vars) -> Option<String> {
    match assertion {
        Assertion::Status(expected) => {
            (response.status != *expected).then(|| format!("expected status {expected}, got {}", response.status))
        }
        Assertion::BodyField { field, expected } => {
            let want = expected.render(vars);
            let got =
                response.body.get(field.as_str()).map(|v| v.as_str().map_or_else(|| v.to_string(), str::to_owned));
            (got.as_deref() != Some(want.as_str())).then(|| format!("expected {field} = {want}, got {got:?}"))
        }
    }
}
