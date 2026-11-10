//! A small project on disk for the engine RPC tests. Its api points at a closed port, so a run
//! or call fails fast without a server, and its `.env` holds a declared secret that a var and a
//! `--set` value repeat, so a test can look for it in what the server sends.

use std::fs;
use std::sync::Arc;

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
