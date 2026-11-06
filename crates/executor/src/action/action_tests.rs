use std::sync::Arc;

use vc_diagnostics::Code;
use vc_interfaces::{Transport, TransportError};
use vc_model::{ActionId, ActionKind, ActionOutcome, Event, Failure, HookRef, OwnerKey, RedactRule};
use vc_testkit::FakeTransport;

use super::{ActionExecutors, ActionKindTag, ActionRequest};
use crate::fixture::{Fixture, Server};
use crate::{ActionReport, ExecutorError, Frame, HostLimits, MASKED, MaskRules, PendingCleanup, PipelineParts};

fn standard(transport: Arc<dyn Transport>) -> ActionExecutors {
    ActionExecutors::standard(PipelineParts::new(transport, HostLimits::spawn(8)))
}

async fn execute(fixture: Fixture, executors: ActionExecutors, action: ActionId) -> ActionReport {
    let cx = fixture.cx(executors);
    let request = ActionRequest { action, owner: OwnerKey::Run, cx: Arc::clone(&cx), frame: Frame::default() };
    cx.executors.execute(request).await
}

fn failure(report: &ActionReport) -> Option<&Failure> {
    report.outcome.failure()
}

fn code(report: &ActionReport) -> Option<&str> {
    failure(report).and_then(|failure| failure.code.as_deref())
}

#[tokio::test]
async fn a_2xx_call_passes_and_registers_the_api_cleanup() {
    let mut fixture = Fixture::new();
    let delete = fixture.call("/users/1");
    let create = fixture.creating("/users", delete);
    let (server, sent) = Server::new();
    let report = execute(fixture, standard(server), create).await;
    assert_eq!(report.outcome, ActionOutcome::Passed);
    assert_eq!(report.cleanups, vec![PendingCleanup::new(delete)]);
    assert_eq!(sent.counts().get("/users"), Some(&1));
}

#[tokio::test]
async fn a_kept_call_registers_no_cleanup() {
    let mut fixture = Fixture::new();
    let delete = fixture.call("/users/1");
    let create = fixture.creating("/users", delete);
    fixture.program.actions[usize::try_from(create.index()).unwrap()].keep = true;
    let (server, _) = Server::new();
    let report = execute(fixture, standard(server), create).await;
    assert_eq!((report.outcome, report.cleanups), (ActionOutcome::Passed, vec![]));
}

#[tokio::test]
async fn a_non_2xx_status_fails_the_call_without_a_cleanup() {
    let mut fixture = Fixture::new();
    let delete = fixture.call("/users/1");
    let create = fixture.creating("/fail", delete);
    let (server, _) = Server::new();
    let report = execute(fixture, standard(server), create).await;
    assert_eq!(code(&report), Some(Code::V5101.to_string().as_str()));
    assert_eq!(
        failure(&report).map(|failure| (failure.action, failure.message.as_str())),
        Some((Some(create), "status: expected [\"2xx\"], got 500"))
    );
    assert_eq!(report.cleanups, vec![]);
}

#[tokio::test]
async fn a_transport_error_fails_the_call_with_its_code() {
    let mut fixture = Fixture::new();
    let get = fixture.call("/users");
    let refused = TransportError::Connect { host: "api.test".into(), reason: "refused".into() };
    let report = execute(fixture, standard(Arc::new(FakeTransport::scripted(vec![Err(refused)]))), get).await;
    assert_eq!(code(&report), Some(Code::V5105.to_string().as_str()));
}

#[tokio::test]
async fn a_var_that_is_not_set_fails_the_call_before_it_is_sent() {
    let mut fixture = Fixture::new();
    let get = fixture.call("/users");
    fixture.api_mut(get).path = fixture.template("/users/${var.userId}");
    let fake = FakeTransport::scripted(vec![Ok(vc_testkit::status(200))]);
    let report = execute(fixture, standard(Arc::new(fake.clone())), get).await;
    assert_eq!(code(&report), Some(Code::V5111.to_string().as_str()));
    assert_eq!(failure(&report).map(|failure| failure.message.as_str()), Some("`var.userId` is not set"));
    assert_eq!(fake.calls(), 0);
}

#[tokio::test]
async fn a_url_that_does_not_parse_fails_the_call() {
    let mut fixture = Fixture::new();
    let get = fixture.call("/a path with spaces");
    let report = execute(fixture, standard(Arc::new(FakeTransport::echo())), get).await;
    assert_eq!(code(&report), Some(Code::V5111.to_string().as_str()));
}

#[tokio::test]
async fn test_reuse_runs_the_reused_steps_until_one_fails() {
    let mut fixture = Fixture::new();
    let delete = fixture.call("/users/1");
    let steps = vec![fixture.creating("/users", delete), fixture.call("/fail"), fixture.call("/never")];
    let reused = fixture.test(steps, None);
    let reuse = fixture.action(ActionKind::Test(reused));
    let (server, sent) = Server::new();
    let report = execute(fixture, standard(server), reuse).await;
    assert!(failure(&report).is_some());
    assert_eq!(report.cleanups, vec![PendingCleanup::new(delete)], "cleanups so far are kept");
    let counts = sent.counts();
    assert_eq!((counts.get("/users"), counts.get("/fail"), counts.get("/never")), (Some(&1), Some(&1), None));
}

#[tokio::test]
async fn an_action_without_an_executor_fails() {
    let mut fixture = Fixture::new();
    let hook = fixture.action(ActionKind::Function(HookRef { file: "hooks.ts".into(), export: "seed".into() }));
    let report = execute(fixture, ActionExecutors::new(), hook).await;
    assert_eq!(failure(&report).map(|failure| failure.action), Some(Some(hook)));
}

#[tokio::test]
async fn the_check_follows_cleanups_and_reuse_to_every_reachable_action() {
    let mut fixture = Fixture::new();
    let hook = fixture.action(ActionKind::Function(HookRef { file: "hooks.ts".into(), export: "seed".into() }));
    let create = fixture.creating("/users", hook);
    let reused = fixture.test(vec![create], None);
    let reuse = fixture.action(ActionKind::Test(reused));
    let executors = standard(Arc::new(FakeTransport::echo()));
    assert_eq!(executors.check(&fixture.program, vec![]), Ok(()));
    let rejected = Err(ExecutorError::NoExecutor { action: hook, kind: ActionKindTag::Function });
    assert_eq!(executors.check(&fixture.program, vec![reuse, reuse]), rejected);
    let missing = ActionId::new(99);
    assert_eq!(executors.check(&fixture.program, vec![missing]), Err(ExecutorError::UnknownAction(missing)));
    let fine = fixture.call("/users");
    assert_eq!(executors.check(&fixture.program, vec![fine, fine]), Ok(()));
}

#[test]
fn kind_tags_name_their_keys() {
    let tags = [
        ActionKindTag::Call,
        ActionKindTag::Test,
        ActionKindTag::Function,
        ActionKindTag::Command,
        ActionKindTag::Inline,
    ];
    let names: Vec<_> = tags.iter().map(ToString::to_string).collect();
    assert_eq!(names, ["call", "test", "function", "command", "inline"]);
    assert_eq!(
        format!("{:?}", ActionExecutors::new().with(ActionKindTag::Test, Arc::new(super::TestExecutor))),
        "{Test}"
    );
}

#[tokio::test]
async fn mask_rules_make_the_config_rules_and_keep_the_secrets_told() {
    let mut fixture = Fixture::new();
    fixture.program.config.redact_rules = vec![RedactRule::Regex("pw-\\d".to_owned())];
    let cx = Arc::into_inner(fixture.cx(ActionExecutors::new())).unwrap();
    cx.redactor.hide_values(["s3cr3t-key"]);
    let event = |message: &str| {
        let outcome = ActionOutcome::Failed(Failure::new(message));
        Event::ActionFinished { action: ActionId::new(0), owner: OwnerKey::Run, outcome }
    };
    assert_eq!(cx.redactor.event(event("pw-1 ok")).await, event(&format!("{MASKED} ok")));
    let blind = cx.with_mask_rules(&MaskRules::default());
    assert_eq!(blind.redactor.event(event("ok")).await, event(MASKED));
    assert_eq!(blind.redactor.text("s3cr3t-key"), MASKED);
}
