//! M5 acceptance: the commands that write definitions (`init`, `new`, `rename`, `fmt`,
//! `check --fix`, `skip`/`unskip`, `call --save-as-test`) and the ones that help while writing
//! them (`explain`, `config explain`, `watch`, `completion`, `demo`). Every writing command goes
//! through `parser::edit` and the engine's compile guard (D107): it keeps the bytes it does not
//! change, and it refuses an edit that would add an error. Each test gets its own project and
//! home folder.

#![cfg(test)]

mod call;
mod completion;
mod config;
mod demo;
mod explain;
mod fix;
mod fmt;
mod init;
mod new;
mod new_vars;
mod rename;
mod skip;
mod watch;

#[path = "../common/port.rs"]
mod port;
#[path = "../common/project.rs"]
mod project;

use std::ffi::OsString;
use std::fs;

use assert_cmd::Command;
use assert_cmd::cargo::cargo_bin;
pub(crate) use project::Project;

/// A config with one profile and the base URL tests point at their server.
pub(crate) const CONFIG: (&str, &str) = (
    "vibranium.config.yaml",
    "syntaxVersion: 2\nprofiles:\n  - id: dev\n    default: true\nvars: {baseUrl: 'http://x'}\n",
);

/// `users#list`, and `users#create`, whose cleanup is `users#remove`, with comments to keep.
pub(crate) const USERS: (&str, &str) = (
    "services/users/users.api.yaml",
    concat!(
        "# the users service\n",
        "baseUrl: ${var.baseUrl}\n",
        "apis:\n",
        "  # every user\n",
        "  - id: list\n    path: /users\n",
        "  - id: create   # makes one\n    method: POST\n    path: /users\n    body: {name: someone}\n",
        "    expect: {status: 201}\n    cleanup:\n      call: remove\n      vars: {userId: '${response.body.id}'}\n",
        "  - id: remove\n    method: DELETE\n    path: /users/${var.userId}\n    vars: {userId: 0}\n",
        "    expect: {status: 204}\n",
    ),
);

/// `users.crud`: one test per api, referring to them by full ID.
pub(crate) const CRUD: (&str, &str) = (
    "services/users/crud.test.yaml",
    concat!(
        "tests:\n",
        "  - id: listsUsers\n    name: Lists users\n    call: users#list\n",
        "  # makes and removes one\n",
        "  - id: createsUser\n    name: Creates a user\n    call: users#create\n",
    ),
);

/// The schema line every file Vibranium creates starts with, for a file `kind`.
pub(crate) fn schema_line(kind: &str) -> String {
    format!("# yaml-language-server: $schema=https://v007m.github.io/vc-rs/schema/{kind}.schema.json")
}

impl Project {
    /// An empty folder, for `vc init`.
    pub(crate) fn empty() -> Self {
        Self::new(&[])
    }

    /// The text of `path` under the project root.
    pub(crate) fn read(&self, path: &str) -> String {
        fs::read_to_string(self.path().join(path)).unwrap_or_else(|e| panic!("{path}: {e}"))
    }

    pub(crate) fn exists(&self, path: &str) -> bool {
        self.path().join(path).exists()
    }

    /// Points the config's `baseUrl` var at `base`.
    pub(crate) fn serve_from(&self, base: &str) {
        let config = CONFIG.1.replace("http://x", base);
        self.write(CONFIG.0, &config);
    }

    /// `vc <args>` in the project, with no colour, its own home, no `VIB_*` from outside, and
    /// this build's `vc` first on `PATH` (the sample's `command:` steps call `vc demo`).
    pub(crate) fn vc(&self, args: &[&str]) -> Command {
        Command::from_std(self.process(args))
    }

    /// [`Project::vc`] as a plain process, for a command that keeps running (`vc watch`).
    pub(crate) fn process(&self, args: &[&str]) -> std::process::Command {
        let mut vc = std::process::Command::new(cargo_bin!("vc"));
        vc.current_dir(self.path()).args(args).env("NO_COLOR", "1").env("PATH", path_with_vc());
        vc.env("HOME", self.home()).env("XDG_CONFIG_HOME", self.home()).env_remove("APPDATA");
        for (name, _) in std::env::vars_os() {
            if name.to_string_lossy().starts_with("VIB_") {
                vc.env_remove(name);
            }
        }
        vc
    }
}

/// `PATH` with the folder of this build's `vc` in front.
fn path_with_vc() -> OsString {
    let folder = cargo_bin!("vc").parent().unwrap().to_path_buf();
    let rest = std::env::var_os("PATH").unwrap_or_default();
    std::env::join_paths(std::iter::once(folder).chain(std::env::split_paths(&rest))).unwrap()
}

/// Stdout and stderr together, for messages that may go to either.
pub(crate) fn said(output: &std::process::Output) -> String {
    format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr))
}

/// Asserts `vc check` passes with no errors and no warnings.
pub(crate) fn checks_clean(project: &Project) {
    let output = project.vc(&["check"]).output().unwrap();
    let text = said(&output);
    assert_eq!(output.status.code(), Some(0), "{text}");
    assert!(text.contains("0 errors · 0 warnings"), "{text}");
}
