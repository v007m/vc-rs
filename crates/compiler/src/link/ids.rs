//! The ID tables: duplicates (V2001), reserved IDs (V2006), including a client certificate
//! named `none`, and file IDs (V2007).

use std::collections::HashMap;

use vc_diagnostics::{Code, Diagnostic, Diagnostics, Fix, TextEdit};
use vc_model::{FileKind, SourceSpan};
use vc_parser::Node;

use super::build::NO_CLIENT_CERT;
use super::project;
use super::project::{Project, Source};

/// The root names no `id` may take (D128, D137).
const RESERVED: [&str; 11] =
    ["var", "env", "secret", "response", "request", "example", "item", "iteration", "fake", "dataset", "run"];

/// The config's top-level lists whose items have IDs.
const CONFIG_ITEMS: [&str; 4] = ["profiles", "auth", "generators", "plugins"];

/// The config lists below the top level whose items have IDs, each as its path of keys.
const NESTED_ITEMS: [&[&str]; 2] = [&["datasets", "files"], &["http", "tls", "clients"]];

/// The items of the list at `path` under `root`.
fn nested<'a>(root: &'a Node, path: &[&str]) -> &'a [Node] {
    let Some((key, parents)) = path.split_last() else { return &[] };
    parents.iter().try_fold(root, |node, parent| node.get(parent)).map_or(&[], |parent| project::list(parent, key))
}

/// Every `id` value node a file declares, in file order.
pub fn declared<'a>(source: &Source<'a>) -> Vec<&'a Node> {
    let items: &[&str] = match source.kind {
        FileKind::Api => &["apis"],
        FileKind::Test => &["tests"],
        FileKind::Config => &CONFIG_ITEMS,
        FileKind::Load => &[],
    };
    let config = (source.kind == FileKind::Config).then_some(source.root);
    let nested = NESTED_ITEMS.iter().flat_map(|path| config.map(|root| nested(root, path)).unwrap_or_default());
    let nodes = items.iter().flat_map(|key| project::list(source.root, key)).chain(nested);
    let nodes = nodes.chain(project::actions(source));
    let mut ids: Vec<&Node> = nodes.filter_map(|node| node.get("id")).collect();
    ids.sort_by_key(|node| node.span.start.offset);
    ids
}

/// Checks every file's IDs and the file IDs.
pub fn check(project: &Project<'_>, diagnostics: &mut Diagnostics) {
    for source in &project.sources {
        within_file(source, diagnostics);
        diagnostics.extend(explicit_id(source));
    }
    let clients = project.config().map(|config| project::tls_clients(config.root)).unwrap_or_default();
    diagnostics.extend(clients.iter().filter_map(no_cert_id));
    let mut seen: HashMap<&str, SourceSpan> = HashMap::new();
    for source in &project.sources {
        let Some(id) = source.id.as_deref() else { continue };
        let spot = file_spot(source);
        if let Some(first) = seen.get(id) {
            let message = format!("two files have the ID `{id}`");
            let diagnostic = Diagnostic::new(Code::V2001, spot, message).label("this file's ID");
            let diagnostic = diagnostic.secondary(*first, "the other file with that ID");
            diagnostics.push(diagnostic.help("rename or move one of the files"));
        } else {
            seen.insert(id, spot);
        }
    }
}

/// V2001 for an ID declared twice in one file and V2006 for a reserved one.
fn within_file(source: &Source<'_>, diagnostics: &mut Diagnostics) {
    let mut seen: HashMap<&str, SourceSpan> = HashMap::new();
    for node in declared(source) {
        let Some(id) = node.as_scalar().map(|scalar| scalar.text.as_str()) else { continue };
        if RESERVED.contains(&id) {
            let message = format!("`{id}` is a reserved name and cannot be an ID");
            let diagnostic = Diagnostic::new(Code::V2006, node.span, message).label("reserved");
            diagnostics.push(diagnostic.help(format!("pick another ID; `{id}` is a template root")));
        }
        match seen.get(id) {
            Some(first) => diagnostics.push(duplicate(id, node.span, *first)),
            None => {
                seen.insert(id, node.span);
            }
        }
    }
}

/// V2006 for an `http.tls.clients` entry with the ID `none`, which `clientCert: none` means
/// sending no certificate.
fn no_cert_id(client: &Node) -> Option<Diagnostic> {
    let id = client.get("id").filter(|id| id.as_str() == Some(NO_CLIENT_CERT))?;
    let message = format!("`{NO_CLIENT_CERT}` is a reserved name and cannot be a client certificate ID");
    let diagnostic = Diagnostic::new(Code::V2006, id.span, message).label("reserved");
    Some(diagnostic.help(format!("pick another ID; `clientCert: {NO_CLIENT_CERT}` sends no certificate")))
}

fn duplicate(id: &str, span: SourceSpan, first: SourceSpan) -> Diagnostic {
    let diagnostic = Diagnostic::new(Code::V2001, span, format!("duplicate ID `{id}`")).label("declared again");
    let diagnostic = diagnostic.secondary(first, "first declared here");
    diagnostic.help("IDs are unique within a file across every kind; rename one")
}

/// V2007 for an `id:` that is not the ID the path gives.
fn explicit_id(source: &Source<'_>) -> Option<Diagnostic> {
    let derived = source.id.as_deref()?;
    let node = source.root.get("id")?;
    let written = node.as_scalar()?.text.as_str();
    if written == derived {
        return None;
    }
    let message = format!("the file ID is `{derived}` from its path, not `{written}`");
    let diagnostic = Diagnostic::new(Code::V2007, node.span, message).label("does not match the path");
    let diagnostic = diagnostic.help("remove the `id:` key, or move the file to match it");
    if node.span.byte_range().len() != written.len() {
        return Some(diagnostic);
    }
    let edit = TextEdit::replace(node.span, derived);
    Some(diagnostic.fix(Fix::certain(format!("Write {derived}"), vec![edit])))
}

/// Where a whole-file problem is shown: its `id:`, else its first key, else its start.
fn file_spot(source: &Source<'_>) -> SourceSpan {
    let first = source.root.entries().first().map(|entry| entry.key_span);
    source.root.get("id").map(|node| node.span).or(first).unwrap_or(source.root.span)
}
