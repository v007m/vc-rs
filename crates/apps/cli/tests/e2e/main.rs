//! End-to-end runs of the real `vc` binary against an in-process mock server: auth, lifecycle
//! blocks and cleanups, retries and backoff, polls, loops, the per-host limit, timing checks, load
//! thresholds, the config layers, `--changed`, `command:` actions, hooks, computed bodies, file,
//! form and multipart bodies, the report files, `vc call`, `vc explain`, the completion scripts,
//! `vc ui`, `vc sync` with `source.headers`, and the docs playground printing what `vc run` prints.
//! Each test gets its own project, home folder and server.

#![cfg(test)]

mod alerts;
mod allowed;
mod approvals;
mod auth;
mod bodies;
mod cache;
mod call;
mod changed;
mod command;
mod completion;
mod demo;
mod examples;
mod explain;
mod generated;
mod history;
mod hooks;
mod import;
mod labels;
mod layers;
mod lifecycle;
mod load;
mod mcp;
mod migrate;
mod paths;
mod playground;
mod plugin;
mod redaction;
mod reports;
mod scheduling;
mod spec_headers;
mod teardown;
mod timing;
mod tui;
mod ui;
mod values;

#[path = "../common/port.rs"]
mod port;
#[path = "../common/project.rs"]
mod project;

use std::path::Path;

use assert_cmd::Command;
use assert_cmd::cargo::cargo_bin_cmd;
pub(crate) use project::Project;
use serde_json::Value;

/// The config every project starts from: a default profile and the base URL `--set` replaces.
pub(crate) const CONFIG: (&str, &str) = (
    "vibranium.config.yaml",
    "syntaxVersion: 2\nprofiles:\n  - id: dev\n    default: true\nvars: {baseUrl: 'http://x', word: x}\n",
);

/// `users#list`, and `users#create`, whose cleanup is `users#remove`.
pub(crate) const USERS: (&str, &str) = (
    "services/users/users.api.yaml",
    concat!(
        "baseUrl: ${var.baseUrl}\napis:\n",
        "  - id: list\n    path: /users\n",
        "  - id: create\n    method: POST\n    path: /users\n    body: {name: someone}\n",
        "    expect: {status: 201}\n    cleanup:\n      call: remove\n      vars: {userId: '${response.body.id}'}\n",
        "  - id: remove\n    method: DELETE\n    path: /users/${var.userId}\n    vars: {userId: 0}\n",
        "    expect: {status: 204}\n",
    ),
);

/// The same three apis over `/orders`.
pub(crate) const ORDERS: (&str, &str) = (
    "services/orders/orders.api.yaml",
    concat!(
        "baseUrl: ${var.baseUrl}\napis:\n",
        "  - id: list\n    path: /orders\n",
        "  - id: create\n    method: POST\n    path: /orders\n    body: {item: book}\n",
        "    expect: {status: 201}\n    cleanup:\n      call: remove\n      vars: {orderId: '${response.body.id}'}\n",
        "  - id: remove\n    method: DELETE\n    path: /orders/${var.orderId}\n    vars: {orderId: 0}\n",
        "    expect: {status: 204}\n",
    ),
);

/// `scripted#slow` (`/slow?ms=`), `scripted#flaky`, `scripted#poll` and `scripted#echo`.
pub(crate) const SCRIPTED: (&str, &str) = (
    "services/scripted/scripted.api.yaml",
    concat!(
        "baseUrl: ${var.baseUrl}\napis:\n",
        "  - id: slow\n    path: /slow\n    vars: {ms: 0}\n    query: {ms: '${var.ms}'}\n",
        "  - id: flaky\n    path: /flaky\n    vars: {key: k}\n    query: {key: '${var.key}', failTimes: '1'}\n",
        "  - id: poll\n    path: /poll/job\n    query: {after: '3'}\n",
        "  - id: echo\n    method: POST\n    path: /echo\n",
    ),
);

impl Project {
    /// `vc <args>` in the project, with no colour, its own home and no `VIB_*` from outside.
    pub(crate) fn vc(&self, args: &[&str]) -> Command {
        let mut vc = cargo_bin_cmd!("vc");
        vc.current_dir(self.path()).args(args).env("NO_COLOR", "1");
        isolate(&mut vc, self.home());
        vc
    }

    /// `vc run --set vars.baseUrl=<base> <args>`.
    pub(crate) fn run(&self, base: &str, args: &[&str]) -> Command {
        let base = format!("vars.baseUrl={base}");
        let mut vc = self.vc(&["run", "--set", &base]);
        vc.args(args);
        vc
    }
}

fn isolate(vc: &mut Command, home: &Path) {
    vc.env("HOME", home).env("XDG_CONFIG_HOME", home).env_remove("APPDATA");
    for (name, _) in std::env::vars_os() {
        if name.to_string_lossy().starts_with("VIB_") {
            vc.env_remove(name);
        }
    }
}

/// A copy of this build's `name` binary alone in a new folder, so it finds nothing beside it.
pub(crate) fn alone(name: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    let folder = tempfile::tempdir().unwrap();
    let file = format!("{name}{}", std::env::consts::EXE_SUFFIX);
    let built = std::path::Path::new(env!("CARGO_BIN_EXE_vc")).with_file_name(&file);
    let copy = folder.path().join(file);
    std::fs::copy(built, &copy).unwrap();
    (folder, copy)
}

/// The JSON lines a `--reporter json` run printed.
pub(crate) fn events(stdout: &[u8]) -> Vec<Value> {
    let text = String::from_utf8_lossy(stdout);
    text.lines().filter_map(|line| serde_json::from_str(line).ok()).collect()
}

/// The `event` lines of `kind`.
pub(crate) fn of_kind<'e>(events: &'e [Value], kind: &str) -> Vec<&'e Value> {
    events.iter().filter(|event| event["event"] == kind).collect()
}

/// Stdout and stderr together, for messages that may go to either.
pub(crate) fn said(output: &std::process::Output) -> String {
    format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr))
}
