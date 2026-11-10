//! A run's results in their wire form (C6c): an exchange, a check, a finished test and a failure,
//! as `vibranium.results.v1` carries them in the `Run` stream. The run's redactor masked them
//! before they reached the feed; nothing here adds a value the run did not send.

use vc_contracts::millis;
use vc_contracts::results::v1 as results;
use vc_engine::{ActionId, CheckResult, Exchange, Failure, Names, Origin, TestResult, check_parts};

use crate::wire_diagnostics;

/// `exchange`, which `action` made. The body is sent whole, so `body_truncated` is false.
pub fn exchange(names: &Names, action: ActionId, exchange: &Exchange) -> results::Exchange {
    let (request, response) = (&exchange.request, &exchange.response);
    results::Exchange {
        action: names.action(action),
        api: names.api_of(action),
        method: request.method.as_str().to_owned(),
        url: request.uri.to_string(),
        status: Some(u32::from(response.status.as_u16())),
        timing: Some(exchange.timing.into()),
        request_headers: headers(request.headers.iter()),
        response_headers: headers(response.headers.iter()),
        request_body: request.body.to_vec(),
        response_body: response.body.to_vec(),
        body_truncated: false,
        wire_size: response.wire_size,
        connection_reused: exchange.connection.reused,
        protocol: results::Protocol::from(exchange.connection.protocol).into(),
    }
}

/// Header pairs as they were sent; a value that is not UTF-8 is shown lossily.
fn headers<K: AsRef<str>, V: AsRef<[u8]>>(pairs: impl Iterator<Item = (K, V)>) -> Vec<results::Header> {
    let header = |(name, value): (K, V)| results::Header {
        name: name.as_ref().to_owned(),
        value: String::from_utf8_lossy(value.as_ref()).into_owned(),
    };
    pairs.map(header).collect()
}

/// `check`, with where it was written and its expected and actual values as JSON text.
pub fn check(names: &Names, check: &CheckResult) -> results::Check {
    results::Check {
        kind: results::CheckKind::from(check.kind).into(),
        target: check_parts(check).1,
        origin: origin(names, check.origin),
        span: Some(wire_diagnostics::span(&names.program().files, check.span)),
        passed: check.passed,
        expected: check.expected.to_string(),
        actual: check.actual.as_ref().map(ToString::to_string),
        diff: check.diff.clone(),
        error: check.error.clone(),
        code: check.code.clone(),
    }
}

/// Where an expectation was written, as the reporters show it: `api users#create`,
/// `test users.crud#getsUser`, `action createUser`.
fn origin(names: &Names, origin: Origin) -> String {
    match origin {
        Origin::Api(api) => format!("api {}", names.text(api.sym())),
        Origin::Test(test) => format!("test {}", names.test(test)),
        Origin::Action(action) => format!("action {}", names.action(action)),
    }
}

/// A finished test, timed by the executor. Its checks and exchanges came earlier in the stream
/// as their own events.
pub fn test(names: &Names, result: &TestResult) -> results::Test {
    let (example_row, repeat_index) = names.slot(result);
    results::Test {
        id: names.test(result.test).to_owned(),
        label: names.instance(result),
        example_row,
        repeat_index,
        status: results::TestStatus::from(result.status).into(),
        attempts: result.attempts,
        duration_ms: millis(result.duration),
        failure: result.failure.as_ref().map(|found| failure(names, found)),
        blocked_by: None,
        skip_reason: result.skip_reason.clone(),
        checks: Vec::new(),
        exchanges: Vec::new(),
    }
}

/// `failure`, with the action it names by its label.
pub fn failure(names: &Names, failure: &Failure) -> results::Failure {
    results::Failure {
        action: failure.action.map(|action| names.action(action)),
        code: failure.code.clone(),
        message: failure.message.clone(),
    }
}
