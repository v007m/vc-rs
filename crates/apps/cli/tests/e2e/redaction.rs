//! The redaction gate: a secret sent in a header, the query and a JSON body, and a `redact.fields`
//! password, never reach the history or a report file, even where `history.maxBodySize` cuts a
//! body in the middle of one (the executor masks first, then cuts).

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
     redact: {fields: [password]}\nhistory: {bodies: all, maxBodySize: 1kb}\n",
);

const API: (&str, &str) = (
    "services/echo/echo.api.yaml",
    "baseUrl: ${var.baseUrl}\napis:\n  - id: send\n    method: POST\n    path: /echo\n    vars: {pad: x}\n\
     \x20   headers: {x-api-key: '${secret.API_TOKEN}'}\n    query: {key: '${secret.API_TOKEN}'}\n\
     \x20   body: {pad: '${var.pad}', password: Pw9-Qz7x-Lm4k-Rt2v, token: '${secret.API_TOKEN}'}\n\
     \x20   expect: {status: 500}\n",
);

/// One row per padding length, so the 1kb cut sweeps across the password and the secret in both
/// the request body and the echoed response.
fn tests() -> String {
    let rows: String = (930..1020).step_by(3).map(|pad| format!("      - pad: {}\n", "p".repeat(pad))).collect();
    format!("tests:\n  - id: sendsTheSecret\n    examples:\n{rows}    call: echo#send\n    vars: {{pad: '${{example.pad}}'}}\n")
}

#[tokio::test(flavor = "multi_thread")]
async fn no_secret_reaches_the_history_or_a_report() {
    let server = MockServer::start().await.unwrap();
    let tests = tests();
    let schema = (".env.schema", "API_TOKEN=\n");
    let env = format!("API_TOKEN={SECRET}\n");
    let project = Project::new(&[CONFIG, API, schema, (".env", &env), ("services/echo/send.test.yaml", &tests)]);
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
            let leak = value.as_bytes().windows(WINDOW).find(|part| text.as_bytes().windows(WINDOW).any(|w| w == *part));
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

/// Every file under `dir`, as text: the history files' raw bytes and each report file.
fn files(dir: &Path, seen: &mut Vec<(String, String)>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            files(&path, seen);
        } else {
            seen.push((path.display().to_string(), String::from_utf8_lossy(&fs::read(&path).unwrap()).into_owned()));
        }
    }
}
