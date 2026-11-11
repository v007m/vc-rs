//! The HTML report's data (D169): the keys `report.js` loads with `VIB.get` — `summary`, `index`,
//! `tests/<shard>` and `bodies/<hash>`, each body stored once under its BLAKE3 hash.

use std::collections::BTreeMap;

use serde_json::{Map, Value, json};
use vc_model::Location;

use crate::view::{CheckView, ExchangeView, RunSnapshot, TestView};
use crate::{encode, text};

/// Tests per `tests/<shard>` key: the detail pane loads one shard at a time.
pub const SHARD: usize = 500;

/// Hex digits of a body's BLAKE3 hash used as its key.
const HASH_LEN: usize = 32;

/// Every key of the report's data, in the order they are written.
pub fn entries(snapshot: &RunSnapshot) -> Vec<(String, Value)> {
    let files = files(snapshot);
    let mut bodies = BTreeMap::new();
    let mut entries =
        vec![("summary".to_owned(), summary(snapshot, &files)), ("index".to_owned(), index(snapshot, &files))];
    for (number, shard) in snapshot.tests.chunks(SHARD).enumerate() {
        let tests: Map<String, Value> = shard
            .iter()
            .enumerate()
            .map(|(at, view)| ((number * SHARD + at).to_string(), test(view, &mut bodies)))
            .collect();
        entries.push((format!("tests/{number}"), Value::Object(tests)));
    }
    entries.extend(bodies.into_iter().map(|(hash, body)| (format!("bodies/{hash}"), Value::String(body))));
    entries
}

/// The files in the order they started, then any file only a test names.
fn files(snapshot: &RunSnapshot) -> Vec<String> {
    let mut files = snapshot.files.clone();
    for view in &snapshot.tests {
        if !files.contains(&view.file) {
            files.push(view.file.clone());
        }
    }
    files
}

/// The run, its totals and lifecycle, and its files; a profile or file with a `name` is shown
/// with it: `dev "Local dev"`, `users.crud — Users CRUD`.
fn summary(snapshot: &RunSnapshot, files: &[String]) -> Value {
    let run = &snapshot.run;
    let titled =
        |id: &String| snapshot.profile_names.get(id).map_or_else(|| id.clone(), |name| format!("{id} \"{name}\""));
    let profiles = run.profiles.iter().map(titled);
    let files: Vec<String> = files.iter().map(|file| file_title(snapshot, file)).collect();
    json!({
        "run": {
            "id": run.id,
            "status": encode::run_status(run.status),
            "startedMs": run.started_ms,
            "durationMs": run.duration_ms,
            "profiles": profiles.collect::<Vec<_>>(),
            "seed": run.seed,
        },
        "totals": encode::totals(run.totals),
        "lifecycle": lifecycle(snapshot),
        "files": files,
        "shardSize": SHARD,
    })
}

/// A file's ID, followed by its `name` when it gives one.
fn file_title(snapshot: &RunSnapshot, file: &str) -> String {
    let name = snapshot.tests.iter().find(|view| view.file == file).and_then(|view| view.file_name.as_deref());
    name.map_or_else(|| file.to_owned(), |name| format!("{file} — {name}"))
}

/// Lifecycle actions, cleanups and abandoned cleanups as one list of lines.
fn lifecycle(snapshot: &RunSnapshot) -> Vec<Value> {
    let line = |label: String, status: &str, message: Option<&String>| json!({"label": label, "status": status, "message": message});
    let steps = snapshot
        .lifecycle
        .iter()
        .map(|step| line(step.action.clone(), &step.status, step.failure.as_ref().map(|failed| &failed.message)));
    let cleanups = snapshot.cleanups.iter().map(|cleanup| {
        let message = cleanup.failure.as_ref().map(|failed| &failed.message);
        line(format!("cleanup {}", cleanup.action), &cleanup.status, message)
    });
    let abandoned = snapshot.abandoned.iter().map(|gone| line(format!("cleanup {}", gone.action), "abandoned", None));
    steps.chain(cleanups).chain(abandoned).collect()
}

/// One row per test: `[label, name, file, status, ms, tags]`; the label is unique per instance,
/// and the name, shown in its place, is the label followed by the test's `name`.
fn index(snapshot: &RunSnapshot, files: &[String]) -> Value {
    let rows = snapshot.tests.iter().map(|view| {
        let file = files.iter().position(|file| *file == view.file);
        let name = view.name.as_ref().map(|_| view.titled());
        json!([view.label, name, file, text::status(view.status), view.duration_ms(), view.tags])
    });
    Value::Array(rows.collect())
}

fn test(view: &TestView, bodies: &mut BTreeMap<String, String>) -> Value {
    let checks: Vec<Value> = view.checks().map(check).collect();
    let exchanges: Vec<Value> = view.exchanges.iter().map(|exchange| self::exchange(exchange, bodies)).collect();
    json!({
        "location": view.location.as_ref().map(place),
        "blockedBy": view.blocked_by,
        "skipReason": view.skip_reason,
        "message": if view.detail.is_empty() { view.message.clone() } else { Some(view.detail.clone()) },
        "checks": checks,
        "exchanges": exchanges,
    })
}

fn check(check: &CheckView) -> Value {
    json!({
        "label": check.key,
        "passed": check.passed,
        "expected": text::value(&check.expected),
        "actual": check.actual.as_ref().map(text::value),
        "location": check.location.as_ref().map(place),
    })
}

fn exchange(exchange: &ExchangeView, bodies: &mut BTreeMap<String, String>) -> Value {
    let mut keep = |body: &Option<String>| body.as_ref().map(|text| stored(text, bodies));
    json!({
        "action": exchange.action,
        "method": exchange.method,
        "url": exchange.url,
        "status": exchange.status,
        "ms": exchange.timing.total,
        "requestHeaders": exchange.request_headers,
        "requestBody": keep(&exchange.request_body),
        "responseHeaders": exchange.response_headers,
        "responseBody": keep(&exchange.response_body),
        "truncated": exchange.truncated,
    })
}

/// Stores `body` once and returns its key.
fn stored(body: &str, bodies: &mut BTreeMap<String, String>) -> String {
    let hex = blake3::hash(body.as_bytes()).to_hex();
    let hash = hex.as_str().get(..HASH_LEN).unwrap_or_default().to_owned();
    bodies.entry(hash.clone()).or_insert_with(|| body.to_owned());
    hash
}

fn place(at: &Location) -> String {
    format!("{}:{}", at.path, at.line)
}
