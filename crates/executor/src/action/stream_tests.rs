use std::sync::Arc;

use serde_json::json;
use vc_model::{CheckKind, OwnerKey, Protocol, StatusPattern, StreamSettings};
use vc_testkit::{FakeTransport, RecordingTransport};

use crate::fixture::Fixture;
use crate::{ActionExecutors, ActionRequest, Frame, HostLimits, PendingCleanup, PipelineParts};

#[tokio::test]
async fn a_call_of_a_stream_api_fails_with_a_plain_message_and_sends_nothing() {
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
        let message = format!("`{kind}` calls are not supported yet");
        assert_eq!(failure, Some((None, message)));
        assert!(recordings.take().is_empty(), "{kind}: nothing is sent");
    }
}

/// An api whose stream opens on the WebSocket `101`, called through the `http` runtime until
/// the stream runtime lands: its default status check and its cleanup follow that status (D386).
#[tokio::test]
async fn a_websocket_api_succeeds_on_101_so_its_default_status_and_its_cleanup_follow_it() {
    for (status, passed) in [(101, true), (200, false)] {
        let mut fixture = Fixture::new();
        let delete = fixture.call("/rooms");
        let open = fixture.creating("/chat", delete);
        fixture.api_mut(open).stream = Some(StreamSettings { opens_on: StatusPattern::Code(101), reconnect: false });
        let fake = FakeTransport::scripted(vec![Ok(vc_testkit::status(status))]);
        let executors = ActionExecutors::standard(PipelineParts::new(Arc::new(fake), HostLimits::spawn(8)));
        let cx = fixture.cx(executors);
        let request =
            ActionRequest { action: open, owner: OwnerKey::Run, cx: Arc::clone(&cx), frame: Frame::default() };
        let report = cx.executors.execute(request).await;
        let check = &report.checks[0];
        assert_eq!((check.kind, check.passed, check.expected.clone()), (CheckKind::Status, passed, json!([101])));
        let cleanups = if passed { vec![PendingCleanup::new(delete)] } else { vec![] };
        assert_eq!(report.cleanups, cleanups, "{status}: only the stream's own status registers the cleanup");
    }
}
