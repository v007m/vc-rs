//! The demo's test routes, as the mock serves them: `/slow`, `/flaky`, `/status/{code}` and
//! `/poll/{id}`. Each bounds what it waits for or keeps, and refuses more with 400 (D329).

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{any, get};
use axum::{Json, Router};
use serde_json::{Value, json};
use tokio::sync::OnceCell;

/// The longest `/slow` waits, in milliseconds.
pub const MAX_SLEEP_MS: u64 = 10_000;

/// The longest `/flaky` key or `/poll` id, in bytes.
pub const MAX_KEY_BYTES: usize = 64;

/// The most keys `/flaky` counts, and the most ids `/poll` counts.
pub const MAX_KEYS: usize = 1024;

/// A query string, by name.
type Params = BTreeMap<String, String>;

/// Attempt counts per key, in a table of [`MAX_KEYS`] slots: a key takes the first free slot from
/// where its hash points, and keeps it, so the memory held never grows.
pub struct Counters {
    slots: Box<[Slot]>,
}

/// A key, set once, and its count: a slot filled once, as the LLD's shared slots are (D113).
#[derive(Default)]
struct Slot {
    key: OnceCell<String>,
    count: AtomicU64,
}

impl Counters {
    /// A table with every slot free.
    pub fn new() -> Self {
        Self { slots: (0..MAX_KEYS).map(|_| Slot::default()).collect() }
    }

    /// Counts one more attempt for `key`: the attempt's number, from 1, or `None` when every slot
    /// holds another key.
    pub async fn bump(&self, key: &str) -> Option<u64> {
        for slot in self.slots.iter().cycle().skip(start_of(key)).take(MAX_KEYS) {
            if slot.holds(key).await {
                return Some(slot.count.fetch_add(1, Ordering::Relaxed) + 1);
            }
        }
        None
    }
}

impl Slot {
    /// Whether the slot is `key`'s, taking it when it is free. Of two requests taking one free
    /// slot at once, one sets it and the other waits and reads that key back: the slot is the
    /// second one's only when the keys are the same, so a key never gets two counts.
    async fn holds(&self, key: &str) -> bool {
        self.key.get_or_init(|| async { key.to_owned() }).await == key
    }
}

/// Where `key`'s probe starts (FNV-1a over its bytes).
fn start_of(key: &str) -> usize {
    let hash = key
        .bytes()
        .fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| (hash ^ u64::from(byte)).wrapping_mul(0x100_0000_01b3));
    usize::try_from(hash >> 32).unwrap_or_default() % MAX_KEYS
}

/// The counts the test routes keep: `/flaky`'s by key and `/poll`'s by id.
struct Tables {
    flaky: Counters,
    polls: Counters,
}

/// A request the test routes refuse: 400 with `{"error": …}`.
struct Refused(String);

impl IntoResponse for Refused {
    fn into_response(self) -> Response {
        (StatusCode::BAD_REQUEST, Json(json!({"error": self.0}))).into_response()
    }
}

/// The test routes, each with its own state.
pub fn routes() -> Router {
    let tables = Arc::new(Tables { flaky: Counters::new(), polls: Counters::new() });
    Router::new()
        .route("/slow", get(slow))
        .route("/flaky", get(flaky))
        .route("/poll/{id}", get(poll))
        .route("/status/{code}", any(status))
        .with_state(tables)
}

/// `GET /slow?ms=n`: `{"sleptMs": n}` after `n` milliseconds (0 when not given), at most
/// [`MAX_SLEEP_MS`].
async fn slow(Query(query): Query<Params>) -> Result<Json<Value>, Refused> {
    let ms = number(&query, "ms")?;
    if ms > MAX_SLEEP_MS {
        return Err(Refused(format!("`ms` is at most {MAX_SLEEP_MS}")));
    }
    tokio::time::sleep(Duration::from_millis(ms)).await;
    Ok(Json(json!({"sleptMs": ms})))
}

/// `GET /flaky?key=k&failTimes=n`: the first `n` attempts per key get 503 and later ones 200, each
/// with `{"attempt": a}`.
async fn flaky(State(tables): State<Arc<Tables>>, Query(query): Query<Params>) -> Result<Response, Refused> {
    let fail_times = number(&query, "failTimes")?;
    let key = query.get("key").map_or("", String::as_str);
    let attempt = count(&tables.flaky, key, "key").await?;
    let status = if attempt <= fail_times { StatusCode::SERVICE_UNAVAILABLE } else { StatusCode::OK };
    Ok((status, Json(json!({"attempt": attempt}))).into_response())
}

/// `GET /poll/{id}?after=n`: `pending` until the `n`th attempt for that id, `done` from then on,
/// each with `{"status": s, "attempt": a}`.
async fn poll(
    State(tables): State<Arc<Tables>>,
    Path(id): Path<String>,
    Query(query): Query<Params>,
) -> Result<Json<Value>, Refused> {
    let after = number(&query, "after")?;
    let attempt = count(&tables.polls, &id, "id").await?;
    let status = if attempt >= after { "done" } else { "pending" };
    Ok(Json(json!({"status": status, "attempt": attempt})))
}

/// `/status/{code}`, by any method: that status with `{"status": code}`, for 200 to 599.
async fn status(Path(code): Path<String>) -> Result<Response, Refused> {
    let wanted = code.parse::<u16>().ok().filter(|code| (200..=599).contains(code));
    let status = wanted.and_then(|code| StatusCode::from_u16(code).ok());
    let status = status.ok_or_else(|| Refused("the status is a number from 200 to 599".to_owned()))?;
    Ok((status, Json(json!({"status": status.as_u16()}))).into_response())
}

/// The attempt for `key` in `counters`, refused when the key is too long or the table is full.
async fn count(counters: &Counters, key: &str, what: &str) -> Result<u64, Refused> {
    if key.len() > MAX_KEY_BYTES {
        return Err(Refused(format!("the {what} is at most {MAX_KEY_BYTES} bytes")));
    }
    let full = || Refused(format!("the demo counts at most {MAX_KEYS} of these; restart it to count afresh"));
    counters.bump(key).await.ok_or_else(full)
}

/// The whole number `name` names in `query`, 0 when it is not given.
fn number(query: &Params, name: &str) -> Result<u64, Refused> {
    let parse = |text: &String| text.parse().map_err(|_| Refused(format!("`{name}` is a whole number")));
    query.get(name).map_or(Ok(0), parse)
}
