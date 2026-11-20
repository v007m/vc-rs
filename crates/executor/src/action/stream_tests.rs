use std::sync::Arc;

use vc_model::{OwnerKey, Protocol};
use vc_testkit::{FakeTransport, RecordingTransport};

use crate::fixture::Fixture;
use crate::{ActionExecutors, ActionRequest, Frame, HostLimits, PipelineParts};

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
        let message = format!("a `{kind}` api cannot be called yet: stream calls are not run (#92, #93)");
        assert_eq!(failure, Some((None, message)));
        assert!(recordings.take().is_empty(), "{kind}: nothing is sent");
    }
}
