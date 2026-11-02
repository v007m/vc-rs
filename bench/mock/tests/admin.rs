//! `/__stats`, `/__reset`, the handle API and shutdown.

mod support;

use reqwest::{Method, StatusCode};
use serde_json::{Value, json};
use support::{Mock, TestResult};
use vc_mock_server::{MockServer, Stats};

#[tokio::test]
async fn stats_over_http_match_the_handle_and_skip_admin_routes() -> TestResult {
    let mock = Mock::start().await?;
    mock.request(Method::POST, "/users").json(&json!({})).send().await?;
    mock.get("/users/1").send().await?;
    mock.get("/no-such-route").send().await?;
    let served: Stats = mock.get("/__stats").send().await?.json().await?;
    assert_eq!(served, mock.handle.stats().await?);
    assert_eq!(served.count("POST /users"), 1);
    assert_eq!(served.count("GET /users/{id}"), 1);
    assert_eq!(served.requests.len(), 2, "admin and unmatched requests are not counted: {served:?}");
    assert_eq!(served.max_in_flight_total, 1);
    Ok(())
}

#[tokio::test]
async fn reset_over_http_clears_counters_and_stores() -> TestResult {
    let mock = Mock::start().await?;
    mock.request(Method::POST, "/users").json(&json!({})).send().await?;
    mock.request(Method::POST, "/auth/token").send().await?;
    let reset = mock.request(Method::POST, "/__reset").send().await?;
    assert_eq!(reset.status(), StatusCode::NO_CONTENT);
    assert_eq!(mock.handle.stats().await?, Stats::default());
    let users: Value = mock.get("/users").send().await?.json().await?;
    assert_eq!(users, json!([]));
    Ok(())
}

#[tokio::test]
async fn reset_on_the_handle_clears_flaky_and_poll_counters() -> TestResult {
    let mock = Mock::start().await?;
    assert_eq!(mock.get("/flaky?failTimes=1").send().await?.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(mock.get("/flaky?failTimes=1").send().await?.status(), StatusCode::OK);
    mock.handle.reset().await?;
    assert_eq!(mock.get("/flaky?failTimes=1").send().await?.status(), StatusCode::SERVICE_UNAVAILABLE);
    Ok(())
}

#[tokio::test]
async fn each_server_has_its_own_port_and_state() -> TestResult {
    let first = Mock::start().await?;
    let second = Mock::start().await?;
    assert_ne!(first.handle.addr(), second.handle.addr());
    first.request(Method::POST, "/users").json(&json!({})).send().await?;
    assert_eq!(second.handle.stats().await?.count("POST /users"), 0);
    Ok(())
}

#[tokio::test]
async fn shutdown_stops_the_listener() -> TestResult {
    let handle = MockServer::start().await?;
    let url = format!("{}/users", handle.base_url());
    handle.shutdown().await?;
    assert!(reqwest::get(url).await.is_err());
    Ok(())
}
