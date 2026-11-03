//! Reads compiled pieces and plain scalars off a file's tree for the program build.

use indexmap::IndexMap;
use vc_model::{BodySpec, Duration, HookRef, Method, Retry, Skip, Tag, Template, TemplateValue};
use vc_parser::Node;

use crate::link::project::{Source, list, text};
use crate::parts::Piece;

/// The methods an api may name, as written.
const METHODS: [(&str, Method); 7] = [
    ("GET", Method::Get),
    ("POST", Method::Post),
    ("PUT", Method::Put),
    ("PATCH", Method::Patch),
    ("DELETE", Method::Delete),
    ("HEAD", Method::Head),
    ("OPTIONS", Method::Options),
];

/// Makes a body of one kind from its template.
type MakeBody = fn(Template) -> BodySpec;

/// The single-key body maps that name a body kind other than JSON.
const BODY_KINDS: [(&str, MakeBody); 2] = [("$file", BodySpec::File), ("$base64", BodySpec::Base64)];

/// The value the compiler lowered at `node`.
pub fn value(source: &Source<'_>, node: &Node) -> Option<TemplateValue> {
    match source.piece(node)? {
        Piece::Value(value) => Some(value.clone()),
        _ => None,
    }
}

/// The template the compiler parsed at `node`.
pub fn template(source: &Source<'_>, node: &Node) -> Option<Template> {
    match source.piece(node)? {
        Piece::Template(template) => Some(template.clone()),
        _ => None,
    }
}

/// The duration under `key`.
pub fn duration(source: &Source<'_>, node: &Node, key: &str) -> Option<Duration> {
    match source.piece(node.get(key)?)? {
        Piece::Duration(duration) => Some(*duration),
        _ => None,
    }
}

/// The templates of the map under `key`, by name.
pub fn templates(source: &Source<'_>, node: &Node, key: &str) -> IndexMap<String, Template> {
    let entries = node.get(key).map(Node::entries).unwrap_or_default();
    entries.iter().filter_map(|entry| Some((entry.key.clone(), template(source, &entry.value)?))).collect()
}

/// `key` as a whole number.
pub fn number(node: &Node, key: &str) -> Option<u32> {
    text(node, key)?.parse().ok()
}

/// Whether `key` is `true`.
pub fn flag(node: &Node, key: &str) -> bool {
    text(node, key).is_some_and(|value| value.eq_ignore_ascii_case("true"))
}

/// `key` as owned text.
pub fn owned(node: &Node, key: &str) -> Option<String> {
    text(node, key).map(str::to_owned)
}

/// The texts of the list under `key`.
pub fn texts(node: &Node, key: &str) -> Vec<String> {
    list(node, key).iter().filter_map(|item| item.as_scalar()).map(|scalar| scalar.text.clone()).collect()
}

/// The tags under `tags`.
pub fn tags(node: &Node) -> Vec<Tag> {
    texts(node, "tags").into_iter().map(Tag).collect()
}

/// `method`, `GET` when it is left out.
pub fn method(node: &Node) -> Method {
    let written = text(node, "method").unwrap_or_default();
    METHODS.iter().find(|(name, _)| *name == written).map_or(Method::Get, |(_, method)| *method)
}

/// `skip`: a reason, or a reason with an end date.
pub fn skip(node: &Node) -> Option<Skip> {
    let skip = node.get("skip")?;
    if let Some(reason) = skip.as_scalar() {
        return Some(Skip { reason: reason.text.clone(), until: None });
    }
    Some(Skip { reason: owned(skip, "reason")?, until: owned(skip, "until") })
}

/// `retry`: further attempts and the pause between them.
pub fn retry(source: &Source<'_>, node: &Node) -> Option<Retry> {
    let retry = node.get("retry")?;
    let delay = duration(source, retry, "delay").unwrap_or_else(|| Duration::new(std::time::Duration::ZERO));
    Some(Retry { attempts: number(retry, "attempts")?, delay })
}

/// A `path#export` function reference.
pub fn hook(written: &str) -> HookRef {
    let (file, export) = written.split_once('#').unwrap_or((written, ""));
    HookRef { file: file.to_owned(), export: export.to_owned() }
}

/// The functions listed under `key`.
pub fn hooks(node: &Node, key: &str) -> Vec<HookRef> {
    list(node, key).iter().filter_map(|item| text(item, "function")).map(hook).collect()
}

/// The request body at `node`: text, `$file`, `$base64`, or JSON.
pub fn body(source: &Source<'_>, node: &Node) -> Option<BodySpec> {
    let value = value(source, node)?;
    Some(match value {
        TemplateValue::String(template) => BodySpec::Text(template),
        TemplateValue::Extend { base, changes, remove } => BodySpec::Extend { base, changes: *changes, remove },
        TemplateValue::Map(map) => special(map),
        other => BodySpec::Json(other),
    })
}

fn special(map: IndexMap<String, TemplateValue>) -> BodySpec {
    let kind = map.first().and_then(|(key, value)| {
        let TemplateValue::String(template) = value else { return None };
        let (_, make) = BODY_KINDS.iter().find(|(name, _)| name == key)?;
        Some(make(template.clone()))
    });
    match kind {
        Some(kind) if map.len() == 1 => kind,
        _ => BodySpec::Json(TemplateValue::Map(map)),
    }
}
