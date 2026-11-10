//! A small project on disk for the engine RPC tests. Its api points at a closed port, so a run
//! or call fails fast without a server, and its `.env` holds a declared secret that a var and a
//! `--set` value repeat, so a test can look for it in what the server sends.

use std::fs;
use std::io::{Read as _, Write as _};
use std::net::TcpListener;
use std::sync::Arc;
use std::thread;

use tempfile::TempDir;
use vc_engine::EngineBuilder;

use crate::EngineApi;

/// The secret's value: test data, in no real provider's key format.
pub const SECRET: &str = "tok-fixture-5c2e81";
pub const CONFIG: &str = "syntaxVersion: 2\nproject: shop\nvars:\n  baseUrl: 'http://127.0.0.1:9'\n  note: 'sent tok-fixture-5c2e81'\nprofiles:\n  - id: dev\n    default: true\n    agents: {run: true, call: [GET], writeDefinitions: true}\n  - id: bare\n";
pub const API: &str = "services/users/users.api.yaml";
pub const USERS: &str = "baseUrl: ${var.baseUrl}\napis:\n  - id: list\n    name: List users\n    path: /users\n  - id: create\n    name: Create a user\n    method: POST\n    path: /users\n";
pub const TESTS: &str = "services/users/crud.test.yaml";
pub const CRUD: &str =
    "tests:\n  - id: listsUsers\n    name: Lists every user\n    tags: [smoke]\n    call: users#list\n";

/// The project, and the API over its engine with no server layers.
pub fn project() -> (TempDir, Arc<EngineApi>) {
    let dir = tempfile::tempdir().unwrap();
    let files = [
        ("vibranium.config.yaml", CONFIG),
        (".env.schema", "SHOP_TOKEN=\n"),
        (".env", "SHOP_TOKEN=tok-fixture-5c2e81\n"),
        (API, USERS),
        (TESTS, CRUD),
    ];
    fs::create_dir_all(dir.path().join("services/users")).unwrap();
    for (path, text) in files {
        fs::write(dir.path().join(path), text).unwrap();
    }
    let engine = EngineBuilder::for_project(dir.path()).unwrap().build().unwrap();
    (dir, Arc::new(EngineApi::new(Arc::new(engine), Vec::new(), Vec::new())))
}

/// The body the large-body server answers with: past the default `history.maxBodySize` of 64 KiB.
pub const LARGE: usize = 70 * 1024;

/// A server on a free local port that answers every request with `status` and `LARGE` bytes; its
/// base URL.
pub fn large_body_server(status: u16) -> String {
    serving(status, vec![b'a'; LARGE])
}

/// A server on a free local port that answers every request with `status` and `body`; its base
/// URL.
pub fn serving(status: u16, body: Vec<u8>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    thread::spawn(move || {
        for mut socket in listener.incoming().flatten() {
            let mut request = [0_u8; 4096];
            let _ = socket.read(&mut request);
            let length = body.len();
            let head = format!("HTTP/1.1 {status} X\r\ncontent-length: {length}\r\nconnection: close\r\n\r\n");
            let _ = socket.write_all(&[head.as_bytes(), &body].concat());
        }
    });
    url
}
