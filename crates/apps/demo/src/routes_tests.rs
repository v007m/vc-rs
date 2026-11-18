use std::collections::BTreeSet;
use std::sync::Arc;

use axum::body::{self, Body};
use axum::http::{Method, Request, StatusCode};
use serde_json::{Value, json};
use tokio::sync::Barrier;
use tokio::task::JoinSet;
use tower::ServiceExt;

use super::router;
use crate::scripted::Counters;

/// The status and JSON body (`null` when empty) of `method path` with `body`.
async fn send(app: &axum::Router, method: Method, path: &str, body: &str) -> (StatusCode, Value) {
    let request = Request::builder().method(method).uri(path).body(Body::from(body.to_owned())).unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

#[tokio::test]
async fn users_are_listed_read_created_and_removed() {
    let app = router();
    let (status, all) = send(&app, Method::GET, "/users", "").await;
    assert_eq!((status, all[1]["name"].clone()), (StatusCode::OK, json!("Grace Hopper")));
    assert_eq!(send(&app, Method::GET, "/users/1", "").await.1["name"], "Ada Lovelace");
    assert_eq!(send(&app, Method::GET, "/users/9", "").await.0, StatusCode::NOT_FOUND);
    let created = send(&app, Method::POST, "/users", r#"{"name": "Alan", "email": "alan@example.com"}"#).await;
    assert_eq!(created, (StatusCode::CREATED, json!({"id": "3", "name": "Alan", "email": "alan@example.com"})));
    let unnamed = send(&app, Method::POST, "/users", r#"{"email": "x@example.com"}"#).await.1;
    assert_eq!(unnamed, json!({"id": "4", "name": "someone", "email": "x@example.com"}));
    assert_eq!(send(&app, Method::GET, "/users/3", "").await, (StatusCode::OK, json!({"id": "3"})));
    assert_eq!(send(&app, Method::GET, "/users/5", "").await.0, StatusCode::NOT_FOUND, "not created yet");
    assert_eq!(send(&app, Method::DELETE, "/users/3", "").await, (StatusCode::NO_CONTENT, Value::Null));
}

#[tokio::test]
async fn a_user_without_an_email_is_refused() {
    let app = router();
    let refused = send(&app, Method::POST, "/users", r#"{"name": "No Email"}"#).await;
    assert_eq!((refused.0, refused.1["id"].clone()), (StatusCode::BAD_REQUEST, Value::Null));
    assert_eq!(send(&app, Method::POST, "/users", "").await.0, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn echo_describes_the_request() {
    let app = router();
    let (status, echoed) = send(&app, Method::PUT, "/echo?a=1", r#"{"word": "hi"}"#).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!((echoed["method"].clone(), echoed["path"].clone()), (json!("PUT"), json!("/echo")));
    assert_eq!((echoed["query"]["a"].clone(), echoed["body"]["word"].clone()), (json!("1"), json!("hi")));
    assert_eq!(send(&app, Method::GET, "/echo", "plain").await.1["body"], "plain");
    assert_eq!(send(&app, Method::GET, "/echo", "").await.1["body"], Value::Null);
}

#[tokio::test]
async fn echo_names_the_pid_of_the_demo() {
    let request = Request::builder().uri("/echo").body(Body::empty()).unwrap();
    let response = router().oneshot(request).await.unwrap();
    let pid = response.headers().get(super::PID_HEADER).and_then(|value| value.to_str().ok());
    assert_eq!(pid, Some(std::process::id().to_string().as_str()));
}

#[tokio::test(start_paused = true)]
async fn slow_waits_the_milliseconds_asked_for_up_to_the_cap() {
    let app = router();
    let started = tokio::time::Instant::now();
    assert_eq!(send(&app, Method::GET, "/slow?ms=10000", "").await, (StatusCode::OK, json!({"sleptMs": 10_000})));
    assert!(started.elapsed() >= std::time::Duration::from_secs(10), "it waits");
    assert_eq!(send(&app, Method::GET, "/slow", "").await.1, json!({"sleptMs": 0}));
    let over = send(&app, Method::GET, "/slow?ms=10001", "").await;
    assert_eq!(over, (StatusCode::BAD_REQUEST, json!({"error": "`ms` is at most 10000"})), "refused, not clamped");
    for bad in ["/slow?ms=-1", "/slow?ms=soon", "/slow?ms=%2B5", "/slow?ms=+5", "/slow?ms=1.5", "/slow?ms="] {
        assert_eq!(
            send(&app, Method::GET, bad, "").await,
            (StatusCode::BAD_REQUEST, json!({"error": "`ms` is a whole number"})),
            "{bad}"
        );
    }
}

#[tokio::test]
async fn every_count_takes_ascii_digits_only() {
    let app = router();
    for (path, name) in [("/flaky?key=k&failTimes=", "failTimes"), ("/poll/p?after=", "after")] {
        for bad in ["%2B1", "-1", "1.5", ""] {
            let error = json!({"error": format!("`{name}` is a whole number")});
            assert_eq!(send(&app, Method::GET, &format!("{path}{bad}"), "").await, (StatusCode::BAD_REQUEST, error));
        }
    }
}

#[tokio::test]
async fn a_request_the_routes_cannot_read_gets_a_json_error() {
    let app = router();
    for (path, name) in [("/poll/%FF", "id"), ("/status/%FF", "code")] {
        let error = json!({"error": format!("Invalid URL: Invalid UTF-8 in `{name}`")});
        assert_eq!(send(&app, Method::GET, path, "").await, (StatusCode::BAD_REQUEST, error), "{path}");
    }
    let lossy = send(&app, Method::GET, "/slow?ms=%FF", "").await;
    assert_eq!(lossy, (StatusCode::BAD_REQUEST, json!({"error": "`ms` is a whole number"})), "a query reads lossily");
}

#[tokio::test]
async fn flaky_fails_the_first_attempts_per_key_then_passes() {
    let app = router();
    let attempt = |n: u64| json!({"attempt": n});
    for (n, status) in [(1, StatusCode::SERVICE_UNAVAILABLE), (2, StatusCode::SERVICE_UNAVAILABLE), (3, StatusCode::OK)]
    {
        assert_eq!(send(&app, Method::GET, "/flaky?key=a&failTimes=2", "").await, (status, attempt(n)));
    }
    assert_eq!(send(&app, Method::GET, "/flaky?key=b&failTimes=2", "").await.1, attempt(1), "keys count apart");
    assert_eq!(send(&app, Method::GET, "/flaky", "").await, (StatusCode::OK, attempt(1)), "no key and no failures");
    let bad = send(&app, Method::GET, "/flaky?key=a&failTimes=x", "").await;
    assert_eq!(bad, (StatusCode::BAD_REQUEST, json!({"error": "`failTimes` is a whole number"})));
    assert_eq!(send(&app, Method::GET, "/flaky?key=a", "").await.1, attempt(4), "a refused attempt is not counted");
}

#[tokio::test]
async fn flaky_and_poll_refuse_a_key_over_64_bytes() {
    let app = router();
    let (longest, over) = ("k".repeat(64), "k".repeat(65));
    assert_eq!(send(&app, Method::GET, &format!("/flaky?key={longest}"), "").await.0, StatusCode::OK);
    let refused = send(&app, Method::GET, &format!("/flaky?key={over}"), "").await;
    assert_eq!(refused, (StatusCode::BAD_REQUEST, json!({"error": "the key is at most 64 bytes"})));
    assert_eq!(send(&app, Method::GET, &format!("/poll/{longest}"), "").await.0, StatusCode::OK);
    let refused = send(&app, Method::GET, &format!("/poll/{over}"), "").await;
    assert_eq!(refused, (StatusCode::BAD_REQUEST, json!({"error": "the id is at most 64 bytes"})));
}

#[tokio::test]
async fn a_full_table_refuses_a_new_key_and_still_counts_the_ones_it_holds() {
    let app = router();
    for key in 0..1024 {
        assert_eq!(send(&app, Method::GET, &format!("/flaky?key={key}"), "").await.0, StatusCode::OK);
    }
    let full = send(&app, Method::GET, "/flaky?key=new", "").await;
    let error = json!({"error": "the demo counts at most 1024 of these; restart it to count afresh"});
    assert_eq!(full, (StatusCode::BAD_REQUEST, error.clone()));
    assert_eq!(send(&app, Method::GET, "/flaky?key=7", "").await.1, json!({"attempt": 2}));
    assert_eq!(send(&app, Method::GET, "/poll/new", "").await.0, StatusCode::OK, "poll ids are a table of their own");
    for id in 0..1023 {
        assert_eq!(send(&app, Method::GET, &format!("/poll/{id}"), "").await.0, StatusCode::OK);
    }
    assert_eq!(send(&app, Method::GET, "/poll/last", "").await, (StatusCode::BAD_REQUEST, error));
}

/// The attempts 32 tasks get bumping one new key at once in `counters`.
async fn bumped_at_once(counters: &Arc<Counters>) -> BTreeSet<u64> {
    let (start, mut bumping) = (Arc::new(Barrier::new(32)), JoinSet::new());
    for _ in 0..32 {
        let (counters, start) = (Arc::clone(counters), Arc::clone(&start));
        bumping.spawn(async move {
            start.wait().await;
            counters.bump("same").await
        });
    }
    bumping.join_all().await.into_iter().map(Option::unwrap).collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn one_new_key_bumped_at_once_by_many_tasks_gets_one_count() {
    // The race is between tasks taking one free slot, so it is run on many fresh tables.
    for _ in 0..100 {
        let counters = Arc::new(Counters::new());
        assert_eq!(bumped_at_once(&counters).await, (1..=32).collect(), "each attempt has its own number");
        assert_eq!(counters.bump("same").await, Some(33), "in one slot");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn concurrent_requests_for_one_key_count_each_attempt_once() {
    let app = router();
    let mut requests = JoinSet::new();
    for _ in 0..32 {
        let app = app.clone();
        requests.spawn(async move { send(&app, Method::GET, "/flaky?key=race", "").await.1["attempt"].as_u64() });
    }
    let attempts: BTreeSet<u64> = requests.join_all().await.into_iter().map(Option::unwrap).collect();
    assert_eq!(attempts, (1..=32).collect());
}

#[tokio::test]
async fn poll_is_pending_until_the_attempt_it_is_done_after() {
    let app = router();
    for (n, status) in [(1, "pending"), (2, "pending"), (3, "done"), (4, "done")] {
        let polled = send(&app, Method::GET, "/poll/job?after=3", "").await;
        assert_eq!(polled, (StatusCode::OK, json!({"status": status, "attempt": n})));
    }
    assert_eq!(send(&app, Method::GET, "/poll/other", "").await.1, json!({"status": "done", "attempt": 1}));
    let bad = send(&app, Method::GET, "/poll/job?after=later", "").await;
    assert_eq!(bad, (StatusCode::BAD_REQUEST, json!({"error": "`after` is a whole number"})));
}

#[tokio::test]
async fn status_answers_any_code_from_200_to_599_by_any_method() {
    let app = router();
    for (method, code) in [(Method::GET, 200), (Method::POST, 201), (Method::DELETE, 418), (Method::PUT, 599)] {
        let status = StatusCode::from_u16(code).unwrap();
        assert_eq!(send(&app, method, &format!("/status/{code}"), "").await, (status, json!({"status": code})));
    }
    for code in [204, 205, 304] {
        let path = format!("/status/{code}");
        let request = Request::builder().uri(&path).body(Body::empty()).unwrap();
        let response = app.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status().as_u16(), code);
        assert!(body::to_bytes(response.into_body(), usize::MAX).await.unwrap().is_empty(), "{path} has no body");
    }
    let refused = (StatusCode::BAD_REQUEST, json!({"error": "the status is a number from 200 to 599"}));
    for bad in ["199", "600", "100", "70000", "-1", "ok"] {
        assert_eq!(send(&app, Method::GET, &format!("/status/{bad}"), "").await, refused, "{bad}");
    }
}

/// The raw response to `GET path` from `app` served on a socket, as hyper writes it.
async fn over_a_socket(app: axum::Router, path: &str) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await });
    let request = format!("GET {path} HTTP/1.1\r\nhost: demo\r\nconnection: close\r\n\r\n");
    let read = move || {
        use std::io::{Read, Write};
        let mut stream = std::net::TcpStream::connect(address).unwrap();
        stream.write_all(request.as_bytes()).unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        response.to_ascii_lowercase()
    };
    tokio::task::spawn_blocking(read).await.unwrap()
}

#[tokio::test]
async fn statuses_without_a_body_send_none_over_a_socket() {
    for (code, length) in [(204, None), (205, Some("content-length: 0")), (304, None)] {
        let response = over_a_socket(router(), &format!("/status/{code}")).await;
        let (head, body) = response.split_once("\r\n\r\n").unwrap();
        assert!(head.starts_with(&format!("http/1.1 {code}")), "{head}");
        assert_eq!(head.lines().find(|line| line.starts_with("content-length")), length, "{head}");
        assert_eq!(body, "", "{code}");
    }
}
