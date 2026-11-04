use std::sync::Arc;
use std::time::Duration;

use http::{Method, Uri};
use vc_interfaces::TransportError;
use vc_model::{ConnectionInfo, Exchange, RenderedRequest, StatusPattern, Timing};

use super::{Attempted, HttpRetryPolicy, Jittered, NoRetry, RetryOn, RetryPolicy};

fn request(method: Method) -> RenderedRequest {
    RenderedRequest::new(method, Uri::from_static("http://api.test/x"))
}

fn exchange(code: u16) -> Exchange {
    let response = vc_testkit::status(code);
    Exchange {
        request: request(Method::GET),
        response,
        timing: Timing::default(),
        connection: ConnectionInfo::default(),
    }
}

fn connect_failed() -> Result<Exchange, TransportError> {
    Err(TransportError::Connect { host: "api.test:80".into(), reason: "refused".into() })
}

fn delay(
    policy: &dyn RetryPolicy,
    method: Method,
    result: &Result<Exchange, TransportError>,
    retries: u32,
) -> Option<Duration> {
    policy.delay(&Attempted { request: &request(method), result, retries })
}

fn on(attempts: u32) -> HttpRetryPolicy {
    HttpRetryPolicy { attempts, ..HttpRetryPolicy::default() }
}

#[test]
fn defaults_follow_d143_and_retry_nothing() {
    let policy = HttpRetryPolicy::default();
    assert_eq!(policy.attempts, 0);
    assert_eq!(policy.on.len(), 5);
    assert!(policy.on.contains(&RetryOn::Status(StatusPattern::Code(504))));
    assert_eq!((policy.base, policy.max), (Duration::from_millis(100), Duration::from_secs(2)));
    assert_eq!(delay(&policy, Method::GET, &Ok(exchange(503)), 0), None);
    assert_eq!(delay(&NoRetry, Method::GET, &Ok(exchange(503)), 0), None);
}

#[test]
fn retries_listed_outcomes_of_idempotent_methods_only() {
    let policy = on(3);
    let timed_out = Err(TransportError::Timeout { after: Duration::from_secs(1) });
    for result in [Ok(exchange(502)), Ok(exchange(503)), Ok(exchange(504)), connect_failed(), timed_out] {
        assert_eq!(delay(&policy, Method::DELETE, &result, 0), Some(Duration::from_millis(100)), "{result:?}");
    }
    assert_eq!(delay(&policy, Method::POST, &Ok(exchange(503)), 0), None, "POST is not idempotent");
    assert_eq!(delay(&policy, Method::PATCH, &connect_failed(), 0), None);
    assert_eq!(delay(&policy, Method::GET, &Ok(exchange(500)), 0), None, "500 is not listed");
    assert_eq!(delay(&policy, Method::GET, &Ok(exchange(200)), 0), None);
    let dns = Err(TransportError::Dns { host: "api.test".into(), reason: "nx".into() });
    assert_eq!(delay(&policy, Method::GET, &dns, 0), None, "only listed errors are retried");
}

#[test]
fn backs_off_exponentially_up_to_max_and_stops_after_the_last_retry() {
    let policy = on(10);
    let waits: Vec<_> = (0..7).map(|retries| delay(&policy, Method::GET, &Ok(exchange(503)), retries)).collect();
    let millis = [100, 200, 400, 800, 1600, 2000, 2000].map(|ms| Some(Duration::from_millis(ms)));
    assert_eq!(waits, millis);
    assert_eq!(on(100).backoff(40), Duration::from_secs(2), "no overflow");
    assert_eq!(delay(&on(2), Method::GET, &Ok(exchange(503)), 1), Some(Duration::from_millis(200)));
    assert_eq!(delay(&on(2), Method::GET, &Ok(exchange(503)), 2), None);
}

#[test]
fn a_status_entry_matches_only_responses_with_that_status() {
    assert!(RetryOn::Status(StatusPattern::Code(502)).matches(&Ok(exchange(502))));
    assert!(!RetryOn::Status(StatusPattern::Code(502)).matches(&connect_failed()));
    assert!(!RetryOn::Connect.matches(&Ok(exchange(502))));
    assert!(!RetryOn::Timeout.matches(&connect_failed()));
}

#[test]
fn a_jittered_policy_debugs_as_its_name() {
    let random = Arc::new(vc_testkit::ScriptedRandom::new([0]));
    let jittered = Jittered::new(Arc::new(NoRetry), random);
    assert_eq!(format!("{jittered:?}"), "Jittered { .. }");
}

#[test]
fn a_status_class_matches_every_code_in_it() {
    let policy = HttpRetryPolicy { on: vec![RetryOn::Status(StatusPattern::Class(5))], ..on(1) };
    assert!(delay(&policy, Method::GET, &Ok(exchange(500)), 0).is_some());
    assert!(delay(&policy, Method::GET, &Ok(exchange(404)), 0).is_none());
}
