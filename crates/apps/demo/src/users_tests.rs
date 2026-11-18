use std::collections::BTreeSet;

use axum::http::{Method, StatusCode};
use serde_json::{Value, json};
use tokio::task::JoinSet;

use super::{MAX_USER_BYTES, MAX_USERS};
use crate::router;
use crate::testing::send;

const ADA: &str = r#"{"name": "Ada", "email": "ada@example.com"}"#;

/// The ID of a user made on `app` from `body`.
async fn made(app: &axum::Router, body: &str) -> String {
    let (status, user) = send(app, Method::POST, "/users", body).await;
    assert_eq!(status, StatusCode::CREATED, "{user}");
    user["id"].as_str().unwrap().to_owned()
}

#[tokio::test]
async fn ada_and_grace_are_listed_and_read() {
    let app = router();
    let all = json!([{"id": "1", "name": "Ada Lovelace"}, {"id": "2", "name": "Grace Hopper"}]);
    assert_eq!(send(&app, Method::GET, "/users", "").await, (StatusCode::OK, all));
    assert_eq!(
        send(&app, Method::GET, "/users/2", "").await,
        (StatusCode::OK, json!({"id": "2", "name": "Grace Hopper"}))
    );
    assert_eq!(send(&app, Method::GET, "/users/9", "").await, (StatusCode::NOT_FOUND, Value::Null));
}

#[tokio::test]
async fn ada_and_grace_cannot_be_changed_or_deleted() {
    let app = router();
    for (method, body) in [(Method::PUT, ADA), (Method::PATCH, r#"{"name": "Bob"}"#), (Method::DELETE, "")] {
        let (status, refused) = send(&app, method.clone(), "/users/1", body).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{method}");
        assert!(refused["error"].as_str().unwrap().contains("users 1 and 2"), "{refused}");
    }
    assert_eq!(send(&app, Method::GET, "/users/1", "").await.1["name"], "Ada Lovelace", "unchanged");
}

#[tokio::test]
async fn a_created_user_is_kept_until_it_is_deleted() {
    let app = router();
    let (status, created) = send(&app, Method::POST, "/users", ADA).await;
    assert_eq!(
        (status, &created),
        (StatusCode::CREATED, &json!({"id": "3", "name": "Ada", "email": "ada@example.com"}))
    );
    assert_eq!(send(&app, Method::GET, "/users/3", "").await, (StatusCode::OK, created.clone()));
    assert_eq!(send(&app, Method::GET, "/users", "").await.1[2], created, "listed after Ada and Grace");
    assert_eq!(send(&app, Method::DELETE, "/users/3", "").await, (StatusCode::NO_CONTENT, Value::Null));
    assert_eq!(send(&app, Method::GET, "/users/3", "").await.0, StatusCode::NOT_FOUND);
    assert_eq!(send(&app, Method::DELETE, "/users/3", "").await.0, StatusCode::NOT_FOUND, "gone already");
    assert_eq!(send(&app, Method::GET, "/users", "").await.1.as_array().map(Vec::len), Some(2));
    assert_eq!(made(&app, ADA).await, "4", "an ID is never handed out twice");
}

#[tokio::test]
async fn a_user_gets_a_name_keeps_its_fields_and_not_its_own_id() {
    let app = router();
    let unnamed = send(&app, Method::POST, "/users", r#"{"email": "x@example.com", "id": "99", "team": [1]}"#).await;
    let expected = json!({"id": "3", "name": "someone", "email": "x@example.com", "team": [1]});
    assert_eq!(unnamed, (StatusCode::CREATED, expected));
    let admin = send(&app, Method::POST, "/users", r#"{"email": "a@example.com", "role": "admin"}"#).await.1;
    assert_eq!(admin["role"], "admin");
}

#[tokio::test]
async fn a_user_that_is_not_valid_is_refused() {
    let app = router();
    let cases = [
        (r#"{"name": "No Email"}"#, "a user needs an email"),
        (r#"{"email": 42}"#, "a user needs an email"),
        (r#"{"email": "a@example.com", "name": 7}"#, "a user's `name` is text"),
        (r#"{"email": "a@example.com", "role": "owner"}"#, "a user's `role` is one of admin, member"),
        (r#"[{"email": "a@example.com"}]"#, "a user is a JSON object, with an email"),
        ("not json", "a user is a JSON object, with an email"),
        ("", "a user is a JSON object, with an email"),
    ];
    for (body, error) in cases {
        assert_eq!(
            send(&app, Method::POST, "/users", body).await,
            (StatusCode::BAD_REQUEST, json!({"error": error})),
            "{body}"
        );
    }
    assert_eq!(send(&app, Method::GET, "/users", "").await.1.as_array().map(Vec::len), Some(2), "none kept");
}

#[tokio::test]
async fn a_user_over_the_size_cap_is_refused() {
    let app = router();
    let too_long = json!({"email": "a@example.com", "bio": "x".repeat(MAX_USER_BYTES)}).to_string();
    let error = json!({"error": format!("a user is at most {MAX_USER_BYTES} bytes")});
    assert_eq!(send(&app, Method::POST, "/users", &too_long).await, (StatusCode::BAD_REQUEST, error.clone()));
    let huge = format!("{{{}}}", " ".repeat(2 * MAX_USER_BYTES));
    assert_eq!(send(&app, Method::POST, "/users", &huge).await, (StatusCode::BAD_REQUEST, error.clone()), "unread");
    let near = json!({"email": "a@example.com", "bio": "x".repeat(MAX_USER_BYTES - 100)}).to_string();
    let id = made(&app, &near).await;
    let more = json!({"more": "y".repeat(100)}).to_string();
    let grown = send(&app, Method::PATCH, &format!("/users/{id}"), &more).await;
    assert_eq!(grown, (StatusCode::BAD_REQUEST, error), "the merged user is checked too");
}

#[tokio::test]
async fn a_user_at_the_size_cap_can_be_patched_and_replaced() {
    let app = router();
    let short = json!({"email": "a@example.com", "name": ""}).to_string().len();
    let body = json!({"email": "a@example.com", "name": "x".repeat(MAX_USER_BYTES - short)}).to_string();
    assert_eq!(body.len(), MAX_USER_BYTES);
    let id = made(&app, &body).await;
    let path = format!("/users/{id}");
    let (status, user) = send(&app, Method::PATCH, &path, "{}").await;
    assert_eq!(status, StatusCode::OK, "the id it was given does not count: {user}");
    assert_eq!(send(&app, Method::PUT, &path, &user.to_string()).await.0, StatusCode::OK, "nor one sent back");
    assert_eq!(send(&app, Method::POST, "/users", &body).await.0, StatusCode::CREATED);
}

#[tokio::test]
async fn put_replaces_a_created_user() {
    let app = router();
    let id = made(&app, r#"{"email": "a@example.com", "team": "core"}"#).await;
    let path = format!("/users/{id}");
    let replaced = send(&app, Method::PUT, &path, r#"{"email": "b@example.com", "role": "member"}"#).await;
    let expected = json!({"id": id, "name": "someone", "email": "b@example.com", "role": "member"});
    assert_eq!(replaced, (StatusCode::OK, expected.clone()), "`team` is gone");
    assert_eq!(send(&app, Method::GET, &path, "").await.1, expected);
    let refused = send(&app, Method::PUT, &path, r#"{"name": "No Email"}"#).await;
    assert_eq!(refused, (StatusCode::BAD_REQUEST, json!({"error": "a user needs an email"})));
    assert_eq!(send(&app, Method::GET, &path, "").await.1, expected, "a refused PUT changes nothing");
    assert_eq!(send(&app, Method::PUT, "/users/77", ADA).await.0, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn patch_merges_into_a_created_user() {
    let app = router();
    let id = made(&app, r#"{"name": "Ada", "email": "a@example.com", "team": "core"}"#).await;
    let path = format!("/users/{id}");
    let patched = send(&app, Method::PATCH, &path, r#"{"role": "admin", "team": null, "id": "1"}"#).await;
    let expected = json!({"id": id, "name": "Ada", "email": "a@example.com", "role": "admin"});
    assert_eq!(patched, (StatusCode::OK, expected.clone()), "`null` removes a key; the ID stays");
    assert_eq!(send(&app, Method::PATCH, &path, "{}").await, (StatusCode::OK, expected.clone()));
    let refused = send(&app, Method::PATCH, &path, r#"{"email": null}"#).await;
    assert_eq!(refused, (StatusCode::BAD_REQUEST, json!({"error": "a user needs an email"})));
    assert_eq!(send(&app, Method::PATCH, &path, r#"{"role": "owner"}"#).await.0, StatusCode::BAD_REQUEST);
    assert_eq!(send(&app, Method::GET, &path, "").await.1, expected, "a refused PATCH changes nothing");
    assert_eq!(send(&app, Method::PATCH, "/users/77", "{}").await.0, StatusCode::NOT_FOUND);
    assert_eq!(send(&app, Method::PATCH, &path, "[]").await.0, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn an_id_in_any_other_form_names_no_user() {
    let app = router();
    made(&app, ADA).await;
    for id in ["03", "+3", "3.0", "three", "-1", "0", "18446744073709551616"] {
        for method in [Method::GET, Method::DELETE] {
            assert_eq!(
                send(&app, method.clone(), &format!("/users/{id}"), "").await.0,
                StatusCode::NOT_FOUND,
                "{method} {id}"
            );
        }
    }
    assert_eq!(send(&app, Method::GET, "/users/%FF", "").await.0, StatusCode::BAD_REQUEST, "not UTF-8");
}

#[tokio::test]
async fn limit_takes_the_first_users() {
    let app = router();
    made(&app, ADA).await;
    let names = |list: Value| list.as_array().unwrap().iter().map(|user| user["id"].clone()).collect::<Vec<_>>();
    assert_eq!(names(send(&app, Method::GET, "/users?limit=1", "").await.1), vec![json!("1")]);
    assert_eq!(names(send(&app, Method::GET, "/users?limit=3", "").await.1).len(), 3);
    assert_eq!(names(send(&app, Method::GET, "/users?limit=50", "").await.1).len(), 3);
    assert_eq!(send(&app, Method::GET, "/users?limit=0", "").await, (StatusCode::OK, json!([])));
    for bad in ["-1", "1.5", "", "lots"] {
        let refused = send(&app, Method::GET, &format!("/users?limit={bad}"), "").await;
        assert_eq!(refused, (StatusCode::BAD_REQUEST, json!({"error": "`limit` is a whole number"})), "{bad}");
    }
}

#[tokio::test]
async fn the_store_holds_at_most_max_users_and_a_delete_makes_room() {
    let app = router();
    for _ in 0..MAX_USERS {
        made(&app, ADA).await;
    }
    let full = send(&app, Method::POST, "/users", ADA).await;
    let error = format!("the demo holds at most {MAX_USERS} created users; delete some, or restart it");
    assert_eq!(full, (StatusCode::BAD_REQUEST, json!({"error": error})), "refused, never dropped quietly");
    assert_eq!(send(&app, Method::GET, "/users", "").await.1.as_array().map(Vec::len), Some(MAX_USERS + 2));
    assert_eq!(send(&app, Method::DELETE, "/users/3", "").await.0, StatusCode::NO_CONTENT);
    assert_eq!(made(&app, ADA).await, (MAX_USERS + 3).to_string(), "room again, with a new ID");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn users_created_at_once_each_get_their_own_id() {
    let app = router();
    let mut creating = JoinSet::new();
    for _ in 0..64 {
        let app = app.clone();
        creating.spawn(async move { made(&app, ADA).await });
    }
    let ids: BTreeSet<String> = creating.join_all().await.into_iter().collect();
    assert_eq!(ids.len(), 64);
    assert_eq!(send(&app, Method::GET, "/users", "").await.1.as_array().map(Vec::len), Some(66));
}

#[tokio::test]
async fn each_router_keeps_its_own_users() {
    let (first, second) = (router(), router());
    made(&first, ADA).await;
    assert_eq!(made(&second, ADA).await, "3");
    assert_eq!(send(&second, Method::GET, "/users", "").await.1.as_array().map(Vec::len), Some(3));
}
