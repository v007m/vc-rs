use std::io;

use axum::Router;
use axum::body::{Body, Bytes};
use axum::extract::Query;
use axum::http::{HeaderMap, HeaderValue, header};
use axum::response::{IntoResponse, Response};
use axum::routing;
use futures_util::stream::{self, StreamExt};
use serde::Deserialize;

/// `GET /events`: the stream suite's server-sent events (D426).
pub fn routes() -> Router {
    Router::new().route("/events", routing::get(events))
}

const fn three() -> u64 {
    3
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct EventsQuery {
    #[serde(default = "three")]
    count: u64,
    #[serde(default)]
    drop: bool,
    drop_at: Option<u64>,
    retry: Option<u64>,
    #[serde(default)]
    heartbeat: bool,
    #[serde(default)]
    hold: bool,
}

/// How the body ends after its last event.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Ending {
    End,
    Drop,
    Hold,
}

/// `GET /events?count=n` sends `id: i` and `data: i` for each `i` from 1 to `n`, after the
/// `last-event-id` number when one is sent, then ends. `drop=true` drops the connection instead
/// of ending; `dropAt=k` drops a first connection (no `last-event-id`) after event `k`, and a
/// resumed one goes on to `n` and ends; `hold=true` keeps it open. `retry=ms` sends a `retry`
/// field first, and `heartbeat=true` a `: heartbeat` comment before each event.
async fn events(headers: HeaderMap, Query(query): Query<EventsQuery>) -> Response {
    let resumed = headers.get("last-event-id").map(|value| value.to_str().ok().and_then(|id| id.trim().parse().ok()));
    let after: u64 = resumed.flatten().unwrap_or(0);
    let (last, ending) = match query.drop_at {
        Some(at) if resumed.is_none() => (at.min(query.count), Ending::Drop),
        _ => (query.count, ending(&query)),
    };
    let heartbeat = query.heartbeat;
    let retry = stream::iter(query.retry.map(|ms| Ok(Bytes::from(format!("retry: {ms}\n\n")))));
    let sent = stream::iter(after.saturating_add(1)..=last).map(move |i| Ok(event(i, heartbeat)));
    let dropped = stream::iter((ending == Ending::Drop).then_some(())).then(|()| dropped("the mock"));
    let events = retry.chain(sent).chain(dropped);
    let body = if ending == Ending::Hold {
        Body::from_stream(events.chain(stream::pending()))
    } else {
        Body::from_stream(events)
    };
    let stream_headers = [
        (header::CONTENT_TYPE, HeaderValue::from_static("text/event-stream")),
        (header::CACHE_CONTROL, HeaderValue::from_static("no-cache")),
    ];
    (stream_headers, body).into_response()
}

const fn ending(query: &EventsQuery) -> Ending {
    if query.drop {
        Ending::Drop
    } else if query.hold {
        Ending::Hold
    } else {
        Ending::End
    }
}

/// The error that drops the connection, after a yield: with every event ready at once, hyper
/// would otherwise meet the error before it writes the head, and send nothing at all.
async fn dropped(by: &str) -> Result<Bytes, io::Error> {
    tokio::task::yield_now().await;
    Err(io::Error::other(format!("{by} dropped the stream")))
}

fn event(i: u64, heartbeat: bool) -> Bytes {
    let comment = if heartbeat { ": heartbeat\n" } else { "" };
    Bytes::from(format!("{comment}id: {i}\ndata: {i}\n\n"))
}
