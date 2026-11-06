use std::sync::Arc;

use serde_json::{Value, json};
use vc_interfaces::{HookCall, HookError, HookOutput, Limit, Transport};
use vc_model::{
    ActionId, ActionKind, Check, CheckKind, CheckResult, HookRef, Matcher, Origin, OwnerKey, Path, Segment,
    TemplateValue,
};
use vc_testkit::{FakeHooks, FakeTransport, Recorded, RecordingTransport};

use super::{HookBridge, judge};
use crate::fixture::{self, Fixture};
use crate::{ActionExecutors, ActionReport, ActionRequest, Frame, HostLimits, PipelineParts};

const FILE: &str = "hooks/sign.ts";

fn hook(export: &str) -> HookRef {
    HookRef { file: FILE.into(), export: export.into() }
}

fn named(export: &str) -> String {
    format!("{FILE}#{export}")
}

/// Runs `action` with `hooks` as the run's runner (none when `None`) against a transport that
/// answers 200, recording what was sent.
async fn run(fixture: Fixture, action: ActionId, hooks: Option<FakeHooks>) -> (ActionReport, Vec<Recorded>) {
    let fake = FakeTransport::scripted(vec![Ok(vc_testkit::status(200))]);
    let (transport, recordings) = RecordingTransport::new(Arc::new(fake));
    let parts = PipelineParts::new(Arc::new(transport) as Arc<dyn Transport>, HostLimits::spawn(8));
    let parts = match hooks {
        Some(hooks) => parts.with_hook_runner(Arc::new(hooks)),
        None => parts,
    };
    let cx = fixture.cx(ActionExecutors::standard(parts));
    let request = ActionRequest { action, owner: OwnerKey::Run, cx: Arc::clone(&cx), frame: Frame::default() };
    let report = cx.executors.execute(request).await;
    (report, recordings.take())
}

fn code(report: &ActionReport) -> Option<String> {
    report.outcome.failure().and_then(|failure| failure.code.clone())
}

fn message(report: &ActionReport) -> String {
    report.outcome.failure().map(|failure| failure.message.clone()).unwrap_or_default()
}

/// A hook that returns the request with `change` made to it.
fn changing(change: fn(&mut Value)) -> impl Fn(&HookCall) -> Result<HookOutput, HookError> + Send + Sync + 'static {
    move |call: &HookCall| {
        let mut request = call.input["request"].clone();
        change(&mut request);
        Ok(HookOutput::new(request))
    }
}

#[tokio::test]
async fn before_request_hooks_change_the_request_the_api_file_s_first() {
    let mut fixture = Fixture::new();
    let call = fixture.call("/users");
    fixture.group_mut().hooks.before_request = vec![hook("group")];
    fixture.api_mut(call).hooks.before_request = vec![hook("sign")];
    let hooks = FakeHooks::new()
        .on(&named("group"), changing(|request| request["headers"]["x-order"] = json!("group")))
        .on(&named("sign"), |call: &HookCall| {
            let mut request = call.input["request"].clone();
            request["headers"]["x-signature"] = json!(format!("{}:{}", request["path"], request["headers"]["x-order"]));
            request["query"]["page"] = json!(["1", "2"]);
            Ok(HookOutput::new(request))
        });
    let (report, sent) = run(fixture, call, Some(hooks)).await;
    assert_eq!(code(&report), None, "{report:?}");
    let request = &sent[0].request;
    assert_eq!(request.headers["x-signature"], "\"/users\":\"group\"");
    assert_eq!(request.uri.to_string(), "http://api.test/users?page=1&page=2");
}

#[tokio::test]
async fn a_hook_that_returns_nothing_sends_the_request_as_it_is() {
    let mut fixture = Fixture::new();
    let call = fixture.call("/users");
    let query = fixture.templates(&[("a", "b c")]);
    fixture.action_mut(call).query = query;
    fixture.api_mut(call).hooks.before_request = vec![hook("look")];
    let hooks = FakeHooks::new().on(&named("look"), |call: &HookCall| {
        assert_eq!(call.input["request"]["query"], json!({"a": "b c"}));
        Ok(HookOutput::new(Value::Null))
    });
    let (report, sent) = run(fixture, call, Some(hooks)).await;
    assert_eq!(code(&report), None, "{report:?}");
    assert_eq!(sent[0].request.uri.query(), Some("a=b%20c"));
}

#[tokio::test]
async fn a_hook_that_throws_or_returns_no_request_fails_the_call_unsent() {
    for (script, wanted) in [
        (Err(HookError::Threw { message: "Error: no key".into(), line: Some(3) }), "threw: Error: no key (line 3)"),
        (Ok(HookOutput::new(json!(5))), "returned a number, not a request"),
    ] {
        let mut fixture = Fixture::new();
        let call = fixture.call("/users");
        fixture.api_mut(call).hooks.before_request = vec![hook("sign")];
        let hooks = FakeHooks::new().on(&named("sign"), move |_: &HookCall| script.clone());
        let (report, sent) = run(fixture, call, Some(hooks)).await;
        assert_eq!(code(&report).as_deref(), Some("V5115"));
        assert!(message(&report).starts_with("`hooks/sign.ts#sign`: ") && message(&report).contains(wanted));
        assert!(sent.is_empty(), "a refused attempt is not sent or retried");
    }
}

#[tokio::test]
async fn without_a_runner_any_hook_fails_with_v5115() {
    let mut fixture = Fixture::new();
    let call = fixture.call("/users");
    fixture.api_mut(call).hooks.before_request = vec![hook("sign")];
    let (report, sent) = run(fixture, call, None).await;
    assert_eq!(code(&report).as_deref(), Some("V5115"));
    assert!(message(&report).contains("no hook runner"));
    assert_eq!(sent.len(), 0, "nothing is sent");
}

#[tokio::test]
async fn after_response_hooks_see_the_exchange_and_an_overrun_is_v5116() {
    let mut fixture = Fixture::new();
    let call = fixture.call("/users");
    fixture.api_mut(call).hooks.after_response = vec![hook("log"), hook("spin")];
    let hooks = FakeHooks::new()
        .on(&named("log"), |call: &HookCall| {
            assert_eq!(
                (&call.input["response"]["status"], &call.input["request"]["method"]),
                (&json!(200), &json!("GET"))
            );
            Ok(HookOutput::new(json!("ignored")))
        })
        .on(&named("spin"), |_: &HookCall| Err(HookError::Overran { limit: Limit::Time, line: Some(9) }));
    let (report, sent) = run(fixture, call, Some(hooks)).await;
    assert_eq!(sent.len(), 1);
    assert_eq!(code(&report).as_deref(), Some("V5116"));
    assert!(message(&report).contains("time budget") && message(&report).contains("(line 9)"));
}

/// A hook that returns `value`.
fn returns(value: Value) -> impl Fn(&HookCall) -> Result<HookOutput, HookError> + Send + Sync + 'static {
    move |_: &HookCall| Ok(HookOutput::new(value.clone()))
}

fn custom(export: &str) -> Check {
    let target = Some(Path { segments: vec![Segment::Key(named(export))] });
    let matcher = Matcher::Equals(TemplateValue::Bool(true));
    Check { kind: CheckKind::Custom, target, matcher, origin: Origin::Action(ActionId::new(0)), span: fixture::span() }
}

fn unjudged(check: &Check) -> CheckResult {
    CheckResult {
        kind: check.kind,
        target: check.target.clone(),
        origin: check.origin,
        span: check.span,
        passed: false,
        expected: Value::Null,
        actual: None,
        diff: None,
        error: Some("not run".into()),
        code: Some("V5103".into()),
    }
}

#[tokio::test]
async fn a_custom_check_holds_on_true_or_pass_and_fails_with_the_message_it_returns() {
    let hooks = FakeHooks::new()
        .on(&named("yes"), returns(json!(true)))
        .on(&named("pass"), returns(json!({"pass": true})))
        .on(&named("says"), returns(json!("too short")))
        .on(&named("refuses"), returns(json!({"pass": false, "message": "unsorted"})))
        .on(&named("no"), returns(json!(false)))
        .on(&named("throws"), |_: &HookCall| Err(HookError::Threw { message: "Error: x".into(), line: None }))
        .on(&named("spins"), |_: &HookCall| Err(HookError::Overran { limit: Limit::Memory, line: None }));
    let exports = ["yes", "pass", "says", "refuses", "no", "throws", "spins", "missing"];
    let mut checks: Vec<Check> = exports.iter().map(|export| custom(export)).collect();
    checks.insert(1, Check { kind: CheckKind::Status, ..custom("yes") });
    let mut results: Vec<CheckResult> = checks.iter().map(unjudged).collect();
    let bridge = HookBridge::new(Some(Arc::new(hooks)), &Fixture::new().program);
    judge(&bridge, &checks, (results.as_mut_slice(), &json!({}))).await;
    let got: Vec<_> =
        results.iter().map(|result| (result.passed, result.code.as_deref(), result.error.as_deref())).collect();
    assert_eq!(got[..3], [(true, None, None), (false, Some("V5103"), Some("not run")), (true, None, None)]);
    assert_eq!(
        got[3..6],
        [
            (false, Some("V5103"), Some("too short")),
            (false, Some("V5103"), Some("unsorted")),
            (false, Some("V5103"), Some("the function returned false")),
        ]
    );
    let codes: Vec<_> = got[6..].iter().map(|(_, code, _)| *code).collect();
    assert_eq!(codes, [Some("V5103"), Some("V5116"), Some("V5115")]);
}

#[tokio::test]
async fn a_call_runs_its_custom_checks_with_the_response_and_vars() {
    let mut fixture = Fixture::new();
    fixture.program.config.vars.values = fixture.vars(&[("userId", "u-1")]);
    let call = fixture.call("/users");
    fixture.action_mut(call).expect.checks = vec![custom("found"), custom("strict")];
    let hooks = FakeHooks::new()
        .on(&named("found"), |call: &HookCall| {
            let found = call.input["response"]["status"] == 200 && call.input["vars"]["userId"] == "u-1";
            Ok(HookOutput::new(json!(found)))
        })
        .on(&named("strict"), |_: &HookCall| Ok(HookOutput::new(json!("needs an id"))));
    let (report, _) = run(fixture, call, Some(hooks)).await;
    let passed: Vec<_> = report.checks.iter().map(|result| result.passed).collect();
    assert_eq!(passed, [true, true, false], "the default status check, then the two functions");
    assert_eq!(code(&report).as_deref(), Some("V5103"));
    assert!(message(&report).ends_with(": needs an id"), "{}", message(&report));
}

#[tokio::test]
async fn a_function_action_saves_the_fields_it_returns() {
    let mut fixture = Fixture::new();
    fixture.program.config.vars.values = fixture.vars(&[("userId", "u-1")]);
    let seed = fixture.action(ActionKind::Function(hook("seed")));
    let hooks = FakeHooks::new()
        .on(&named("seed"), |call: &HookCall| {
            Ok(HookOutput::new(json!({"token": call.input["vars"]["userId"], "n": 2})))
        })
        .on(&named("count"), |call: &HookCall| Ok(HookOutput::new(json!(call.input["run"]["profiles"].is_array()))));
    let (report, sent) = run(fixture, seed, Some(hooks.clone())).await;
    assert_eq!((code(&report), Value::Object(report.saved), sent.len()), (None, json!({"token": "u-1", "n": 2}), 0));
    let mut fixture = Fixture::new();
    let count = fixture.action(ActionKind::Function(hook("count")));
    let (report, _) = run(fixture, count, Some(hooks)).await;
    assert_eq!((code(&report), report.saved.len()), (None, 0), "a value that is not an object saves nothing");
}

#[tokio::test]
async fn a_function_action_fails_with_the_runner_s_code_or_without_a_runner() {
    let mut fixture = Fixture::new();
    let seed = fixture.action(ActionKind::Function(hook("seed")));
    let (report, _) = run(fixture, seed, None).await;
    assert_eq!(code(&report).as_deref(), Some("V5115"));
    assert_eq!(report.outcome.failure().and_then(|failure| failure.action), Some(seed));
    let mut fixture = Fixture::new();
    let seed = fixture.action(ActionKind::Function(hook("seed")));
    let hooks =
        FakeHooks::new().on(&named("seed"), |_: &HookCall| Err(HookError::Overran { limit: Limit::Time, line: None }));
    let (report, _) = run(fixture, seed, Some(hooks)).await;
    assert_eq!(code(&report).as_deref(), Some("V5116"));
}

#[tokio::test]
async fn a_name_declared_in_provides_and_not_saved_is_v5109() {
    let mut fixture = Fixture::new();
    let seed = fixture.action(ActionKind::Function(hook("seed")));
    fixture.action_mut(seed).provides = vec![fixture.var("token"), fixture.var("email"), fixture.var("userId")];
    let hooks = FakeHooks::new().on(&named("seed"), |_: &HookCall| Ok(HookOutput::new(json!({"token": "t"}))));
    let (report, _) = run(fixture, seed, Some(hooks)).await;
    assert_eq!(code(&report).as_deref(), Some("V5109"));
    assert_eq!(message(&report), "declared in `provides` but not saved: `email`, `userId`");
    assert_eq!(report.outcome.failure().and_then(|failure| failure.action), Some(seed));
}

#[tokio::test]
async fn a_reused_test_must_save_what_the_reuse_declares() {
    let mut fixture = Fixture::new();
    let call = fixture.call("/users");
    let test = fixture.test(vec![call], None);
    let reuse = fixture.action(ActionKind::Test(test));
    fixture.action_mut(reuse).provides = vec![fixture.var("userId")];
    let (report, sent) = run(fixture, reuse, None).await;
    assert_eq!((code(&report).as_deref(), sent.len()), (Some("V5109"), 1));
    assert_eq!(report.outcome.failure().and_then(|failure| failure.action), Some(reuse));
}
