//! `vc new api|test|auth|file`: add an item to the file its ID derives from, or create that
//! file (D139). `vc new auth` asks for what its flags leave out, on a terminal only.

use std::io::{BufRead, IsTerminal, Write};
use std::path::Path;

use clap::{Args, Subcommand, ValueEnum};
use vc_engine::{Added, Engine, EngineError, FileKind, NewApi, NewAuth, NewTest, auth_fields};

use crate::{Outcome, changes, project};

#[derive(Args)]
pub struct NewArgs {
    #[command(subcommand)]
    what: NewCommand,
}

#[derive(Subcommand)]
enum NewCommand {
    /// Appends an api to the api file its ID derives from, creating the file if needed.
    Api(ApiArgs),
    /// Appends a happy-path test calling an api to the test file its ID derives from.
    Test(TestArgs),
    /// Adds an auth definition to the config, asking for its fields on a terminal.
    Auth(AuthArgs),
    /// Creates an empty file of a kind, holding only its header.
    File(FileArgs),
}

#[derive(Args)]
struct ApiArgs {
    /// The api's full ID: `users#deactivate`.
    id: String,
    /// The HTTP method; GET when left out.
    #[arg(long)]
    method: Option<String>,
    /// The path, with `{name}` for each parameter: `/users/{id}/deactivate`.
    #[arg(long, conflicts_with = "url")]
    path: Option<String>,
    /// A full URL; its origin becomes a base-URL var in the config.
    #[arg(long)]
    url: Option<String>,
}

#[derive(Args)]
struct TestArgs {
    /// The test's full ID: `users.crud#listsUsers`.
    id: String,
    /// The api it calls: `users#list`.
    #[arg(long, value_name = "API")]
    from: String,
    /// Also the failure cases built from the api's body schema.
    #[arg(long)]
    negative: bool,
    /// Also a test calling the api with `auth: none`, expecting 401.
    #[arg(long)]
    auth: bool,
}

#[derive(Args)]
struct AuthArgs {
    /// The auth's ID: `partnerToken`.
    id: String,
    /// Its type: `bearer`, `basic`, `apiKey`, `oauth2ClientCredentials`, …
    #[arg(long = "type", value_name = "TYPE")]
    auth_type: Option<String>,
    /// Prints what was added as JSON, and never asks.
    #[arg(long)]
    json: bool,
}

#[derive(Args)]
struct FileArgs {
    /// The kind of file.
    kind: Kind,
    /// Its file ID: `users.crud`.
    id: String,
}

#[derive(Clone, Copy, ValueEnum)]
enum Kind {
    Api,
    Test,
    Load,
}

/// `vc new …`: exit 0 once written, 2 when refused.
pub fn run(args: NewArgs, project: Option<&Path>) -> anyhow::Result<Outcome> {
    let located = match project::locate(project)? {
        Ok(located) => located,
        Err(outcome) => return Ok(outcome),
    };
    let engine = &located.engine;
    Ok(match args.what {
        NewCommand::Api(args) => {
            let request = NewApi { id: args.id, method: args.method, path: args.path, url: args.url };
            written(engine, engine.new_api(&request), false)
        }
        NewCommand::Test(args) => {
            let TestArgs { id, from, negative, auth } = args;
            written(engine, engine.new_test(&NewTest { id, from, negative, auth }), false)
        }
        NewCommand::Auth(args) => auth(engine, args)?,
        NewCommand::File(args) => written(engine, engine.new_file(args.kind.into(), &args.id), false),
    })
}

/// `vc new auth`: the type and fields from the flags, and on a terminal (without `--json`)
/// asked for when left out.
fn auth(engine: &Engine, args: AuthArgs) -> anyhow::Result<Outcome> {
    let asks = !args.json && std::io::stdin().is_terminal();
    let auth_type = match (args.auth_type, asks) {
        (Some(auth_type), _) => auth_type,
        (None, true) => ask("type (bearer, basic, apiKey, oauth2ClientCredentials, …):")?,
        (None, false) => {
            let help = Some("for example `--type bearer`".to_owned());
            return Ok(Outcome::Helped("`vc new auth` needs `--type` when it cannot ask".to_owned(), help));
        }
    };
    let mut fields = Vec::new();
    for field in auth_fields(&auth_type).iter().filter(|_| asks) {
        fields.push(((*field).to_owned(), ask(&format!("{field} (Enter to leave it out):"))?));
    }
    Ok(written(engine, engine.new_auth(&NewAuth { id: args.id, auth_type, fields }), args.json))
}

/// Asks `question` on stderr and reads one answer line from stdin, trimmed.
fn ask(question: &str) -> anyhow::Result<String> {
    eprint!("? {question} ");
    std::io::stderr().flush()?;
    let mut answer = String::new();
    std::io::stdin().lock().read_line(&mut answer)?;
    Ok(answer.trim().to_owned())
}

/// Writes what was added and says where: `✓ added users#deactivate to <path>:<line>`, or the
/// JSON object.
fn written(engine: &Engine, made: Result<Added, EngineError>, json: bool) -> Outcome {
    match made {
        Ok(added) => {
            let done =
                if json { added.json() } else { format!("✓ added {} to {}:{}", added.id, added.path, added.line) };
            changes::write(engine, Ok(added.changes), &done)
        }
        Err(error) => Outcome::Engine(error),
    }
}

impl From<Kind> for FileKind {
    fn from(kind: Kind) -> Self {
        match kind {
            Kind::Api => Self::Api,
            Kind::Test => Self::Test,
            Kind::Load => Self::Load,
        }
    }
}
