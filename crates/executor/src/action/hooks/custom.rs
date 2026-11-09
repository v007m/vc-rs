//! `expect.custom`: functions that judge a response (D172). Each gets `{response, vars}` and
//! holds when it returns `true` or `{pass: true}`; a string, or `{pass: false, message}`, is the
//! failure message.

use serde_json::{Map, Value};
use vc_diagnostics::Code;
use vc_interfaces::HookError;
use vc_model::{Check, CheckKind, CheckResult, HookRef, Segment};

use super::HookBridge;

/// Runs each custom check among `checks` with `input` and settles its result in `results`, which
/// [`crate::check::evaluate`] gave in the same order.
pub async fn judge(bridge: &HookBridge, checks: &[Check], (results, input): (&mut [CheckResult], &Value)) {
    let custom = checks.iter().zip(results.iter_mut()).filter(|(check, _)| check.kind == CheckKind::Custom);
    for (check, result) in custom {
        let returned = match hook_of(check) {
            Ok(hook) => bridge.call(&hook, input.clone()).await,
            Err(error) => Err(error),
        };
        settle(result, returned);
    }
}

/// The function a custom check names, as the compiler wrote it: one `file#export` key.
fn hook_of(check: &Check) -> Result<HookRef, HookError> {
    let named = check.target.as_ref().and_then(|path| path.segments.first());
    let Some(Segment::Key(name)) = named else {
        return Err(HookError::NotFound { hook: "a custom check without a function".to_owned() });
    };
    let Some((file, export)) = name.rsplit_once('#') else {
        return Err(HookError::NotFound { hook: name.clone() });
    };
    Ok(HookRef { file: file.to_owned(), export: export.to_owned() })
}

/// `result` as the function's return, or its error, decides it.
fn settle(result: &mut CheckResult, returned: Result<Value, HookError>) {
    let (error, code) = match &returned {
        Ok(value) => (refusal(value), Code::V5103),
        Err(error) => (Some(error.to_string()), code_of(error)),
    };
    result.passed = error.is_none();
    result.expected = Value::Bool(true);
    result.actual = returned.ok();
    result.diff = None;
    result.code = error.is_some().then(|| code.to_string());
    result.error = error;
}

/// V5103 when the function threw, as a check that did not hold; the runner's code otherwise.
const fn code_of(error: &HookError) -> Code {
    if matches!(error, HookError::Threw { .. }) { Code::V5103 } else { error.code() }
}

/// Why a returned value fails the check; `None` when it holds.
fn refusal(value: &Value) -> Option<String> {
    match value {
        Value::Bool(true) => None,
        Value::String(message) => Some(message.clone()),
        Value::Object(fields) => passed(fields),
        Value::Bool(false) | Value::Null | Value::Number(_) | Value::Array(_) => {
            Some(format!("the function returned {}", vc_template::preview(value)))
        }
    }
}

/// `{pass, message}`: `None` when `pass` is `true`, else the message.
fn passed(fields: &Map<String, Value>) -> Option<String> {
    if fields.get("pass") == Some(&Value::Bool(true)) {
        return None;
    }
    let message = fields.get("message").filter(|message| !message.is_null());
    Some(message.map_or_else(|| "the function returned `pass: false`".to_owned(), vc_template::to_text))
}
