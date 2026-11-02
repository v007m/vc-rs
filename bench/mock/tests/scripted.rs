//! `/slow`, `/flaky`, `/poll/{id}`, `/status/{code}`, `/large` with compression, and the
//! in-flight gauge.

mod support;

use std::time::{Duration, Instant};

use reqwest::StatusCode;
use reqwest::header::{ACCEPT_ENCODING, CONTENT_ENCODING, HeaderValue};
use serde_json::Value;
use support::{Mock, TestResult};

#[tokio::test]
async fn slow_waits_before_answering() -> TestResult {
    let mock = Mock::start().await?;
    let started = Instant::now();
    let body: Value = mock.get("/slow?ms=150").send().await?.json().await?;
    assert!(started.elapsed() >= Duration::from_millis(150));
    assert_eq!(body["sleptMs"], 150);
    Ok(())
}

#[tokio::test]
async fn flaky_fails_the_first_calls_per_key() -> TestResult {
    let mock = Mock::start().await?;
    let mut statuses = Vec::new();
    for _ in 0..3 {
        statuses.push(mock.get("/flaky?key=a&failTimes=2").send().await?.status());
    }
    assert_eq!(statuses, [StatusCode::SERVICE_UNAVAILABLE, StatusCode::SERVICE_UNAVAILABLE, StatusCode::OK]);
    let other_key = mock.get("/flaky?key=b&failTimes=1").send().await?;
    assert_eq!(other_key.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(mock.handle.stats().await?.count("GET /flaky"), 4);
    Ok(())
}

#[tokio::test]
async fn poll_is_pending_until_the_nth_call() -> TestResult {
    let mock = Mock::start().await?;
    let mut statuses = Vec::new();
    for _ in 0..4 {
        let body: Value = mock.get("/poll/job1?after=3").send().await?.json().await?;
        statuses.push(body["status"].as_str().unwrap_or_default().to_owned());
    }
    assert_eq!(statuses, ["pending", "pending", "done", "done"]);
    let other: Value = mock.get("/poll/job2?after=2").send().await?.json().await?;
    assert_eq!(other["status"], "pending");
    assert_eq!(mock.handle.stats().await?.count("GET /poll/{id}"), 5);
    Ok(())
}

#[tokio::test]
async fn status_answers_with_the_requested_code() -> TestResult {
    let mock = Mock::start().await?;
    assert_eq!(mock.get("/status/418").send().await?.status(), StatusCode::IM_A_TEAPOT);
    assert_eq!(mock.get("/status/204").send().await?.status(), StatusCode::NO_CONTENT);
    assert_eq!(mock.get("/status/1000").send().await?.status(), StatusCode::BAD_REQUEST);
    Ok(())
}

#[tokio::test]
async fn large_bodies_are_compressed_when_the_client_accepts_it() -> TestResult {
    let mock = Mock::start().await?;
    let plain = mock.get("/large?kb=64").send().await?;
    assert!(plain.headers().get(CONTENT_ENCODING).is_none());
    assert!(plain.bytes().await?.len() > 64_000);
    for encoding in ["gzip", "br"] {
        let compressed = mock.get("/large?kb=64").header(ACCEPT_ENCODING, encoding).send().await?;
        assert_eq!(compressed.headers().get(CONTENT_ENCODING).map(HeaderValue::as_bytes), Some(encoding.as_bytes()));
        assert!(compressed.bytes().await?.len() < 6_400);
    }
    Ok(())
}

#[tokio::test]
async fn the_in_flight_gauge_records_concurrent_requests() -> TestResult {
    let mock = Mock::start().await?;
    let requests = (0..4).map(|_| mock.get("/slow?ms=300").send());
    for response in join_all(requests).await {
        assert_eq!(response?.status(), StatusCode::OK);
    }
    let stats = mock.handle.stats().await?;
    assert_eq!(stats.max_in_flight.get("GET /slow"), Some(&4));
    assert_eq!(stats.max_in_flight_total, 4);
    Ok(())
}

#[tokio::test]
async fn a_request_from_before_a_reset_does_not_lower_the_new_gauge() -> TestResult {
    let mock = Mock::start().await?;
    let early = tokio::spawn(mock.get("/slow?ms=200").send());
    tokio::time::sleep(Duration::from_millis(50)).await;
    mock.handle.reset().await?;
    let long = tokio::spawn(mock.get("/slow?ms=500").send());
    early.await??;
    tokio::time::sleep(Duration::from_millis(50)).await;
    mock.get("/slow?ms=10").send().await?;
    long.await??;
    let stats = mock.handle.stats().await?;
    assert_eq!(stats.max_in_flight.get("GET /slow"), Some(&2));
    assert_eq!(stats.max_in_flight_total, 2);
    Ok(())
}

/// Runs the futures concurrently on spawned tasks and collects their results in order.
async fn join_all<F>(futures: impl Iterator<Item = F>) -> Vec<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    let tasks: Vec<_> = futures.map(tokio::spawn).collect();
    let mut outputs = Vec::new();
    for task in tasks {
        if let Ok(output) = task.await {
            outputs.push(output);
        }
    }
    outputs
}
