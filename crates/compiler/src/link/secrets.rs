//! Secret names against `.env.schema` (D165): a `${secret.X}` the schema does not declare
//! (V2106), and a `${env.X}` that reads a declared secret around the redactor (V2107).

use vc_diagnostics::{Code, Diagnostic, Diagnostics, Fix, TextEdit};
use vc_model::{Namespace, Reference, Segment};

use super::project::{Project, Source};
use super::uses::{did_you_mean, piece_refs};

/// The root `${env.X}` is rewritten to when `X` is a declared secret.
const SECRET_ROOT: &str = "secret";

/// Every `${secret.*}` and `${env.*}` in the project, checked against the `declared` names.
pub fn check(project: &Project<'_>, declared: &[String]) -> Diagnostics {
    let mut diagnostics = Diagnostics::new();
    for source in &project.sources {
        let mut found = Vec::new();
        for piece in source.pieces(source.root.span, &[]) {
            piece_refs(piece, &mut found);
        }
        for reference in found {
            diagnostics.extend(problem(source, reference, declared));
        }
    }
    diagnostics
}

fn problem(source: &Source<'_>, reference: &Reference, declared: &[String]) -> Option<Diagnostic> {
    let Some(Segment::Key(name)) = reference.path.segments.first() else { return None };
    let is_declared = declared.iter().any(|known| known == name);
    match reference.ns {
        Namespace::Secret if !is_declared => Some(undeclared(source, reference, (name, declared))),
        Namespace::Env if is_declared => Some(env_secret(source, reference, name)),
        _ => None,
    }
}

fn undeclared(source: &Source<'_>, reference: &Reference, (name, declared): (&str, &[String])) -> Diagnostic {
    let message = format!("`${{secret.{name}}}` is not declared in `.env.schema`");
    let diagnostic = Diagnostic::new(Code::V2106, reference.span, message)
        .label(format!("{name} is not in .env.schema"))
        .help(format!("add the line `{name}=` to `.env.schema` at the project root, or fix the name"));
    let known: Vec<&str> = declared.iter().map(String::as_str).collect();
    did_you_mean(diagnostic, (source, reference, name), &known)
}

fn env_secret(source: &Source<'_>, reference: &Reference, name: &str) -> Diagnostic {
    let message = format!("`${{env.{name}}}` reads a secret that `.env.schema` declares");
    let diagnostic = Diagnostic::new(Code::V2107, reference.span, message)
        .label(format!("{name} is a secret"))
        .note("a declared secret is reachable only as `${secret.X}`, so it is masked everywhere")
        .help(format!("write `${{secret.{name}}}`"));
    let root = reference.root_span;
    if source.text.get(root.byte_range()) != Some("env") {
        return diagnostic;
    }
    let edit = TextEdit::replace(root, SECRET_ROOT);
    diagnostic.fix(Fix::certain(format!("Replace with secret.{name}"), vec![edit]))
}
