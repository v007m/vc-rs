use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use http::{HeaderValue, Method, StatusCode, Uri};
use tokio::time::Instant;
use tower::ServiceExt;
use vc_diagnostics::Code;
use vc_interfaces::{SendContext, Transport, TransportError};
use vc_model::RenderedRequest;
use vc_testkit::{FakeTransport, Fault, FaultyTransport, status};

use super::{Call, Delivery, HostLimits, HttpRetryPolicy, PipelineParts, RequestStage, pipeline};

const SECOND: Duration = Duration::from_secs(1);

fn call(url: &'static str) -> Call {
    let request = RenderedRequest::new(Method::GET, Uri::from_static(url));
    Call { request, timeout: SECOND, ctx: SendContext::default() }
}

async fn run(parts: PipelineParts, call: Call) -> Delivery {
    match pipeline(parts).oneshot(call).await {
        Ok(delivery) => delivery,
        Err(never) => match never {},
    }
}

fn delayed(by: Duration, count: usize) -> Arc<dyn Transport> {
    Arc::new(FaultyTransport::new(Arc::new(FakeTransport::echo()), vec![Fault::Delay(by); count]))
}

fn retrying(transport: Arc<dyn Transport>, attempts: u32) -> PipelineParts {
    PipelineParts {
        retry: Arc::new(HttpRetryPolicy { attempts, ..HttpRetryPolicy::default() }),
        ..PipelineParts::new(transport, HostLimits::spawn(8))
    }
}

#[tokio::test(start_paused = true)]
async fn sends_one_attempt_by_default() {
    let fake = FakeTransport::scripted(vec![Ok(status(503))]);
    let delivery =
        run(PipelineParts::new(Arc::new(fake.clone()), HostLimits::spawn(8)), call("http://api.test/x")).await;
    assert_eq!(delivery.result.map(|exchange| exchange.response.status), Ok(StatusCode::SERVICE_UNAVAILABLE));
    assert_eq!((delivery.attempts, delivery.elapsed, fake.calls()), (1, Duration::ZERO, 1));
}

#[tokio::test(start_paused = true)]
async fn retries_with_backoff_and_times_the_whole_call() {
    let fake = FakeTransport::scripted(vec![Ok(status(503)), Ok(status(502)), Ok(status(200))]);
    let delivery = run(retrying(Arc::new(fake.clone()), 3), call("http://api.test/x")).await;
    assert_eq!(delivery.result.map(|exchange| exchange.response.status), Ok(StatusCode::OK));
    assert_eq!((delivery.attempts, fake.calls()), (3, 3));
    assert_eq!(delivery.elapsed, Duration::from_millis(300), "100 ms, then 200 ms");
}

#[tokio::test(start_paused = true)]
async fn the_timeout_limits_each_attempt_and_a_timed_out_attempt_is_retried() {
    let transport = Arc::new(FaultyTransport::new(Arc::new(FakeTransport::echo()), vec![Fault::Delay(5 * SECOND)]));
    let delivery = run(retrying(transport, 1), call("http://api.test/echo")).await;
    assert_eq!(delivery.result.map(|exchange| exchange.response.status), Ok(StatusCode::OK));
    assert_eq!((delivery.attempts, delivery.elapsed), (2, SECOND + Duration::from_millis(100)));
}

#[tokio::test(start_paused = true)]
async fn a_timeout_is_v5104_with_the_limit() {
    let delivery =
        run(PipelineParts::new(delayed(5 * SECOND, 1), HostLimits::spawn(8)), call("http://api.test/x")).await;
    assert_eq!(delivery.result.as_ref().err(), Some(&TransportError::Timeout { after: SECOND }));
    assert_eq!(delivery.result.err().map(|error| error.code()), Some(Code::V5104));
    assert_eq!(delivery.elapsed, SECOND);
}

/// Appends its name to `x-stages` and counts its calls.
struct Mark(&'static str, AtomicUsize);

#[async_trait]
impl RequestStage for Mark {
    async fn apply(&self, mut request: RenderedRequest) -> Result<RenderedRequest, TransportError> {
        self.1.fetch_add(1, Ordering::SeqCst);
        let before = request.headers.get("x-stages").and_then(|value| value.to_str().ok()).unwrap_or_default();
        let after = HeaderValue::try_from(format!("{before}{}", self.0))
            .map_err(|_| TransportError::Protocol { reason: "bad".into() })?;
        request.headers.insert("x-stages", after);
        Ok(request)
    }
}

#[tokio::test(start_paused = true)]
async fn auth_runs_before_hooks_on_every_attempt() {
    let (auth, hooks) = (Arc::new(Mark("auth;", AtomicUsize::new(0))), Arc::new(Mark("hooks;", AtomicUsize::new(0))));
    let fake = FakeTransport::scripted(vec![Ok(status(503)), Ok(status(200))]);
    let parts = PipelineParts { auth: auth.clone(), hooks: hooks.clone(), ..retrying(Arc::new(fake), 1) };
    let delivery = run(parts, call("http://api.test/x")).await;
    let sent = delivery.result.map(|exchange| exchange.request.headers.get("x-stages").cloned());
    assert_eq!(
        sent,
        Ok(Some(HeaderValue::from_static("auth;hooks;"))),
        "each attempt starts from the rendered request"
    );
    assert_eq!((auth.1.load(Ordering::SeqCst), hooks.1.load(Ordering::SeqCst)), (2, 2));
}

/// A stage that refuses every request.
struct Refuse;

#[async_trait]
impl RequestStage for Refuse {
    async fn apply(&self, _: RenderedRequest) -> Result<RenderedRequest, TransportError> {
        Err(TransportError::InvalidRequest { reason: "no credentials".into() })
    }
}

#[tokio::test(start_paused = true)]
async fn a_failing_stage_stops_the_request_before_the_transport() {
    let fake = FakeTransport::echo();
    let parts =
        PipelineParts { auth: Arc::new(Refuse), ..PipelineParts::new(Arc::new(fake.clone()), HostLimits::spawn(8)) };
    let delivery = run(parts, call("http://api.test/x")).await;
    assert_eq!(delivery.result.err(), Some(TransportError::InvalidRequest { reason: "no credentials".into() }));
    assert_eq!(fake.calls(), 0);
}

/// Sends three 100 ms calls at once and returns how long they took together.
async fn three_at_once(per_host: usize, urls: [&'static str; 3]) -> Duration {
    let parts = PipelineParts::new(delayed(Duration::from_millis(100), 3), HostLimits::spawn(per_host));
    let started = Instant::now();
    let [a, b, c] = urls.map(|url| run(parts.clone(), call(url)));
    let (a, b, c) = tokio::join!(a, b, c);
    assert!(a.result.is_ok() && b.result.is_ok() && c.result.is_ok());
    started.elapsed()
}

#[tokio::test(start_paused = true)]
async fn limits_concurrent_attempts_per_host() {
    let same = ["http://api.test/echo", "http://API.test:80/echo", "http://api.test/echo?x=1"];
    assert_eq!(three_at_once(2, same).await, Duration::from_millis(200), "two run, the third waits");
    assert_eq!(three_at_once(3, same).await, Duration::from_millis(100));
    assert_eq!(three_at_once(0, same).await, Duration::from_millis(300), "zero means one");
    let spread = ["http://a.test/echo", "http://b.test/echo", "https://a.test/echo"];
    assert_eq!(three_at_once(1, spread).await, Duration::from_millis(100), "hosts do not share permits");
}

#[tokio::test(start_paused = true)]
async fn waiting_for_a_permit_does_not_count_against_the_timeout() {
    let parts = PipelineParts::new(delayed(Duration::from_millis(800), 2), HostLimits::spawn(1));
    let (first, second) =
        tokio::join!(run(parts.clone(), call("http://api.test/echo")), run(parts, call("http://api.test/echo")));
    assert!(first.result.is_ok() && second.result.is_ok(), "{second:?}");
    assert_eq!(second.elapsed.max(first.elapsed), Duration::from_millis(1600));
}

#[tokio::test]
async fn debug_output_names_every_layer() {
    let shown = format!("{:?}", pipeline(PipelineParts::new(Arc::new(FakeTransport::echo()), HostLimits::spawn(1))));
    for layer in ["Timing", "Retry", "Stage", "HostLimit", "Timeout", "Transmit"] {
        assert!(shown.contains(layer), "{layer} missing from {shown}");
    }
}
