use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use serde_json::{Value, json};
use vc_interfaces::{HookCall, HookError, HookOutput, HookRunner, Limit};
use vc_model::{ActionId, ActionOutcome, Config, Event, Failure, HookRef, OwnerKey, Program, RedactRule};
use vc_testkit::FakeHooks;

use crate::fixture::Fixture;
use crate::{ActionExecutors, MASKED, MaskRule, MaskRules, Redactor};

const MASK: &str = "hooks/redact.ts#maskCards";

fn rule() -> RedactRule {
    RedactRule::Function(HookRef { file: "hooks/redact.ts".into(), export: "maskCards".into() })
}

fn program() -> Arc<Program> {
    Arc::clone(&Fixture::new().cx(ActionExecutors::default()).program)
}

/// The `function:` rule made with `hooks` as the run's runner (none when `None`).
fn made(hooks: Option<FakeHooks>) -> Arc<dyn MaskRule> {
    let runner = hooks.map(|hooks| Arc::new(hooks) as Arc<dyn HookRunner>);
    MaskRules::standard().with_hooks(runner, &program()).rule(&rule())
}

/// A hook that masks every `4111…` card number it is given, in a text or in a JSON value.
fn cards(call: &HookCall) -> Result<HookOutput, HookError> {
    let masked = |text: &str| text.replace("4111111111111111", "[card]");
    let value = match (&call.input["text"], &call.input["json"]) {
        (Value::String(text), _) => json!(masked(text)),
        (_, json) => serde_json::from_str(&masked(&json.to_string())).unwrap_or(Value::Null),
    };
    Ok(HookOutput::new(value))
}

#[tokio::test]
async fn a_hook_masks_a_text_and_a_json_value_each_given_whole() {
    let calls = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&calls);
    let hooks = FakeHooks::new().on(MASK, move |call| {
        counted.fetch_add(1, Ordering::SeqCst);
        cards(call)
    });
    let rule = made(Some(hooks));
    assert_eq!(rule.text("paid with 4111111111111111".to_owned()).await, "paid with [card]");
    let body = json!({"card": "4111111111111111", "items": [{"note": "4111111111111111"}], "total": 3});
    let masked = rule.value(body).await;
    assert_eq!(masked, json!({"card": "[card]", "items": [{"note": "[card]"}], "total": 3}));
    assert_eq!(calls.load(Ordering::SeqCst), 2, "one call per piece, not per field");
}

#[tokio::test]
async fn a_hook_that_fails_or_returns_the_wrong_kind_masks_the_whole_piece() {
    let threw = |_: &HookCall| Err(HookError::Threw { message: "no".to_owned(), line: Some(3) });
    let overran = |_: &HookCall| Err(HookError::Overran { limit: Limit::Time, line: None });
    for hooks in [FakeHooks::new().on(MASK, threw), FakeHooks::new().on(MASK, overran)] {
        let rule = made(Some(hooks));
        assert_eq!(rule.text("4111111111111111".to_owned()).await, MASKED);
        assert_eq!(rule.value(json!({"card": "4111111111111111"})).await, json!(MASKED));
    }
    let number = made(Some(FakeHooks::new().on(MASK, |_| Ok(HookOutput::new(json!(4))))));
    assert_eq!(number.text("4111111111111111".to_owned()).await, MASKED, "a text must come back a string");
}

#[tokio::test]
async fn without_a_runner_or_the_hook_a_function_rule_masks_everything() {
    for rule in [made(None), made(Some(FakeHooks::new()))] {
        assert_eq!(rule.text("anything".to_owned()).await, MASKED);
        assert_eq!(rule.value(json!({"a": 1})).await, json!(MASKED));
    }
}

#[tokio::test]
async fn the_standard_rules_alone_mask_a_function_rule_whole_and_with_hooks_keeps_the_others() {
    let blind = MaskRules::standard().rule(&rule());
    assert_eq!(blind.text("plain".to_owned()).await, MASKED, "no function maker: fail closed");
    let rules = MaskRules::standard().with_hooks(None, &program());
    let regex = rules.rule(&RedactRule::Regex(r"\d{3}".to_owned()));
    assert_eq!(regex.text("pin 123".to_owned()).await, format!("pin {MASKED}"));
}

#[tokio::test]
async fn the_hook_runs_after_the_built_in_masking_so_it_never_sees_a_secret() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let log = Arc::clone(&seen);
    let hooks = FakeHooks::new().on(MASK, move |call| {
        log.lock().map(|mut seen| seen.push(call.input.to_string())).ok();
        cards(call)
    });
    let runner = Some(Arc::new(hooks) as Arc<dyn HookRunner>);
    let config = Config { redact_rules: vec![rule()], ..Config::default() };
    let redactor =
        Redactor::new(&[]).with_rules(MaskRules::standard().with_hooks(runner, &program()).for_config(&config));
    redactor.hide_values(["s3cr3t-key"]);
    let failure = Failure::new("key s3cr3t-key card 4111111111111111");
    let (action, owner) = (ActionId::new(0), OwnerKey::Run);
    let event = Event::ActionFinished { action, owner, outcome: ActionOutcome::Failed(failure) };
    let Event::ActionFinished { outcome: ActionOutcome::Failed(masked), .. } = redactor.event(event).await else {
        panic!("not a failure")
    };
    assert_eq!(masked.message, format!("key {MASKED} card [card]"));
    let seen = seen.lock().map(|seen| seen.join("\n")).unwrap_or_default();
    assert!(!seen.is_empty() && !seen.contains("s3cr3t-key"), "{seen}");
}
