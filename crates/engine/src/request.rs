//! What apps ask the engine to plan, run, list and explain.

use std::path::PathBuf;

use vc_model::SourcePath;

/// Which tests a command selects (cli-workflow "Selectors"); everything when empty.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "built by the apps with `..Default::default()`; every user is in this repo (D160)"
)]
pub struct SelectRequest {
    /// Test IDs, instances, file IDs and globs over IDs.
    pub ids: Vec<String>,
    /// Folders or test files, relative to the project root with `/` separators.
    pub paths: Vec<String>,
    /// One tag expression per `-t`, all of which must hold.
    pub tags: Vec<String>,
    /// Text the test's full ID or name contains, ignoring case.
    pub grep: Option<String>,
    /// `--changed [ref]`: only files changed since the ref (`Some("")` for `HEAD`, the
    /// uncommitted and untracked changes); every file when the config or `.env.schema` changed.
    pub changed: Option<String>,
    /// `vc watch`: only the files [`crate::Engine::watch`] reported saved, and the tests that
    /// call an api in one; every file when the config, `.env` or `.env.schema` was saved.
    pub saved: Option<Vec<SourcePath>>,
    /// Plans skipped tests instead of reporting them as skipped.
    pub include_skipped: bool,
}

/// What `vc plan` asks for, and the part of `vc run` that decides what runs.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "built by the apps with `..Default::default()`; every user is in this repo (D160)"
)]
pub struct PlanRequest {
    /// The tests to plan.
    pub select: SelectRequest,
    /// `--repeat N`: runs every selected test `N` times instead of its own `repeat`.
    pub repeat: Option<u32>,
    /// `--profile`, in order; later ones win. Each may be a comma list.
    pub profiles: Vec<String>,
    /// `--set key=value`, in order; the top config layer (D129).
    pub sets: Vec<String>,
}

/// What `vc run` asks for.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "built by the apps with `..Default::default()`; every user is in this repo (D160)"
)]
#[expect(clippy::struct_excessive_bools, reason = "each bool is one `vc run` flag, independent of the others")]
pub struct RunRequest {
    /// What runs, and with which profiles and settings.
    pub plan: PlanRequest,
    /// `--seed`: the seed for random values; a fresh one when `None`.
    pub seed: Option<u64>,
    /// `--bail`: stop starting tests after the first failure.
    pub bail: bool,
    /// `--concurrency`: test instances at once, over `defaults.concurrency`.
    pub concurrency: Option<u32>,
    /// `--reporter`: the console format, `spec` when `None`.
    pub console: Option<String>,
    /// `--report`, `--out` and `--single-file`: the report files, over `reports.*`.
    pub files: ReportFlags,
    /// Whether the console output is coloured.
    pub color: bool,
    /// `--failed`: the failed and blocked tests of the last run instead of the selection, under
    /// its profiles unless `plan` names some.
    pub failed: bool,
    /// `--no-cache`: `cache:` actions neither read nor write the cross-run cache, and no
    /// expired entry is cleaned up.
    pub no_cache: bool,
}

/// Which report files a run or `vc report` writes, and where.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "built by the apps with `..Default::default()`; every user is in this repo (D160)"
)]
pub struct ReportFlags {
    /// `--report` or `--format`: the formats. Each may be a comma list.
    pub formats: Vec<String>,
    /// `--out`: the report folder; absolute, or relative to the root.
    pub out: Option<PathBuf>,
    /// `--single-file`: the HTML report as one self-contained file.
    pub single_file: bool,
}

/// What `vc list` lists.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_enums,
    reason = "matched by the apps, one arm per listing; every user is in this repo (D160)"
)]
pub enum ListKind {
    /// The selected tests.
    #[default]
    Tests,
    /// Every api.
    Apis,
    /// Every test and api file.
    Files,
}

/// What `vc list` asks for.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "built by the apps with `..Default::default()`; every user is in this repo (D160)"
)]
pub struct ListRequest {
    /// What to list.
    pub kind: ListKind,
    /// What to list: tests, apis (no tags) or definition files.
    pub select: SelectRequest,
    /// `--skipped`: only the skipped tests.
    pub skipped: bool,
}

/// What `vc explain <id>` and `vc config explain <key>` resolve against.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "built by the apps with `..Default::default()`; every user is in this repo (D160)"
)]
pub struct ExplainRequest {
    /// `--profile`, in order; later ones win. Each may be a comma list.
    pub profiles: Vec<String>,
    /// `--set key=value`, in order; the top config layer (D129).
    pub sets: Vec<String>,
}

/// What `vc call <apiId>` sends: one api, ad hoc, with no `beforeRun` or `beforeAll` (D178).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "built by the apps with `..Default::default()`; every user is in this repo (D160)"
)]
pub struct CallRequest {
    /// The api's full ID, `users#list`.
    pub api: String,
    /// `--var name=value`, in order; each beats the vars the api and the config define.
    pub vars: Vec<String>,
    /// `--keep`: the api's cleanup does not run afterwards.
    pub keep: bool,
    /// `--profile`, in order; later ones win. Each may be a comma list.
    pub profiles: Vec<String>,
    /// `--set key=value`, in order; the top config layer (D129).
    pub sets: Vec<String>,
}
