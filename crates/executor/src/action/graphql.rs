//! The runtime of a `graphql` api (D343): a JSON `POST` of `{query, variables, operationName}`.
//! The response stays the HTTP one; `expect.errors` and the normalised view come later.

use http::HeaderValue;
use http::header::ACCEPT;
use serde_json::{Map, Value};
use vc_model::{Action, ActionId, Api, Exchange, Failure, GraphqlBody, Protocol, RenderedRequest};

use super::protocol::ProtocolRuntime;
use super::request;
use crate::Render;

/// The `accept` a `graphql` call sends when no level sets one (GraphQL over HTTP).
const ACCEPTS: &str = "application/graphql-response+json, application/json;q=0.9";

/// `graphql`: the request [`request::rendered`] builds, with [`ACCEPTS`] unless a level writes
/// `accept`; a null that removes it wins.
#[derive(Debug)]
pub struct GraphqlRuntime;

impl ProtocolRuntime for GraphqlRuntime {
    fn protocol(&self) -> Protocol {
        Protocol::Graphql
    }

    fn request(&self, render: &Render<'_>, api: &Api, call: (ActionId, &Action)) -> Result<RenderedRequest, Failure> {
        let mut request = request::rendered(render, api, call)?;
        if !request::mentions(render, (api, call.1), &ACCEPT) {
            drop(request.headers.insert(ACCEPT, HeaderValue::from_static(ACCEPTS)));
        }
        Ok(request)
    }

    fn response(&self, exchange: &Exchange) -> Value {
        request::received(exchange)
    }
}

/// The JSON a `graphql` api sends: its document as written, the rendered `variables`, and its
/// `operationName` when it sets one.
pub fn payload(body: &GraphqlBody, variables: Value) -> Value {
    let mut payload = Map::new();
    drop(payload.insert("query".to_owned(), Value::String(body.document.clone())));
    drop(payload.insert("variables".to_owned(), variables));
    if let Some(name) = &body.operation_name {
        drop(payload.insert("operationName".to_owned(), Value::String(name.clone())));
    }
    Value::Object(payload)
}

#[cfg(test)]
#[path = "graphql_tests.rs"]
mod tests;
