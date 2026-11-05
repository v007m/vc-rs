//! End-to-end runs of the real `vc` binary against an in-process mock server: lifecycle blocks
//! and cleanups, retries, polls, loops, the per-host limit, the config layers, `--changed`,
//! `command:` actions, hooks, computed bodies and the report files. Each test gets its own project, home folder and
//! server.

#![cfg(test)]

mod changed;
mod command;
mod hooks;
mod layers;
mod lifecycle;
mod reports;
mod scheduling;
mod values;

use std::fs;
use std::path::Path;

use assert_cmd::Command;
use assert_cmd::cargo::cargo_bin_cmd;
use serde_json::Value;
use tempfile::TempDir;

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

/// A project on disk and a home folder of its own, so no user config leaks in.
pub(crate) struct Project {
    root: TempDir,
    home: TempDir,
}

impl Project {
    /// A project holding `files`.
    pub(crate) fn new(files: &[(&str, &str)]) -> Self {
        let project = Self { root: tempfile::tempdir().unwrap(), home: tempfile::tempdir().unwrap() };
        for (path, text) in files {
            project.write(path, text);
        }
        project
    }

    pub(crate) fn path(&self) -> &Path {
        self.root.path()
    }

    pub(crate) fn home(&self) -> &Path {
        self.home.path()
    }

    /// Writes `text` to `path` under the project root.
    pub(crate) fn write(&self, path: &str, text: &str) {
        let file = self.path().join(path);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(file, text).unwrap();
    }

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
