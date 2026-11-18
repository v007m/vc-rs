//! The compile rules of a `graphql` api (D343): it sends `POST` with its `query`, `variables`
//! and `operationName`, to its own `path`, else its file's `graphql.path`, else `/graphql`.

use indexmap::IndexMap;
use vc_model::{BodySpec, GraphqlBody, Method, Part, Protocol, Template, TemplateValue};
use vc_parser::Node;

use super::protocol::ProtocolRules;
use super::read;
use crate::link::project;
use crate::link::project::Source;

/// The `path` of a `graphql` api when neither it nor its file's `graphql.path` sets one.
const DEFAULT_PATH: &str = "/graphql";

/// The `body` key that holds the variables, the only part of the body that is a template.
const VARIABLES: &str = "variables";

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
        let Some(document) = body.get("query").and_then(Node::as_str) else { return BodySpec::None };
        let variables = body.get(VARIABLES).and_then(|variables| read::value(source, variables));
        BodySpec::Graphql(GraphqlBody {
            document: document.to_owned(),
            variables: variables.unwrap_or_else(|| TemplateValue::Map(IndexMap::new())),
            operation_name: project::text(body, "operationName").map(str::to_owned),
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
}

#[cfg(test)]
#[path = "graphql_tests.rs"]
mod tests;
