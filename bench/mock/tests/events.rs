//! `/events`: the stream suite's contract, the reconnect drop and the opt-in fields (D426).

mod support;

use std::time::Duration;

use reqwest::header::{ACCEPT_ENCODING, CONTENT_ENCODING, CONTENT_TYPE, HeaderValue};
use reqwest::{RequestBuilder, Response};
use support::{Mock, TestResult};

/// The body read chunk by chunk until it ends, and whether it ended in an error.
async fn read(request: RequestBuilder) -> Result<(String, bool), Box<dyn std::error::Error>> {
    let mut response: Response = request.send().await?;
    let mut text = String::new();
    loop {
        match response.chunk().await {
            Ok(Some(chunk)) => text.push_str(std::str::from_utf8(&chunk)?),
            Ok(None) => return Ok((text, false)),
            Err(_) => return Ok((text, true)),
        }
    }
}

#[tokio::test]
async fn events_send_ids_and_data_then_end() -> TestResult {
    let mock = Mock::start().await?;
    let response = mock.get("/events?count=3").header(ACCEPT_ENCODING, "gzip").send().await?;
    assert_eq!(response.headers().get(CONTENT_TYPE).map(HeaderValue::as_bytes), Some(&b"text/event-stream"[..]));
    assert!(response.headers().get(CONTENT_ENCODING).is_none(), "an event stream is never compressed");
    assert_eq!(response.text().await?, "id: 1\ndata: 1\n\nid: 2\ndata: 2\n\nid: 3\ndata: 3\n\n");
    assert_eq!(read(mock.get("/events")).await?.0.matches("data:").count(), 3, "three events when no count is given");
    assert_eq!(mock.handle.stats().await?.count("GET /events"), 2);
    Ok(())
}

#[tokio::test]
async fn a_last_event_id_resumes_after_it() -> TestResult {
    let mock = Mock::start().await?;
    let (text, failed) = read(mock.get("/events?count=4").header("last-event-id", "2")).await?;
    assert_eq!((text.as_str(), failed), ("id: 3\ndata: 3\n\nid: 4\ndata: 4\n\n", false));
    let (text, _) = read(mock.get("/events?count=2").header("last-event-id", "2")).await?;
    assert_eq!(text, "", "nothing is left after the last id");
    let (text, _) = read(mock.get("/events?count=1").header("last-event-id", "x")).await?;
    assert_eq!(text, "id: 1\ndata: 1\n\n", "an id that is not a number starts from the first event");
    Ok(())
}

#[tokio::test]
async fn drop_ends_the_stream_in_an_error_after_its_events() -> TestResult {
    let mock = Mock::start().await?;
    let (text, failed) = read(mock.get("/events?count=2&drop=true")).await?;
    assert_eq!((text.as_str(), failed), ("id: 1\ndata: 1\n\nid: 2\ndata: 2\n\n", true));
    Ok(())
}

#[tokio::test]
async fn drop_at_drops_only_the_first_connection() -> TestResult {
    let mock = Mock::start().await?;
    let (text, failed) = read(mock.get("/events?count=4&dropAt=2")).await?;
    assert_eq!((text.as_str(), failed), ("id: 1\ndata: 1\n\nid: 2\ndata: 2\n\n", true));
    let resumed = mock.get("/events?count=4&dropAt=2").header("last-event-id", "2");
    let (text, failed) = read(resumed).await?;
    assert_eq!((text.as_str(), failed), ("id: 3\ndata: 3\n\nid: 4\ndata: 4\n\n", false));
    let (text, failed) = read(mock.get("/events?count=1&dropAt=5")).await?;
    assert_eq!((text.as_str(), failed), ("id: 1\ndata: 1\n\n", true), "a drop past the count drops after the last");
    Ok(())
}

#[tokio::test]
async fn retry_and_heartbeat_are_sent_only_when_asked() -> TestResult {
    let mock = Mock::start().await?;
    let (text, _) = read(mock.get("/events?count=2&retry=1500&heartbeat=true")).await?;
    assert_eq!(text, "retry: 1500\n\n: heartbeat\nid: 1\ndata: 1\n\n: heartbeat\nid: 2\ndata: 2\n\n");
    Ok(())
}

#[tokio::test]
async fn hold_keeps_the_stream_open_after_its_events() -> TestResult {
    let mock = Mock::start().await?;
    let mut response = mock.get("/events?count=1&hold=true").send().await?;
    let first = response.chunk().await?.unwrap_or_default();
    assert_eq!(&first[..], b"id: 1\ndata: 1\n\n");
    let next = tokio::time::timeout(Duration::from_millis(200), response.chunk()).await;
    assert!(next.is_err(), "the stream stays open and silent: {next:?}");
    Ok(())
}
