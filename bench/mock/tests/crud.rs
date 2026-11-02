//! CRUD routes for `/users` and `/orders`, and their counters.

mod support;

use reqwest::{Method, StatusCode};
use serde_json::{Value, json};
use support::{Mock, TestResult};

#[tokio::test]
async fn users_go_through_create_read_update_delete() -> TestResult {
    let mock = Mock::start().await?;
    let created = mock.request(Method::POST, "/users").json(&json!({ "name": "Ada" })).send().await?;
    assert_eq!(created.status(), StatusCode::CREATED);
    let user: Value = created.json().await?;
    assert_eq!(user["name"], "Ada");
    let path = format!("/users/{}", user["id"]);

    let read: Value = mock.get(&path).send().await?.json().await?;
    assert_eq!(read, user);
    let list: Value = mock.get("/users").send().await?.json().await?;
    assert_eq!(list, json!([user]));

    let put_response = mock.request(Method::PUT, &path).json(&json!({ "email": "a@x.io" })).send().await?;
    let replaced: Value = put_response.json().await?;
    assert_eq!(replaced, json!({ "id": user["id"], "email": "a@x.io" }));
    let patch_response = mock.request(Method::PATCH, &path).json(&json!({ "name": "Grace" })).send().await?;
    let merged: Value = patch_response.json().await?;
    assert_eq!(merged, json!({ "id": user["id"], "email": "a@x.io", "name": "Grace" }));

    assert_eq!(mock.request(Method::DELETE, &path).send().await?.status(), StatusCode::NO_CONTENT);
    assert_eq!(mock.get(&path).send().await?.status(), StatusCode::NOT_FOUND);
    assert_eq!(mock.request(Method::DELETE, &path).send().await?.status(), StatusCode::NOT_FOUND);
    Ok(())
}

#[tokio::test]
async fn deletes_are_counted_per_route_template() -> TestResult {
    let mock = Mock::start().await?;
    for _ in 0..3 {
        let order: Value = mock.request(Method::POST, "/orders").json(&json!({})).send().await?.json().await?;
        mock.request(Method::DELETE, &format!("/orders/{}", order["id"])).send().await?;
    }
    let stats = mock.handle.stats().await?;
    assert_eq!(stats.count("POST /orders"), 3);
    assert_eq!(stats.count("DELETE /orders/{id}"), 3);
    assert_eq!(stats.count("DELETE /users/{id}"), 0);
    Ok(())
}

#[tokio::test]
async fn collections_are_separate() -> TestResult {
    let mock = Mock::start().await?;
    let order: Value = mock.request(Method::POST, "/orders").json(&json!({ "total": 3 })).send().await?.json().await?;
    assert_eq!(mock.get(&format!("/orders/{}", order["id"])).send().await?.status(), StatusCode::OK);
    assert_eq!(mock.get(&format!("/users/{}", order["id"])).send().await?.status(), StatusCode::NOT_FOUND);
    let users: Value = mock.get("/users").send().await?.json().await?;
    assert_eq!(users, json!([]));
    Ok(())
}

#[tokio::test]
async fn updating_a_missing_item_is_not_found() -> TestResult {
    let mock = Mock::start().await?;
    for method in [Method::PUT, Method::PATCH] {
        let response = mock.request(method, "/users/42").json(&json!({})).send().await?;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
    Ok(())
}
