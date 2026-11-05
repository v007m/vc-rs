//! The redaction gate: a token sent in a `redact.headers` header, the query and a JSON body
//! (masked there by a `redact.rules` regex), and a `redact.fields` password, never reach the
//! history or a report file, even where `history.maxBodySize` cuts a body in the middle of one
//! (masked first, then cut: D184).

use std::fs;
use std::path::Path;

use rusqlite::types::ValueRef;
use vc_mock_server::MockServer;

use crate::{Project, said};

const SECRET: &str = "Sx7Q-9vLk-Tz3W-8pRn-Mq2D";
const PASSWORD: &str = "Pw9-Qz7x-Lm4k-Rt2v";
/// No run of this many characters of either value may show anywhere.
const WINDOW: usize = 6;

const CONFIG: (&str, &str) = (
    "vibranium.config.yaml",
    "syntaxVersion: 2\nprofiles:\n  - id: dev\n    default: true\nvars: {baseUrl: 'http://x'}\n\
     redact: {headers: [x-api-key], fields: [password], rules: [{regex: 'Sx7Q-[0-9A-Za-z-]{19}'}]}\n\
     history: {bodies: all, maxBodySize: 1kb}\n",
);

const API: (&str, &str) = (
    "services/echo/echo.api.yaml",
    "baseUrl: ${var.baseUrl}\napis:\n  - id: send\n    method: POST\n    path: /echo\n\
     \x20   vars: {pad: x, token: Sx7Q-9vLk-Tz3W-8pRn-Mq2D}\n    headers: {x-api-key: '${var.token}'}\n    query: {key: '${var.token}'}\n\
     \x20   body: {pad: '${var.pad}', password: Pw9-Qz7x-Lm4k-Rt2v, token: '${var.token}'}\n\
     \x20   expect: {status: 500}\n",
);

/// One row per padding length, so the 1kb cut sweeps across the password and the secret in both
/// the request body and the echoed response.
fn tests() -> String {
    let rows =
        (930..1020).step_by(3).map(|pad| format!("      - pad: {}\n", "p".repeat(pad))).collect::<Vec<_>>().concat();
    format!(
        "tests:\n  - id: sendsTheSecret\n    examples:\n{rows}    call: echo#send\n    vars: {{pad: '${{example.pad}}'}}\n"
    )
}

#[tokio::test(flavor = "multi_thread")]
async fn no_secret_reaches_the_history_or_a_report() {
    let server = MockServer::start().await.unwrap();
    let tests = tests();
    let project = Project::new(&[CONFIG, API, ("services/echo/send.test.yaml", &tests)]);
    let run = project.run(&server.base_url(), &["--report", "ctrf,junit,html"]).output().unwrap();
    assert_eq!(run.status.code(), Some(1), "{}", said(&run));
    let again = project.vc(&["report", "1", "--format", "ctrf,junit,html", "--out", "again"]).output().unwrap();
    assert_eq!(again.status.code(), Some(0), "{}", said(&again));

    let history = history_text(&project.path().join(".vibranium/history.db"));
    assert!(history.contains("••••") && history.contains("pppp"), "the bodies were stored, masked");
    let mut seen = vec![("history.db rows".to_owned(), history), ("vc output".to_owned(), said(&run) + &said(&again))];
    files(project.path().join(".vibranium").as_path(), &mut seen);
    files(project.path().join("again").as_path(), &mut seen);
    assert!(seen.iter().any(|(name, _)| name.ends_with("index.html")), "the HTML report was written");
    for (name, text) in &seen {
        for value in [SECRET, PASSWORD] {
            let leak =
                value.as_bytes().windows(WINDOW).find(|part| text.as_bytes().windows(WINDOW).any(|w| w == *part));
            assert!(leak.is_none(), "{name} holds part of a secret: {:?}", leak.map(String::from_utf8_lossy));
        }
    }
}

/// Every text and blob cell of every table, blobs unpacked when they are zstd frames.
fn history_text(path: &Path) -> String {
    let db = rusqlite::Connection::open(path).unwrap();
    let mut tables = db.prepare("SELECT name FROM sqlite_master WHERE type = 'table'").unwrap();
    let names: Vec<String> = tables.query_map([], |row| row.get(0)).unwrap().map(Result::unwrap).collect();
    let mut text = String::new();
    for table in names {
        let mut rows = db.prepare(&format!("SELECT * FROM \"{table}\"")).unwrap();
        let columns = rows.column_count();
        let mut cursor = rows.query([]).unwrap();
        while let Some(row) = cursor.next().unwrap() {
            (0..columns).for_each(|column| text.push_str(&cell(row.get_ref(column).unwrap())));
        }
    }
    text
}

fn cell(value: ValueRef<'_>) -> String {
    match value {
        ValueRef::Text(bytes) => String::from_utf8_lossy(bytes).into_owned() + "\n",
        ValueRef::Blob(bytes) => {
            let unpacked = zstd::stream::decode_all(bytes).unwrap_or_else(|_| bytes.to_vec());
            String::from_utf8_lossy(&unpacked).into_owned() + "\n"
        }
        _ => String::new(),
    }
}

/// Every file under `dir`, as text: the history files' raw bytes and each report file. The
/// compile cache is left out: it holds the project's own source, where the values are written.
fn files(dir: &Path, seen: &mut Vec<(String, String)>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.ends_with(".vibranium/cache") {
            continue;
        }
        if path.is_dir() {
            files(&path, seen);
        } else {
            seen.push((path.display().to_string(), String::from_utf8_lossy(&fs::read(&path).unwrap()).into_owned()));
        }
    }
}
