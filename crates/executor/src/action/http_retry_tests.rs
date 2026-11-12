use std::sync::Arc;
use std::time::Duration;

use http::Uri;
use vc_interfaces::{Random, TransportError};
use vc_model::{HttpRetry, Method, OwnerKey, StatusPattern};
use vc_testkit::{FakeTransport, Fault, FaultyTransport, RecordingTransport, ScriptedRandom};

use super::{described, policy};
use crate::fixture::Fixture;
use crate::{ActionExecutors, ActionReport, ActionRequest, Attempted, Frame, HostLimits, PipelineParts, RetryOn};

fn connect_failed() -> TransportError {
    TransportError::Connect { host: "api.test:80".into(), reason: "refused".into() }
}

fn retrying(attempts: u32) -> HttpRetry {
    HttpRetry { attempts, jitter: false, ..HttpRetry::default() }
}

/// Runs a `GET` call whose first attempt fails to connect, with the config's `http.retry` set to
/// `config` and the api's to `api`; returns the report and how many attempts were sent.
async fn flaky_call(config: HttpRetry, api: Option<HttpRetry>) -> (ActionReport, usize) {
    let mut fixture = Fixture::new();
    let call = fixture.call("/users");
    fixture.program.config.http_retry = config;
    fixture.api_mut(call).http_retry = api;
    let faulty =
        FaultyTransport::new(Arc::new(FakeTransport::scripted(Vec::new())), vec![Fault::Fail(connect_failed())]);
    let (transport, recordings) = RecordingTransport::new(Arc::new(faulty));
    let executors = ActionExecutors::standard(PipelineParts::new(Arc::new(transport), HostLimits::spawn(8)));
    let cx = fixture.cx(executors);
    let request = ActionRequest { action: call, owner: OwnerKey::Run, cx: Arc::clone(&cx), frame: Frame::default() };
    let report = cx.executors.execute(request).await;
    (report, recordings.take().len())
}

/// The first wait after a connect failure of an api whose `http.retry` is `spec`, with `draw`
/// as the run's next random draw.
fn wait(spec: HttpRetry, draw: u64) -> Option<Duration> {
    let mut fixture = Fixture::new();
    let call = fixture.call("/users");
    fixture.api_mut(call).http_retry = Some(spec);
    let api = fixture.api_mut(call).clone();
    let random: Arc<dyn Random> = Arc::new(ScriptedRandom::new([draw]));
    let request = vc_model::RenderedRequest::new(http::Method::GET, Uri::from_static("http://api.test/users"));
    let result = Err(connect_failed());
    policy(&fixture.program, &api, &random).delay(&Attempted { request: &request, result: &result, retries: 0 })
}

#[tokio::test(start_paused = true)]
async fn retry_is_off_by_default_so_a_connect_failure_is_final() {
    let (report, attempts) = flaky_call(HttpRetry::default(), None).await;
    assert!(report.outcome.failure().is_some());
    assert_eq!(attempts, 1);
}

#[tokio::test(start_paused = true)]
async fn the_configs_retry_sends_again_after_a_connect_failure() {
    let (report, attempts) = flaky_call(retrying(2), None).await;
    assert!(report.outcome.failure().is_none(), "{:?}", report.outcome);
    assert_eq!(attempts, 2);
}

#[tokio::test(start_paused = true)]
async fn an_apis_retry_replaces_the_configs() {
    let (report, attempts) = flaky_call(retrying(2), Some(retrying(0))).await;
    assert!(report.outcome.failure().is_some());
    assert_eq!(attempts, 1);
}

#[test]
fn jitter_draws_the_wait_from_the_runs_random_and_no_jitter_keeps_it() {
    let jittered = HttpRetry { attempts: 1, ..HttpRetry::default() };
    assert_eq!(wait(jittered.clone(), 0), Some(Duration::from_millis(50)), "the least is half");
    assert_eq!(wait(jittered.clone(), 50_000_000), Some(Duration::from_millis(100)), "the most is all");
    assert_eq!(wait(jittered.clone(), 50_000_001), Some(Duration::from_millis(50)), "draws wrap around");
    assert_eq!(wait(jittered, 7), Some(Duration::from_nanos(50_000_007)));
    assert_eq!(wait(retrying(1), 0), Some(Duration::from_millis(100)), "no jitter keeps the wait");
    assert_eq!(wait(retrying(0), 0), None, "no attempts, no retry");
}

#[test]
fn the_spec_names_map_to_the_policys_outcomes_and_methods() {
    let spec = HttpRetry {
        attempts: 3,
        on_status: vec![StatusPattern::Class(5)],
        on_transport: vec!["timeout".into(), "unknown".into()],
        methods: vec![Method::Get, Method::Patch],
        base: vc_model::Duration::new(Duration::from_millis(10)),
        max: vc_model::Duration::new(Duration::from_millis(80)),
        jitter: false,
    };
    let policy = described(&spec);
    assert_eq!(policy.attempts, 3);
    assert_eq!(policy.on, vec![RetryOn::Timeout, RetryOn::Status(StatusPattern::Class(5))]);
    assert_eq!(policy.methods, vec![http::Method::GET, http::Method::PATCH]);
    assert_eq!((policy.base, policy.max), (Duration::from_millis(10), Duration::from_millis(80)));
}

/// How long a `GET` call takes on the paused clock when its transport answers 503 twice and then
/// 200, under the config's `http.retry` with `base` and `max` in milliseconds and no jitter; and
/// how many attempts it sent.
async fn backing_off(base: u64, max: u64) -> (Duration, usize) {
    let mut fixture = Fixture::new();
    let call = fixture.call("/users");
    let millis = |ms| vc_model::Duration::new(Duration::from_millis(ms));
    let on_status = vec![StatusPattern::Code(503)];
    let spec = HttpRetry { attempts: 2, on_status, base: millis(base), max: millis(max), ..retrying(2) };
    fixture.program.config.http_retry = spec;
    let answers = [503, 503, 200].map(|code| Ok(vc_testkit::status(code)));
    let fake = FakeTransport::scripted(answers.into());
    let executors = ActionExecutors::standard(PipelineParts::new(Arc::new(fake.clone()), HostLimits::spawn(8)));
    let cx = fixture.cx(executors);
    let request = ActionRequest { action: call, owner: OwnerKey::Run, cx: Arc::clone(&cx), frame: Frame::default() };
    let started = tokio::time::Instant::now();
    let report = cx.executors.execute(request).await;
    assert!(report.outcome.failure().is_none(), "{:?}", report.outcome);
    (started.elapsed(), fake.calls())
}

/// The exact waits, which `vc run` on a loaded machine can only bound from below (the e2e
/// `http_retry_backs_off_exponentially_up_to_its_max`).
#[tokio::test(start_paused = true)]
async fn the_configs_backoff_doubles_each_wait_up_to_its_max() {
    assert_eq!(backing_off(400, 5_000).await, (Duration::from_millis(1_200), 3), "400 ms, then 800 ms");
    assert_eq!(backing_off(400, 100).await, (Duration::from_millis(200), 3), "max caps both waits at 100 ms");
    assert_eq!(backing_off(100, 150).await, (Duration::from_millis(250), 3), "100 ms, then 200 ms cut to 150 ms");
}
