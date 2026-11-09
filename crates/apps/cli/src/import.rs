//! `vc import <kind> <url-or-file>`: api files from a spec or a recording.

use std::path::Path;

use clap::Args;
use vc_engine::{Engine, ImportRequest};

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
