use std::sync::Arc;

use vc_diagnostics::Code;
use vc_interfaces::{Transport, TransportError};
use vc_model::{
    ActionId, ActionKind, ActionOutcome, Failure, HookRef, Namespace, OwnerKey, Part, Path, Reference, SourceFileId,
    SourceSpan, Template,
};
use vc_testkit::{FakeTransport, status};

use super::{ActionCx, ActionExecutors, ActionKindTag, ActionRequest, Scope};
use crate::fixture::{Fixture, Server};
use crate::{ActionReport, ExecutorError, HostLimits, PendingCleanup, PipelineParts};

fn standard(transport: Arc<dyn Transport>) -> ActionExecutors {
    ActionExecutors::standard(PipelineParts::new(transport, HostLimits::spawn(8)))
}

async fn execute(fixture: Fixture, executors: ActionExecutors, action: ActionId) -> ActionReport {
    let cx = Arc::new(ActionCx { program: Arc::new(fixture.program), executors: Arc::new(executors) });
    let request = ActionRequest { action, owner: OwnerKey::Run, cx: Arc::clone(&cx), scope: Scope::default() };
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
    assert_eq!(report.cleanups, vec![PendingCleanup { action: delete }]);
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
        Some((Some(create), "expected a 2xx status, got 500 Internal Server Error"))
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
async fn a_template_with_references_does_not_render_yet() {
    let mut fixture = Fixture::new();
    let get = fixture.call("/users");
    let span = SourceSpan::new(SourceFileId::new(0), vc_model::Pos::new(1, 1, 0), vc_model::Pos::new(1, 1, 0));
    let reference =
        Reference { ns: Namespace::Var, path: Path::default(), args: vec![], pipes: vec![], span, root_span: span };
    for api in fixture.program.apis.values_mut() {
        api.path = Template { parts: vec![Part::Literal("/users/".into()), Part::Ref(reference.clone())] };
    }
    let fake = FakeTransport::scripted(vec![Ok(status(200))]);
    let report = execute(fixture, standard(Arc::new(fake.clone())), get).await;
    assert_eq!(code(&report), Some(Code::V5111.to_string().as_str()));
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
    assert_eq!(report.cleanups, vec![PendingCleanup { action: delete }], "cleanups so far are kept");
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
