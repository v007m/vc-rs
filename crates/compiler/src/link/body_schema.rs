//! `body.schema` (D201): the JSON Schema a response body must match, written inline or named by
//! a file. A file is a bare string or a single-key `$file` map, relative to the file holding it,
//! with an optional `#/pointer` inside it. The compiler does no I/O (D132): the engine reads a
//! named file the compile asks for and compiles again with it, as it does for datasets.

use vc_parser::{Node, Value};

use super::assets;
use super::project::Source;

/// The keys whose `body.schema` is a check: `expect`, and `poll.until`.
const HOLDERS: [&str; 2] = ["expect", "until"];

/// Every `body.schema` under `root`.
pub fn nodes(root: &Node) -> Vec<&Node> {
    let mut found = Vec::new();
    walk(root, &mut found);
    found
}

fn walk<'n>(node: &'n Node, found: &mut Vec<&'n Node>) {
    match &node.value {
        Value::Map(entries) => {
            for entry in entries {
                if HOLDERS.contains(&entry.key.as_str()) {
                    found.extend(entry.value.get("body").and_then(|body| body.get("schema")));
                }
                walk(&entry.value, found);
            }
        }
        Value::Seq(items) => items.iter().for_each(|item| walk(item, found)),
        Value::Scalar(_) => {}
    }
}

/// The file a schema at `node` names, as written; `None` when it is written inline.
fn written(node: &Node) -> Option<&str> {
    if let Some(text) = node.as_str() {
        return Some(text);
    }
    let [only] = node.entries() else { return None };
    only.value.as_str().filter(|_| only.key == "$file")
}

/// The project path of the file the schema at `node` is read from, when the compiler can know it:
/// a path holding a template is only known at run time.
pub fn path(source: &Source<'_>, node: &Node) -> Option<String> {
    let file = written(node)?.split('#').next().unwrap_or_default();
    if file.contains("${") {
        return None;
    }
    assets::resolve(&assets::folder(source.path), file)
}

/// The key the check at `node` names its schema by in the program: the file's project path with
/// the `#/pointer` as written, the text as written when the path is not known, or
/// `<file>:<line>:<column>` for a schema written inline.
pub fn key(source: &Source<'_>, node: &Node) -> String {
    let Some(text) = written(node) else { return inline(source, node) };
    let pointer = text.split_once('#').map(|(_, pointer)| pointer);
    match (path(source, node), pointer) {
        (Some(path), Some(pointer)) => format!("{path}#{pointer}"),
        (Some(path), None) => path,
        (None, _) => text.to_owned(),
    }
}

/// The key of a schema written inline at `node`.
pub fn inline(source: &Source<'_>, node: &Node) -> String {
    format!("{}:{}:{}", source.path, node.span.start.line, node.span.start.column)
}

/// Whether the schema at `node` is written inline rather than named by a file.
pub fn is_inline(node: &Node) -> bool {
    written(node).is_none()
}
