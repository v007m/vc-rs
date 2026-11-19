//! The compile rules of a `graphql` api (D343): it sends `POST` with its `query`, `variables`
//! and `operationName`, to its own `path`, else its file's `graphql.path`, else `/graphql`.

use indexmap::IndexMap;
use vc_diagnostics::{Code, Diagnostic};
use vc_model::{BodySpec, GraphqlBody, Method, Part, Protocol, Template, TemplateValue};
use vc_parser::{Entry, Node};

use super::protocol::ProtocolRules;
use super::read;
use crate::link::project;
use crate::link::project::Source;
use crate::shape_check;
use crate::suggest::{self, Misspelling};

/// The `path` of a `graphql` api when neither it nor its file's `graphql.path` sets one.
const DEFAULT_PATH: &str = "/graphql";

/// The `body` key that holds the document.
const QUERY: &str = "query";

/// The `body` key that holds the variables, the only part of the body that is a template.
const VARIABLES: &str = "variables";

/// The `body` key that names the operation to run.
const OPERATION_NAME: &str = "operationName";

/// Every key a `graphql` api's `body` takes (D343).
const BODY_KEYS: &[&str] = &[QUERY, VARIABLES, OPERATION_NAME];

/// The `body` keys that are templates: the `query` is a document, sent as written (D344).
const TEMPLATED: &[&str] = &[VARIABLES];

/// `graphql`: always `POST`; the document is read as written, never as a template (D344).
pub struct GraphqlRules;

impl ProtocolRules for GraphqlRules {
    fn protocol(&self) -> Protocol {
        Protocol::Graphql
    }

    fn method(&self, _: &Node) -> Method {
        Method::Post
    }

    fn body(&self, source: &Source<'_>, api: &Node) -> BodySpec {
        let Some(body) = api.get("body") else { return BodySpec::None };
        let Some(document) = body.get(QUERY).and_then(Node::as_str) else { return BodySpec::None };
        let variables = body.get(VARIABLES).and_then(|variables| read::value(source, variables));
        BodySpec::Graphql(GraphqlBody {
            document: document.to_owned(),
            variables: variables.unwrap_or_else(|| TemplateValue::Map(IndexMap::new())),
            operation_name: project::text(body, OPERATION_NAME).map(str::to_owned),
        })
    }

    fn path(&self, source: &Source<'_>, api: &Node) -> Option<Template> {
        let filed = source.root.get("graphql").and_then(|graphql| graphql.get("path"));
        let written = api.get("path").or(filed).and_then(|path| read::template(source, path));
        Some(written.unwrap_or_else(|| Template { parts: vec![Part::Literal(DEFAULT_PATH.to_owned())] }))
    }

    fn templated_body_keys(&self) -> Option<&'static [&'static str]> {
        Some(TEMPLATED)
    }

    fn check(&self, api: &Node) -> Vec<Diagnostic> {
        let Some(body) = api.get("body") else { return Vec::new() };
        let unknown = body.entries().iter().filter(|entry| !BODY_KEYS.contains(&entry.key.as_str()));
        let mut found: Vec<Diagnostic> = unknown.map(unknown_key).collect();
        found.extend(body.get(QUERY).and_then(document));
        found.extend(body.get(OPERATION_NAME).and_then(operation_name));
        found
    }
}

/// V2213 for a key a `graphql` body does not take, with the close one as its fix.
fn unknown_key(entry: &Entry) -> Diagnostic {
    let message = format!("`{}` does not apply to a `graphql` api's `body`", entry.key);
    let diagnostic = Diagnostic::new(Code::V2213, entry.key_span, message).label("not a `graphql` body key");
    suggest::hint(diagnostic, &Misspelling { word: &entry.key, span: entry.key_span, known: BODY_KEYS })
}

/// V2302 for a `query` that is not the document written as text. A `$file` document is not read
/// yet, so it is refused rather than sent as a POST without a body.
fn document(query: &Node) -> Option<Diagnostic> {
    if query.as_str().is_some() {
        return None;
    }
    if query.get("$file").is_some() {
        let message = "a `graphql` api cannot read its `query` from a `$file` yet";
        let diagnostic = Diagnostic::new(Code::V2302, query.span, message).label("a `$file` document");
        return Some(diagnostic.help("write the document inline, under `query: |`"));
    }
    let message = format!("`query` takes a GraphQL document as text, found {}", shape_check::found(query));
    let diagnostic = Diagnostic::new(Code::V2302, query.span, message).label("not a document");
    Some(diagnostic.help("write the document as text, such as `query: '{ me { id } }'`"))
}

/// V2302 for an `operationName` that is not text, or holds a template. It names an operation in
/// the document, which is read as written (D344), so it is read as written too.
fn operation_name(name: &Node) -> Option<Diagnostic> {
    let message = match name.as_str() {
        Some(text) if !text.contains("${") => return None,
        Some(_) => "`operationName` takes the operation's name as written, not a template".to_owned(),
        None => format!("`operationName` takes the operation's name as text, found {}", shape_check::found(name)),
    };
    let diagnostic = Diagnostic::new(Code::V2302, name.span, message).label("not an operation name");
    let help = "write the name the document gives the operation, such as `GetUser`; values go in `variables`";
    Some(diagnostic.help(help))
}

#[cfg(test)]
#[path = "graphql_tests.rs"]
mod tests;
