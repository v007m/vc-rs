//! Reads compiled pieces and plain scalars off a file's tree for the program build.

use indexmap::IndexMap;
use vc_model::{
    AgentPolicy, ApiSource, BodySpec, Duration, Examples, HookRef, Loop, Method, Retry, Size, Skip, Tag, Template,
    TemplateValue, WriteDefinitions,
};
use vc_parser::Node;

use crate::link::assets;
use crate::link::project;
use crate::link::project::Source;
use crate::parts::Piece;
use crate::shapes;
use crate::shapes::Shape;

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

/// The values `writeDefinitions` may take, as written.
const WRITES: [(&str, WriteDefinitions); 3] =
    [("true", WriteDefinitions::Allow), ("ask", WriteDefinitions::Ask), ("false", WriteDefinitions::Deny)];

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

/// The size under `key`.
pub fn size(source: &Source<'_>, node: &Node, key: &str) -> Option<Size> {
    match source.piece(node.get(key)?)? {
        Piece::Size(size) => Some(*size),
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
    project::text(node, key)?.parse().ok()
}

/// Whether `key` is `true`, taking the default `shape` declares for it when it is not written.
pub fn flag(shape: &Shape, node: &Node, key: &str) -> bool {
    let written = project::text(node, key).or_else(|| shape.find(key)?.default);
    written.is_some_and(|value| value.eq_ignore_ascii_case("true"))
}

/// `key` as owned text.
pub fn owned(node: &Node, key: &str) -> Option<String> {
    project::text(node, key).map(str::to_owned)
}

/// The texts of the list under `key`.
pub fn texts(node: &Node, key: &str) -> Vec<String> {
    project::list(node, key).iter().filter_map(|item| item.as_scalar()).map(|scalar| scalar.text.clone()).collect()
}

/// The tags under `tags`.
pub fn tags(node: &Node) -> Vec<Tag> {
    texts(node, "tags").into_iter().map(Tag).collect()
}

/// `method`, `GET` when it is left out.
pub fn method(node: &Node) -> Method {
    method_named(project::text(node, "method").unwrap_or_default()).unwrap_or(Method::Get)
}

/// The method written as `name`, such as `GET`.
pub fn method_named(name: &str) -> Option<Method> {
    METHODS.iter().find(|(written, _)| *written == name).map(|(_, method)| *method)
}

/// The methods of the list under `key`; the shape check has reported any other text.
pub fn methods(node: &Node, key: &str) -> Vec<Method> {
    texts(node, key).iter().filter_map(|name| method_named(name)).collect()
}

/// A profile's `agents` rules; reads only when there is no `agents` block (D195).
pub fn agents(node: Option<&Node>) -> AgentPolicy {
    let Some(node) = node else { return AgentPolicy::default() };
    let written = project::text(node, "writeDefinitions").unwrap_or_default();
    let write = WRITES.iter().find(|(name, _)| name.eq_ignore_ascii_case(written));
    AgentPolicy {
        run: flag(shapes::agents(), node, "run"),
        call: methods(node, "call"),
        approve: methods(node, "approve"),
        write_definitions: write.map_or(WriteDefinitions::Deny, |(_, write)| *write),
    }
}

/// An api file's `source`: the API spec it syncs from.
pub fn api_source(source: &Source<'_>, root: &Node) -> Option<ApiSource> {
    let node = root.get("source")?;
    Some(ApiSource {
        openapi: owned(node, "openapi")?,
        headers: templates(source, node, "headers"),
        include: texts(node, "include"),
    })
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

/// `forEach` or `repeat` on an action, `parallel` iterations at a time (1 when not written);
/// `forEach` wins when both are written.
pub fn looping(source: &Source<'_>, node: &Node) -> Option<Loop> {
    let parallel = number(node, "parallel").unwrap_or(1);
    if let Some(items) = node.get("forEach") {
        return Some(Loop::ForEach { items: value(source, items)?, parallel });
    }
    Some(Loop::Repeat { times: number(node, "repeat")?, parallel })
}

/// A `path#export` function reference, with the path made project-relative (D172).
pub fn hook(source: &Source<'_>, written: &str) -> HookRef {
    let (file, export) = written.split_once('#').unwrap_or((written, ""));
    let file = assets::resolve(&assets::folder(source.path), file).unwrap_or_else(|| file.to_owned());
    HookRef { file, export: export.to_owned() }
}

/// The functions listed under `key`.
pub fn hooks(source: &Source<'_>, node: &Node, key: &str) -> Vec<HookRef> {
    let written = project::list(node, key).iter().filter_map(|item| project::text(item, "function"));
    written.map(|written| hook(source, written)).collect()
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

/// `examples`: rows written inline, a single-key `$file` map, or a template that yields the rows.
/// The rows of a `$file` or a dataset are read by the engine and passed to the planner (D132).
pub fn examples(source: &Source<'_>, node: Option<&Node>) -> Option<Examples> {
    match value(source, node?)? {
        TemplateValue::List(rows) => Some(Examples::Rows(rows)),
        TemplateValue::String(template) => Some(Examples::Template(template)),
        TemplateValue::Map(map) => match special(map) {
            BodySpec::File(template) => Some(Examples::File(template)),
            _ => None,
        },
        _ => None,
    }
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
