//! `/openapi/users*.yaml`: the users spec in two versions, the switch between them, and the
//! private copy only a bearer token is served.

mod support;

use reqwest::{Method, StatusCode};
use support::{Mock, TestResult};

#[tokio::test]
async fn each_version_has_its_own_route_and_names_the_server_it_came_from() -> TestResult {
    let mock = Mock::start().await?;
    let v1 = mock.get("/openapi/users-v1.yaml").send().await?;
    assert_eq!(v1.headers()[reqwest::header::CONTENT_TYPE], "application/yaml");
    let v1 = v1.text().await?;
    let v2 = mock.get("/openapi/users-v2.yaml").send().await?.text().await?;
    assert!(v1.contains("version: 1.0.0") && v1.contains("deleteUser") && !v1.contains("updateUser"), "{v1}");
    assert!(v2.contains("version: 2.0.0") && v2.contains("updateUser") && !v2.contains("deleteUser"), "{v2}");
    assert!(v1.contains(&format!("url: {}", mock.handle.base_url())), "{v1}");
    Ok(())
}

#[tokio::test]
async fn the_current_spec_is_v1_until_switched_and_again_after_a_reset() -> TestResult {
    let mock = Mock::start().await?;
    let version =
        async || -> Result<String, reqwest::Error> { mock.get("/openapi/users.yaml").send().await?.text().await };
    assert!(version().await?.contains("version: 1.0.0"));
    let switched = mock.request(Method::POST, "/__openapi/users/v2").send().await?;
    assert_eq!(switched.status(), StatusCode::NO_CONTENT);
    assert!(version().await?.contains("version: 2.0.0"));
    let unknown = mock.request(Method::POST, "/__openapi/users/v3").send().await?;
    assert_eq!(unknown.status(), StatusCode::NOT_FOUND);
    assert!(version().await?.contains("version: 2.0.0"));
    mock.handle.reset().await?;
    assert!(version().await?.contains("version: 1.0.0"));
    let stats = mock.handle.stats().await?;
    assert_eq!(stats.count("GET /openapi/users.yaml"), 1, "the switch is not counted: {stats:?}");
    Ok(())
}

#[tokio::test]
async fn the_private_spec_is_served_only_with_its_bearer_token() -> TestResult {
    let mock = Mock::start().await?;
    let path = "/openapi/private/users.yaml";
    assert_eq!(mock.get(path).send().await?.status(), StatusCode::UNAUTHORIZED);
    let wrong = mock.get(path).header("authorization", "Bearer someone-else").send().await?;
    assert_eq!(wrong.status(), StatusCode::UNAUTHORIZED);
    let read = mock.get(path).header("authorization", "Bearer spec-reader").send().await?;
    assert_eq!(read.status(), StatusCode::OK);
    assert!(read.text().await?.contains("version: 1.0.0"));
    Ok(())
}
