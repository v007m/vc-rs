use std::sync::Arc;

use serde_json::Value;
use serde_json::json;
use vc_model::{
    Action, ActionId, Api, Check, CheckKind, Exchange, Failure, Matcher, Origin, OwnerKey, Protocol, RenderedRequest,
    StatusPattern,
};
use vc_testkit::{FakeTransport, RecordingTransport};

use super::super::protocol::{ProtocolRuntime, ProtocolRuntimes};
use super::super::request;
use crate::fixture::{self, Fixture};
use crate::{
    ActionExecutors, ActionKindTag, ActionRequest, CallExecutor, Frame, HostLimits, PendingCleanup, PipelineParts,
    Render,
};

/// A `websocket` runtime that sends its handshake as an `http` request, reading no stream, so
/// the status, cleanup and save of a stream api's opening can be checked on their own.
#[derive(Debug)]
struct HandshakeOnly;

impl ProtocolRuntime for HandshakeOnly {
    fn protocol(&self) -> Protocol {
        Protocol::Websocket
    }

    fn request(&self, render: &Render<'_>, api: &Api, call: (ActionId, &Action)) -> Result<RenderedRequest, Failure> {
        request::rendered(render, api, call)
    }

    fn response(&self, exchange: &Exchange) -> Value {
        request::received(exchange)
    }
}

/// Calls through `fake` with [`HandshakeOnly`] as the `websocket` runtime.
fn handshakes(fake: FakeTransport) -> ActionExecutors {
    let parts = PipelineParts::new(Arc::new(fake), HostLimits::spawn(8));
    let runtimes = ProtocolRuntimes::standard().with(Arc::new(HandshakeOnly));
    ActionExecutors::new().with(ActionKindTag::Call, Arc::new(CallExecutor::new(parts).with_runtimes(runtimes)))
}

#[tokio::test]
async fn a_stream_call_without_a_stream_transport_fails_with_a_plain_message_and_sends_nothing() {
    for (protocol, kind) in [(Protocol::Sse, "sse"), (Protocol::Websocket, "websocket")] {
        let mut fixture = Fixture::new();
        let call = fixture.call("/events");
        fixture.api_mut(call).protocol = protocol;
        let fake = FakeTransport::scripted(vec![Ok(vc_testkit::status(200))]);
        let (transport, recordings) = RecordingTransport::new(Arc::new(fake));
        let executors = ActionExecutors::standard(PipelineParts::new(Arc::new(transport), HostLimits::spawn(8)));
        let cx = fixture.cx(executors);
        let request =
            ActionRequest { action: call, owner: OwnerKey::Run, cx: Arc::clone(&cx), frame: Frame::default() };
        let report = cx.executors.execute(request).await;
        let failure = report.outcome.failure().map(|failure| (failure.code.clone(), failure.message.clone()));
        let message = format!("`{kind}` calls need a stream transport, and none is set");
        assert_eq!(failure, Some((None, message)));
        assert!(recordings.take().is_empty(), "{kind}: nothing is sent");
    }
}

/// A `websocket` api, its handshake sent as an `http` request: its default status check and its
/// cleanup follow its `101` (D386).
#[tokio::test]
async fn a_websocket_api_succeeds_on_101_so_its_default_status_and_its_cleanup_follow_it() {
    for (status, passed) in [(101, true), (200, false)] {
        let mut fixture = Fixture::new();
        let delete = fixture.call("/rooms");
        let open = fixture.creating("/chat", delete);
        fixture.api_mut(open).protocol = Protocol::Websocket;
        let cx = fixture.cx(handshakes(FakeTransport::scripted(vec![Ok(vc_testkit::status(status))])));
        let request =
            ActionRequest { action: open, owner: OwnerKey::Run, cx: Arc::clone(&cx), frame: Frame::default() };
        let report = cx.executors.execute(request).await;
        let check = &report.checks[0];
        assert_eq!((check.kind, check.passed, check.expected.clone()), (CheckKind::Status, passed, json!([101])));
        let cleanups = if passed { vec![PendingCleanup::new(delete)] } else { vec![] };
        assert_eq!(report.cleanups, cleanups, "{status}: only the stream's own status registers the cleanup");
    }
}

/// An api's cleanup registers on its success status (`2xx`; `101` for a `websocket`), whatever its
/// `expect.status` says (D71, D386, D387): a listed `400` passes with no cleanup, and an expected
/// status that the response missed fails the call with its cleanup still registered. The save
/// keeps its default `saveOn`, the expected status: a `websocket` api expecting `200` saves on a
/// `200` and not on a `101`.
#[tokio::test]
async fn a_cleanup_registers_on_the_success_status_whatever_the_expected_status_says() {
    let cases = [
        (Protocol::Http, vec![201, 400], 400, (true, true, false)),
        (Protocol::Http, vec![201], 200, (false, false, true)),
        (Protocol::Websocket, vec![200], 101, (false, false, true)),
        (Protocol::Websocket, vec![200], 200, (true, true, false)),
    ];
    for (protocol, expected, status, (passed, saves, registered)) in cases {
        let mut fixture = Fixture::new();
        let delete = fixture.call("/rooms");
        let open = fixture.creating("/chat", delete);
        let api_saves = fixture.vars(&[("name", "${response.status}")]);
        let api = fixture.api_mut(open);
        let matcher = Matcher::Status(expected.into_iter().map(StatusPattern::Code).collect());
        let check = Check {
            kind: CheckKind::Status,
            target: None,
            matcher,
            origin: Origin::Api(api.id),
            span: fixture::span(),
        };
        (api.protocol, api.save.values, api.save_on) = (protocol, api_saves, check.matcher.clone());
        api.expect.checks = vec![check.clone()];
        fixture.action_mut(open).expect.checks = vec![check];
        let cx = fixture.cx(handshakes(FakeTransport::scripted(vec![Ok(vc_testkit::status(status))])));
        let request =
            ActionRequest { action: open, owner: OwnerKey::Run, cx: Arc::clone(&cx), frame: Frame::default() };
        let report = cx.executors.execute(request).await;
        let saved: Vec<_> = report.saved.keys().cloned().collect();
        let names = if saves { vec!["name".to_owned()] } else { vec![] };
        let cleanups = if registered { vec![PendingCleanup::new(delete)] } else { vec![] };
        let outcome = (report.outcome.failure().is_none(), saved, report.cleanups);
        assert_eq!(outcome, (passed, names, cleanups), "{protocol:?} answered {status}");
    }
}
