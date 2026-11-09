//! `vc import <kind> <url-or-file>` and `vc sync [fileId...]`: api files from a spec or a
//! recording, kept in step with the spec.

use std::path::Path;

use clap::Args;
use vc_engine::{Engine, ImportRequest, SyncMode, SyncReport, SyncRequest, SyncSide};

use crate::project::Located;
use crate::{Outcome, changes, project};

/// `vc import …`.
#[derive(Args)]
pub struct ImportArgs {
    /// The kind of file: `openapi`, `har`, `postman`, `bruno` or `http`.
    kind: String,
    /// The URL or path of the spec or recording.
    source: String,
    /// The file ID of the api file to fill, such as `users`; derived from the spec when left
    /// out.
    #[arg(long = "id", value_name = "FILE_ID")]
    file_id: Option<String>,
    /// Also writes a happy-path test per api.
    #[arg(long)]
    tests: bool,
    /// Also writes tests that send what the schema rejects.
    #[arg(long)]
    negative: bool,
    /// Also writes a test per secured api that expects 401 without auth.
    #[arg(long)]
    auth: bool,
    /// Only the operations with this tag or path prefix; repeat for more.
    #[arg(long, value_name = "TAG_OR_PATH")]
    include: Vec<String>,
    /// Only the operation with this operation ID; repeat for more.
    #[arg(long, value_name = "OPERATION_ID")]
    only: Vec<String>,
    /// The active profiles whose vars fill `source.headers`, as a comma list or repeated.
    #[arg(long = "profile", value_name = "IDS")]
    profiles: Vec<String>,
}

/// `vc sync …`.
#[derive(Args)]
pub struct SyncArgs {
    /// The api files to sync; every file with a `source` when none is given.
    files: Vec<String>,
    /// Prints the changes without writing them.
    #[arg(long, conflicts_with = "check")]
    dry_run: bool,
    /// Writes nothing; exits 4 when a spec changed since its snapshot or a conflict is left.
    #[arg(long)]
    check: bool,
    /// Settles a conflict: `--take spec <apiId>` takes the spec's values, `--take mine <apiId>`
    /// keeps the file's; repeat for more.
    #[arg(long, num_args = 2, value_names = ["SIDE", "API_ID"])]
    take: Vec<String>,
    /// Settles a conflict you fixed by hand: the file stays, the snapshot takes the spec.
    #[arg(long, value_name = "API_ID")]
    resolved: Vec<String>,
}

/// `vc import <kind> <source>`: exit 0 once written, 2 when the source cannot be read or the
/// result would not compile, and then nothing is written.
pub fn import(args: &ImportArgs, project: Option<&Path>) -> anyhow::Result<Outcome> {
    let located = match project::locate(project)? {
        Ok(located) => located,
        Err(outcome) => return Ok(outcome),
    };
    let source = match spec_location(&located, &args.source) {
        Ok(source) => source,
        Err(outcome) => return Ok(outcome),
    };
    let request = ImportRequest {
        kind: args.kind.clone(),
        source,
        file_id: args.file_id.clone(),
        tests: args.tests,
        negative: args.negative,
        auth: args.auth,
        include: args.include.clone(),
        only: args.only.clone(),
        profiles: args.profiles.clone(),
    };
    imported(&located.engine, &request)
}

/// `vc init --from-openapi <source>`: the spec imported into the project just created.
pub fn openapi(project: Option<&Path>, source: &str) -> anyhow::Result<Outcome> {
    let located = match project::locate(project)? {
        Ok(located) => located,
        Err(outcome) => return Ok(outcome),
    };
    let source = match spec_location(&located, source) {
        Ok(source) => source,
        Err(outcome) => return Ok(outcome),
    };
    imported(&located.engine, &ImportRequest { kind: "openapi".to_owned(), source, ..ImportRequest::default() })
}

fn imported(engine: &Engine, request: &ImportRequest) -> anyhow::Result<Outcome> {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build()?;
    let made = runtime.block_on(engine.import(request));
    let notes: Vec<String> =
        made.as_ref().map(|made| made.titles.iter().skip(1).cloned().collect()).unwrap_or_default();
    let outcome = changes::write(engine, made, &format!("✓ imported {}", request.source));
    if matches!(outcome, Outcome::Done) {
        for note in &notes {
            println!("  not imported: {note}");
        }
    }
    Ok(outcome)
}

/// A URL as given; a path from the working folder (the project root with `--project`) as a
/// path in the project, which is the only place a spec file is read from.
pub fn spec_location(located: &Located, source: &str) -> Result<String, Outcome> {
    if source.contains("://") {
        return Ok(source.to_owned());
    }
    located
        .project_path(Path::new(source))
        .map_err(|reason| Outcome::Helped(reason, Some("give a URL, or a path inside the project".to_owned())))
}

/// `vc sync [fileId...] [--dry-run] [--check]`: a line per change, then exit 0 once written
/// (or shown, with `--dry-run`), 2 when a spec cannot be read or the result would not compile,
/// and under `--check` 4 for drift or a conflict left and 0 without (D127).
pub fn sync(args: &SyncArgs, project: Option<&Path>) -> anyhow::Result<Outcome> {
    let located = match project::locate(project)? {
        Ok(located) => located,
        Err(outcome) => return Ok(outcome),
    };
    let request = match sync_request(args) {
        Ok(request) => request,
        Err(outcome) => return Ok(outcome),
    };
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build()?;
    let report = match runtime.block_on(located.engine.sync(&request)) {
        Ok(report) => report,
        Err(error) => return Ok(Outcome::Engine(error)),
    };
    for line in &report.changes.titles {
        println!("{line}");
    }
    match request.mode {
        SyncMode::Check => Ok(Outcome::Exit(if report.drift || report.conflicts > 0 { 4 } else { 0 })),
        SyncMode::DryRun => changes::preview(&report.changes).map(|()| Outcome::Done),
        SyncMode::Write => {
            let done = synced(&report);
            Ok(changes::write(&located.engine, Ok(report.changes), &done))
        }
    }
}

fn sync_request(args: &SyncArgs) -> Result<SyncRequest, Outcome> {
    let mut settle = Vec::new();
    for pair in args.take.chunks(2) {
        let (Some(side), Some(api)) = (pair.first(), pair.get(1)) else { continue };
        let side = match side.as_str() {
            "spec" => SyncSide::Spec,
            "mine" => SyncSide::Mine,
            other => {
                let help = Some(format!("`--take spec {api}` or `--take mine {api}`"));
                return Err(Outcome::Helped(format!("`--take {other}` names no side of the merge"), help));
            }
        };
        settle.push((api.clone(), side));
    }
    settle.extend(args.resolved.iter().map(|api| (api.clone(), SyncSide::Mine)));
    let mode = match (args.dry_run, args.check) {
        (true, _) => SyncMode::DryRun,
        (_, true) => SyncMode::Check,
        _ => SyncMode::Write,
    };
    Ok(SyncRequest { files: args.files.clone(), mode, settle })
}

/// `✓ synced`, the snapshots written and the conflicts left.
fn synced(report: &SyncReport) -> String {
    let snapshots = report.changes.files.iter().filter(|file| file.path.starts_with("openapi/"));
    let mut done =
        snapshots.fold("✓ synced".to_owned(), |done, file| format!("{done} · snapshot {} updated", file.path));
    if report.conflicts > 0 {
        let plural = if report.conflicts == 1 { "" } else { "s" };
        done = format!("{done} · {} conflict{plural} left", report.conflicts);
    }
    done
}
