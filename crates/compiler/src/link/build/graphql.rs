//! The compile rules of a `graphql` api (D343): it sends `POST` with its `query`, `variables`
//! and `operationName`, to its own `path`, else its file's `graphql.path`, else `/graphql`.

use indexmap::IndexMap;
use vc_diagnostics::{Code, Diagnostic, Fix, TextEdit};
use vc_model::{BodySpec, FILE_KEY, GraphqlBody, Method, Part, Protocol, Template, TemplateValue};
use vc_parser::{Entry, Node};

use super::protocol::ProtocolRules;
use super::read;
use crate::graphql::{self, OPERATION_NAME, QUERY, VARIABLES};
use crate::link::data::Data;
use crate::link::project;
use crate::link::project::Source;
use crate::shape_check;
use crate::suggest::{self, Misspelling};

/// The `path` of a `graphql` api when neither it nor its file's `graphql.path` sets one.
const DEFAULT_PATH: &str = "/graphql";

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

    fn body(&self, (source, data): (&Source<'_>, &Data<'_>), api: &Node) -> BodySpec {
        let Some(body) = api.get("body") else { return BodySpec::None };
        let Some(query) = body.get(QUERY) else { return BodySpec::None };
        let filed = || graphql::file(source, query).and_then(|path| data.document(&path));
        let Some(document) = query.as_str().or_else(filed) else { return BodySpec::None };
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

    fn check(&self, source: &Source<'_>, api: &Node) -> Vec<Diagnostic> {
        let Some(body) = api.get("body") else { return Vec::new() };
        let unknown = body.entries().iter().filter(|entry| !BODY_KEYS.contains(&entry.key.as_str()));
        let mut found: Vec<Diagnostic> = unknown.map(unknown_key).collect();
        found.extend(body.get(QUERY).map(|query| document(source, query)).unwrap_or_default());
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

/// V2302 for a `query` that is neither the document as text nor exactly `{$file: …}` naming a
/// `.graphql` or `.gql` file by a literal path.
fn document(source: &Source<'_>, query: &Node) -> Vec<Diagnostic> {
    if query.as_str().is_some() {
        return Vec::new();
    }
    let Some(file) = query.get(FILE_KEY) else {
        let message = format!("`query` takes a GraphQL document as text, found {}", shape_check::found(query));
        let diagnostic = Diagnostic::new(Code::V2302, query.span, message).label("not a document");
        return vec![
            diagnostic.help("write the document as text, such as `query: '{ me { id } }'`, or name its `$file`"),
        ];
    };
    let extra = query.entries().iter().filter(|entry| entry.key != FILE_KEY);
    let mut found: Vec<Diagnostic> = extra.map(|entry| extra_key(source, entry)).collect();
    let path = graphql::literal_path(file).filter(|path| graphql::is_document(path));
    let message = "a `query` `$file` takes the literal path of a `.graphql` or `.gql` file";
    let diagnostic = Diagnostic::new(Code::V2302, file.span, message).label("not a document file");
    found.extend(path.is_none().then(|| diagnostic.help("name the file as written, such as `$file: getUser.graphql`")));
    found
}

/// V2302 for a key beside `$file` in a `query` map, which would leave the api with no document;
/// its fix removes the key when it has lines of its own.
fn extra_key(source: &Source<'_>, entry: &Entry) -> Diagnostic {
    let message = format!("a `query` `$file` map takes only `$file`, found `{}`", entry.key);
    let diagnostic = Diagnostic::new(Code::V2302, entry.key_span, message).label("not `$file`");
    let diagnostic = diagnostic.help("write `query: {$file: <path>}` with no other key");
    let Some(range) = project::whole_lines(source.text, entry) else { return diagnostic };
    let edit = TextEdit { file: source.file, range, new_text: String::new() };
    diagnostic.fix(Fix::certain(format!("Remove `{}`", entry.key), vec![edit]))
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
