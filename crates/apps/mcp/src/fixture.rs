//! A small project on disk for the tool tests. Its api points at a closed port, so a run or
//! call fails fast without a server.

use std::fs;
use std::sync::Arc;

use serde_json::Value;
use tempfile::TempDir;
use vc_engine::{Engine, EngineBuilder};

use crate::args::Args;
use crate::tools::{Cx, Tools};

pub const CONFIG: &str = "syntaxVersion: 2\nvars: {baseUrl: 'http://127.0.0.1:9'}\nprofiles:\n  - id: dev\n    default: true\n    agents: {run: true, call: [GET], writeDefinitions: true}\n  - id: bare\n";
pub const USERS: &str = "baseUrl: ${var.baseUrl}\napis:\n  - id: list\n    name: List users\n    path: /users\n  - id: create\n    name: Create a user\n    method: POST\n    path: /users\n";
pub const CRUD: &str = "tests:\n  - id: listsUsers\n    name: Lists every user\n    call: users#list\n";

/// The project, and its engine.
pub fn project() -> (TempDir, Arc<Engine>) {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("vibranium.config.yaml"), CONFIG).unwrap();
    fs::create_dir_all(dir.path().join("services/users")).unwrap();
    fs::write(dir.path().join("services/users/users.api.yaml"), USERS).unwrap();
    fs::write(dir.path().join("services/users/crud.test.yaml"), CRUD).unwrap();
    let engine = EngineBuilder::for_project(dir.path()).unwrap().build().unwrap();
    (dir, Arc::new(engine))
}

/// A call from the agent `claude` under `profile`.
pub fn cx(engine: &Arc<Engine>, profile: &str) -> Cx {
    Cx { engine: engine.clone(), agent: "claude".to_owned(), profiles: vec![profile.to_owned()], sets: Vec::new() }
}

/// `value` as tool arguments.
pub fn args(value: Value) -> Args {
    let Value::Object(map) = value else { panic!("not an object") };
    map
}

/// Runs the project's one test, which fails against the closed port; the run's number.
pub async fn failed_run(cx: &Cx) -> u64 {
    let tools = Tools::standard();
    let ran =
        tools.get("run_tests").unwrap().call(cx, &args(serde_json::json!({"ids": ["users.crud"]}))).await.unwrap();
    assert_eq!(ran["exit"], 1, "{ran}");
    ran["run"].as_u64().unwrap()
}
