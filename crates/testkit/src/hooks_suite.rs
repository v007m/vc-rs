use std::collections::BTreeMap;
use std::time::Duration;

use serde_json::{Value, json};
use vc_interfaces::{Budget, HookCall, HookError, HookOutput, HookRunner, Limit, Secret, Secrets};
use vc_model::{HookModule, HookModules, HookRef};

/// The file every [`hook_runner_suite`] call names.
pub const SUITE_HOOKS: &str = "hooks/suite.ts";

/// The JavaScript of [`SUITE_HOOKS`]; each JavaScript line `n` came from TypeScript line `n + 10`.
pub const SUITE_JS: &str = "\
export function echo(input) { return input; }
export async function later(input) { return { doubled: input.n * 2 }; }
export function boom() {
  throw new Error('bad input');
}
export function spin() { for (;;) {} }
export function hog() { const all = []; for (;;) { all.push('x'.repeat(1024)); } }
export function key() { return secret('KEY'); }
export function callable() { return () => 1; }
";

/// Builds the runner under test.
pub type MakeHookRunner<'a> = dyn FnMut() -> Box<dyn HookRunner> + Send + 'a;

/// The contract every [`HookRunner`] meets (D112), over [`SUITE_JS`] at [`SUITE_HOOKS`].
///
/// # Panics
///
/// When the runner breaks the contract.
pub async fn hook_runner_suite(make: &mut MakeHookRunner<'_>) {
    let runner = make();
    let runner = runner.as_ref();
    assert_eq!(value(runner, "echo", json!({"a": [1, "two", null]})).await, Ok(json!({"a": [1, "two", null]})));
    assert_eq!(value(runner, "later", json!({"n": 2})).await, Ok(json!({"doubled": 4})), "a promise is awaited");
    let threw = value(runner, "boom", Value::Null).await;
    assert!(
        matches!(&threw, Err(HookError::Threw { message, line: Some(14) }) if message.contains("bad input")),
        "a throw names its message and TypeScript line: {threw:?}"
    );
    assert_overruns(runner).await;
    assert_eq!(value(runner, "echo", json!(1)).await, Ok(json!(1)), "the runner recovers after an overrun");
    assert_secret_is_read(runner).await;
    let missing = value(runner, "nope", Value::Null).await;
    assert!(matches!(missing, Err(HookError::NotFound { .. })), "a missing export: {missing:?}");
    let callable = value(runner, "callable", Value::Null).await;
    assert!(matches!(callable, Err(HookError::BadValue { .. })), "a function is not JSON: {callable:?}");
}

/// Both budgets stop a call.
async fn assert_overruns(runner: &dyn HookRunner) {
    let tight = Budget::new(Duration::from_millis(50), 64 << 20);
    let spun = runner.call(call("spin", Value::Null).with_budget(tight)).await;
    assert!(matches!(spun, Err(HookError::Overran { limit: Limit::Time, .. })), "time: {spun:?}");
    let small = Budget::new(Duration::from_secs(5), 4 << 20);
    let hogged = runner.call(call("hog", Value::Null).with_budget(small)).await;
    assert!(matches!(hogged, Err(HookError::Overran { limit: Limit::Memory, .. })), "memory: {hogged:?}");
}

/// `secret(name)` returns the value and the output names it.
async fn assert_secret_is_read(runner: &dyn HookRunner) {
    let mut secrets = Secrets::new();
    secrets.insert("KEY", Secret::new("s3cr3t"));
    let read = runner.call(call("key", Value::Null).with_secrets(secrets)).await;
    let expected = HookOutput::new(json!("s3cr3t")).reading(vec!["KEY".to_owned()]);
    assert_eq!(read.as_ref().ok(), Some(&expected), "secret(name): {read:?}");
}

async fn value(runner: &dyn HookRunner, export: &str, input: Value) -> Result<Value, HookError> {
    runner.call(call(export, input)).await.map(|output| output.value)
}

/// A call of `export` in the suite's module.
fn call(export: &str, input: Value) -> HookCall {
    let lines = (1..=u32::try_from(SUITE_JS.lines().count()).unwrap_or(0)).map(|line| line + 10).collect();
    let module = HookModule { js: SUITE_JS.to_owned(), lines };
    let modules = HookModules::new(BTreeMap::from([(SUITE_HOOKS.to_owned(), module)]));
    HookCall::new(modules, HookRef { file: SUITE_HOOKS.to_owned(), export: export.to_owned() }, input)
}
