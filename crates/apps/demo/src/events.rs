//! The demo's `/events`: the mock's server-sent events, capped (D426, D329). It holds no stream
//! open past its last event, and refuses a count over [`MAX_EVENTS`] or a query it cannot read
//! with 400 and a JSON error.

use std::io;

use axum::Router;
use axum::body::{Body, Bytes};
use axum::extract::Query;
use axum::http::{HeaderMap, HeaderValue, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use futures_util::stream::{self, StreamExt};

use crate::scripted::{Asked, Params, Refused, number};

/// The most events one `/events` request sends.
pub const MAX_EVENTS: u64 = 100;

/// The longest `retry` it sends, in milliseconds.
pub const MAX_RETRY_MS: u64 = 60_000;

const DEFAULT_COUNT: u64 = 3;

pub fn routes() -> Router {
    Router::new().route("/events", get(events))
}

/// What one request asked for.
struct Asking {
    after: u64,
    last: u64,
    dropped: bool,
    retry: Option<u64>,
    heartbeat: bool,
}

/// `GET /events?count=n` sends `id: i` and `data: i` for each `i` from 1 to `n` (3 when left
/// out), after the `last-event-id` number when one is sent, then ends; `drop=true` drops the
/// connection instead, `dropAt=k` drops a first connection after event `k`, `retry=ms` sends a
/// `retry` field first and `heartbeat=true` a comment before each event.
async fn events(headers: HeaderMap, query: Asked) -> Result<Response, Refused> {
    let Query(query) = query?;
    let asking = asking(&headers, &query)?;
    let heartbeat = asking.heartbeat;
    let retry = stream::iter(asking.retry.map(|ms| Ok(Bytes::from(format!("retry: {ms}\n\n")))));
    let sent = stream::iter(asking.after.saturating_add(1)..=asking.last).map(move |i| Ok(event(i, heartbeat)));
    let dropped = stream::iter(asking.dropped.then_some(())).then(|()| dropped());
    let body = Body::from_stream(retry.chain(sent).chain(dropped));
    let stream_headers = [
        (header::CONTENT_TYPE, HeaderValue::from_static("text/event-stream")),
        (header::CACHE_CONTROL, HeaderValue::from_static("no-cache")),
    ];
    Ok((stream_headers, body).into_response())
}

fn asking(headers: &HeaderMap, query: &Params) -> Result<Asking, Refused> {
    let count = if query.contains_key("count") { number(query, "count")? } else { DEFAULT_COUNT };
    if count > MAX_EVENTS {
        return Err(Refused(format!("`count` is at most {MAX_EVENTS}")));
    }
    let retry = query.contains_key("retry").then(|| number(query, "retry")).transpose()?;
    if retry.is_some_and(|ms| ms > MAX_RETRY_MS) {
        return Err(Refused(format!("`retry` is at most {MAX_RETRY_MS}")));
    }
    let resumed = headers.get("last-event-id").map(|value| value.to_str().ok().and_then(|id| id.trim().parse().ok()));
    let drop_at = query.contains_key("dropAt").then(|| number(query, "dropAt")).transpose()?;
    let (last, dropped) = match drop_at {
        Some(at) if resumed.is_none() => (at.min(count), true),
        _ => (count, flag(query, "drop")?),
    };
    let after = resumed.flatten().unwrap_or(0);
    Ok(Asking { after, last, dropped, retry, heartbeat: flag(query, "heartbeat")? })
}

fn flag(query: &Params, name: &str) -> Result<bool, Refused> {
    match query.get(name).map(String::as_str) {
        None | Some("false") => Ok(false),
        Some("true") => Ok(true),
        Some(_) => Err(Refused(format!("`{name}` is true or false"))),
    }
}

/// The error that drops the connection, after a yield: with every event ready at once, hyper
/// would otherwise meet the error before it writes the head, and send nothing at all.
async fn dropped() -> Result<Bytes, io::Error> {
    tokio::task::yield_now().await;
    Err(io::Error::other("the demo dropped the stream"))
}

fn event(i: u64, heartbeat: bool) -> Bytes {
    let comment = if heartbeat { ": heartbeat\n" } else { "" };
    Bytes::from(format!("{comment}id: {i}\ndata: {i}\n\n"))
}

#[cfg(test)]
#[path = "events_tests.rs"]
mod tests;
