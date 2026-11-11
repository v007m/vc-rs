//! What `vc call` and the web UI's Client get back from one ad hoc call: its exit and text, and
//! the masked exchange, checks and saved values the Client shows as tabs (D245 P1).

use std::fmt::Write as _;

use serde_json::Value;
use vc_executor::{ActionCx, ActionReport, MASKED, Redactor};
use vc_model::{
    ActionId, ActionKind, ActionOutcome, Api, CheckResult, Exchange, Names, Namespace, Program, TemplateValue,
};

use crate::shown::Shown;

/// What `vc call` got back. Everything in it went through the run's redactor.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct CallReport {
    /// 0 when the api's expect held, 1 when it did not or nothing came back.
    pub exit: u8,
    /// `200 OK · 148 ms · api expect passed`, the body, and the failure when there is one.
    pub text: String,
    /// What the run should still say: settings notices, failed cleanups, an unrecorded history.
    pub notices: Vec<String>,
    /// The response as the checks saw it (`status`, `headers`, `body`), masked; what
    /// `--save-as-test` takes its expectations from.
    pub response: Option<Value>,
    /// The last request as sent and its response, bodies whole, masked; `None` when no
    /// response came back.
    pub exchange: Option<Exchange>,
    /// The api's checks, in the order they ran, masked.
    pub checks: Vec<CheckResult>,
    /// The values the api's `save` took from the response, masked.
    pub saved: Vec<SavedValue>,
    /// The names of the program the call ran in, to label the exchange and checks with.
    pub names: Names,
    /// The action that made the call.
    pub action: ActionId,
}

/// A value a call saved.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct SavedValue {
    /// The var it was saved as.
    pub name: String,
    /// Where it came from, as written: `${response.body.id}`; empty when the api's `save`
    /// does not name it.
    pub from: String,
    /// The value, masked.
    pub value: Value,
}

impl CallReport {
    /// The report of `report`, which `action` in `names`' program made, masked by `cx`'s
    /// redactor; its notices are left for the caller.
    pub(crate) async fn masked(cx: &ActionCx, (names, action): (Names, ActionId), report: ActionReport) -> Self {
        let redactor = &cx.redactor;
        let exchange = match report.exchanges.last() {
            Some(exchange) => Some(redactor.exchange(exchange.clone()).await),
            None => None,
        };
        let mut checks = Vec::with_capacity(report.checks.len());
        for check in report.checks.iter().cloned() {
            checks.push(redactor.checked(check).await);
        }
        let response = match (exchange.as_ref(), report.response.as_deref()) {
            (Some(exchange), _) => Some(vc_executor::received(exchange)),
            (None, Some(response)) => Some(redactor.masked(response.clone()).await),
            (None, None) => None,
        };
        let api = called_api(names.program(), action).map(|api| (names.program(), api));
        let saved = saved(redactor, api, (&report, response.as_ref().unwrap_or(&Value::Null))).await;
        let passed = matches!(report.outcome, ActionOutcome::Passed);
        let text = shown(cx, exchange.as_ref(), &report.outcome).await;
        Self { exit: u8::from(!passed), text, notices: Vec::new(), response, exchange, checks, saved, names, action }
    }
}

/// The api the call `action` sends.
fn called_api(program: &Program, action: ActionId) -> Option<&Api> {
    let found = usize::try_from(action.index()).ok().and_then(|index| program.actions.get(index));
    let Some(ActionKind::Call(api)) = found.map(|found| &found.kind) else { return None };
    program.apis.get(api)
}

/// The values `report` saved, each with the template the api's `save` took it from. A value
/// is masked as the run masks values; one whose template reads a part of `response`, the
/// response masked as an event's is, that is masked (a `redact.fields` field, a masked header)
/// is masked whole, as it may be that part.
async fn saved(
    redactor: &Redactor,
    api: Option<(&Program, &Api)>,
    (report, response): (&ActionReport, &Value),
) -> Vec<SavedValue> {
    let mut saved = Vec::with_capacity(report.saved.len());
    for (name, value) in &report.saved {
        let template = api.and_then(|(program, api)| save_of(program, api, name).map(|value| (program, value)));
        let from = template.map(|(program, value)| Shown { program, vars: &[] }.value(value)).unwrap_or_default();
        let value = if template.is_some_and(|(_, value)| reads_masked(value, response)) {
            Value::String(MASKED.to_owned())
        } else {
            redactor.masked(value.clone()).await
        };
        saved.push(SavedValue { name: name.clone(), from, value });
    }
    saved
}

/// The template the api's `save` takes `name` from.
fn save_of<'p>(program: &Program, api: &'p Api, name: &str) -> Option<&'p TemplateValue> {
    let found = api.save.values.iter().find(|(var, _)| program.str(var.sym()) == Some(name));
    found.map(|(_, value)| value)
}

/// Whether `value` reads a part of `response`, the masked response, that is masked, or one
/// the masked response no longer has (a field inside a field masked whole): failing closed.
fn reads_masked(value: &TemplateValue, response: &Value) -> bool {
    value.references().into_iter().filter(|reference| reference.ns == Namespace::Response).any(|reference| {
        let found = vc_template::select("response", response, &reference.path.segments);
        found.ok().is_none_or(|found| found.to_string().contains(MASKED))
    })
}

/// The status line, the body of `exchange` (masked already), and the failure, masked with the
/// rules, when the expect did not hold.
async fn shown(cx: &ActionCx, exchange: Option<&Exchange>, outcome: &ActionOutcome) -> String {
    let passed = matches!(outcome, ActionOutcome::Passed);
    let mut out = String::new();
    if let Some(exchange) = exchange {
        let verdict = if passed { "api expect passed" } else { "api expect failed" };
        let millis = exchange.timing.total.as_millis();
        let _ = writeln!(out, "{} · {millis} ms · {verdict}", exchange.response.status);
        out.push_str(&body(&exchange.response.body));
    }
    if let ActionOutcome::Failed(failure) = outcome {
        let _ = writeln!(out, "✗ {}", cx.redactor.masked_text(&failure.message).await);
    }
    out
}

/// The masked body as indented JSON when it is JSON, else as text, with a closing newline.
fn body(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return String::new();
    }
    let text = serde_json::from_slice::<Value>(bytes).map_or_else(
        |_| String::from_utf8_lossy(bytes).into_owned(),
        |json| serde_json::to_string_pretty(&json).unwrap_or_default(),
    );
    format!("{}\n", text.trim_end())
}

#[cfg(test)]
#[path = "call_report_tests.rs"]
mod tests;
