use std::sync::Arc;
use std::time::Duration;

use http::{Method, StatusCode, Uri};
use tower::ServiceExt;
use vc_interfaces::{SendContext, StreamMode, StreamTransport};
use vc_model::RenderedRequest;
use vc_testkit::{FakeStreamTransport, FakeTransport, StreamAnswer, status};

use super::{Call, Delivery, HostLimits, HttpRetryPolicy, Opening, PipelineParts};

const SECOND: Duration = Duration::from_secs(1);

fn parts(per_host: usize, attempts: u32) -> PipelineParts {
    PipelineParts {
        retry: Arc::new(HttpRetryPolicy { attempts, ..HttpRetryPolicy::default() }),
        ..PipelineParts::new(Arc::new(FakeTransport::echo()), HostLimits::spawn(per_host))
    }
}

async fn open(parts: PipelineParts, streams: &FakeStreamTransport, path: &'static str) -> Delivery<Opening> {
    let request = RenderedRequest::new(Method::GET, Uri::from_static(path));
    let call = Call { request, timeout: SECOND, ctx: SendContext::default() };
    let transport: Arc<dyn StreamTransport> = Arc::new(streams.clone());
    match super::stream_pipeline(parts, (transport, StreamMode::Body)).oneshot(call).await {
        Ok(delivery) => delivery,
        Err(never) => match never {},
    }
}

#[tokio::test(start_paused = true)]
async fn a_refused_open_is_retried_on_its_status_like_an_http_call() {
    let streams = FakeStreamTransport::scripted(vec![
        Ok(StreamAnswer::refused(status(503))),
        Ok(StreamAnswer::stream(status(200), Vec::new())),
    ]);
    let delivery = open(parts(8, 1), &streams, "http://api.test/events").await;
    let opened = delivery.result.as_ref().map(|opening| (opening.head.response.status, opening.stream.is_some()));
    assert_eq!(opened.ok(), Some((StatusCode::OK, true)));
    assert_eq!((delivery.attempts, streams.calls()), (2, 2));
}

#[tokio::test(start_paused = true)]
async fn an_open_stream_holds_its_host_permit_until_it_is_dropped() {
    let streams = FakeStreamTransport::contract();
    let limits = parts(1, 0);
    let first = open(limits.clone(), &streams, "http://api.test/events?count=1").await;
    assert!(first.result.as_ref().is_ok_and(Opening::holds_permit), "{first:?}");
    let waiting = tokio::time::timeout(10 * SECOND, open(limits.clone(), &streams, "http://api.test/events"));
    assert!(waiting.await.is_err(), "the second open waits while the first stream is open");
    drop(first);
    assert!(open(limits, &streams, "http://api.test/events").await.result.is_ok());
}
