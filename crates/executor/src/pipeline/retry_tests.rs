use std::time::Duration;

use http::{Method, StatusCode, Uri};
use vc_interfaces::TransportError;
use vc_model::{ConnectionInfo, Exchange, RenderedRequest, Timing};
use vc_testkit::status;

use super::{Attempted, HttpRetry, NoRetry, RetryOn, RetryPolicy};

fn request(method: Method) -> RenderedRequest {
    RenderedRequest::new(method, Uri::from_static("http://api.test/x"))
}

fn answered(code: u16) -> Result<Exchange, TransportError> {
    let response = status(code);
    Ok(Exchange { request: request(Method::GET), response, timing: Timing::default(), connection: ConnectionInfo::default() })
}

fn connect_failed() -> Result<Exchange, TransportError> {
    Err(TransportError::Connect { host: "api.test:80".into(), reason: "refused".into() })
}

fn delay(policy: &dyn RetryPolicy, method: Method, result: &Result<Exchange, TransportError>, retries: u32) -> Option<Duration> {
    policy.delay(&Attempted { request: &request(method), result, retries })
}

fn on(attempts: u32) -> HttpRetry {
    HttpRetry { attempts, ..HttpRetry::default() }
}

#[test]
fn defaults_follow_d143_and_retry_nothing() {
    let policy = HttpRetry::default();
    assert_eq!(policy.attempts, 0);
    assert_eq!(policy.on.len(), 5);
    assert!(policy.on.contains(&RetryOn::Status(StatusCode::GATEWAY_TIMEOUT)));
    assert_eq!((policy.base, policy.max), (Duration::from_millis(100), Duration::from_secs(2)));
    assert_eq!(delay(&policy, Method::GET, &answered(503), 0), None);
    assert_eq!(delay(&NoRetry, Method::GET, &answered(503), 0), None);
}

#[test]
fn retries_listed_outcomes_of_idempotent_methods_only() {
    let policy = on(3);
    let timed_out = Err(TransportError::Timeout { after: Duration::from_secs(1) });
    for result in [answered(502), answered(503), answered(504), connect_failed(), timed_out] {
        assert_eq!(delay(&policy, Method::DELETE, &result, 0), Some(Duration::from_millis(100)), "{result:?}");
    }
    assert_eq!(delay(&policy, Method::POST, &answered(503), 0), None, "POST is not idempotent");
    assert_eq!(delay(&policy, Method::PATCH, &connect_failed(), 0), None);
    assert_eq!(delay(&policy, Method::GET, &answered(500), 0), None, "500 is not listed");
    assert_eq!(delay(&policy, Method::GET, &answered(200), 0), None);
    let dns = Err(TransportError::Dns { host: "api.test".into(), reason: "nx".into() });
    assert_eq!(delay(&policy, Method::GET, &dns, 0), None, "only listed errors are retried");
}

#[test]
fn backs_off_exponentially_up_to_max_and_stops_after_the_last_retry() {
    let policy = on(10);
    let waits: Vec<_> = (0..7).map(|retries| delay(&policy, Method::GET, &answered(503), retries)).collect();
    let millis = [100, 200, 400, 800, 1600, 2000, 2000].map(|ms| Some(Duration::from_millis(ms)));
    assert_eq!(waits, millis);
    assert_eq!(delay(&policy, Method::GET, &answered(503), 40), Some(Duration::from_secs(2)), "no overflow");
    assert_eq!(delay(&on(2), Method::GET, &answered(503), 1), Some(Duration::from_millis(200)));
    assert_eq!(delay(&on(2), Method::GET, &answered(503), 2), None);
}

#[test]
fn a_status_entry_matches_only_responses_with_that_status() {
    assert!(RetryOn::Status(StatusCode::BAD_GATEWAY).matches(&answered(502)));
    assert!(!RetryOn::Status(StatusCode::BAD_GATEWAY).matches(&connect_failed()));
    assert!(!RetryOn::Connect.matches(&answered(502)));
    assert!(!RetryOn::Timeout.matches(&connect_failed()));
}
